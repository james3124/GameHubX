//! Dependency-free extraction of the first embedded icon from a Windows PE
//! executable (`.exe`).
//!
//! The reference launcher uses a game's embedded application icon as its cover
//! art. This module implements just enough of the PE/COFF and resource-format
//! parsing needed to find that icon, with no external crates and no `unsafe`.
//!
//! # Supported icon formats
//!
//! Icons stored in the `RT_ICON` resource are device-independent bitmaps
//! (DIBs). This decoder supports 32-bit BGRA and 24-bit BGR `BITMAPINFOHEADER`
//! images (including V4/V5-sized headers), with the doubled height convention
//! used by icons (XOR bitmap followed by a 1-bit AND mask). Rows are stored
//! bottom-up in the DIB and are returned top-down as RGBA.
//!
//! PNG-compressed icons (entries beginning with `\x89PNG`, used by some newer
//! Windows toolchains for very large icons) are **not** supported and yield
//! [`None`], as are other bit depths/compressions. Callers should fall back to
//! a placeholder cover in that case.
//!
//! Every offset and length is validated before use; malformed or truncated
//! input always returns [`None`] rather than panicking.

/// A decoded RGBA icon image, with rows ordered top-to-bottom.
pub struct PeIcon {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes in RGBA order.
    pub rgba: Vec<u8>,
}

/// Resource directory entry: an integer ID (or named offset) and a link to
/// either a subdirectory (high bit set) or a data entry.
#[derive(Clone, Copy)]
struct ResEntry {
    name: u32,
    target: u32,
}

/// `IMAGE_RESOURCE_DATA_ENTRY`: the RVA and byte size of raw resource data.
#[derive(Clone, Copy)]
struct ResData {
    rva: u32,
    size: u32,
}

/// Parsed section header (only the fields needed for RVA translation).
struct Section {
    name: [u8; 8],
    virtual_address: u32,
    raw_size: u32,
    raw_offset: u32,
}

const DIR_HIGH_BIT: u32 = 0x8000_0000;
const DIR_OFFSET_MASK: u32 = 0x7fff_ffff;
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

/// Read a `len`-byte window without ever overflowing or going out of bounds.
fn window(bytes: &[u8], off: usize, len: usize) -> Option<&[u8]> {
    let end = off.checked_add(len)?;
    bytes.get(off..end)
}

fn u16le(bytes: &[u8], off: usize) -> Option<u16> {
    let b = window(bytes, off, 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn u32le(bytes: &[u8], off: usize) -> Option<u32> {
    let b = window(bytes, off, 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Map an RVA to a file slice, preferring the `.rsrc` section when the RVA
/// falls inside it (the resource tree normally lives there).
fn rva_region<'a>(bytes: &'a [u8], sections: &[Section], rva: u32, size: u32) -> Option<&'a [u8]> {
    fn region_in<'a>(bytes: &'a [u8], s: &Section, rva: u32, size: u32) -> Option<&'a [u8]> {
        if rva < s.virtual_address {
            return None;
        }
        let delta = u64::from(rva - s.virtual_address);
        if delta >= u64::from(s.raw_size) {
            return None;
        }
        let start = usize::try_from(u64::from(s.raw_offset).checked_add(delta)?).ok()?;
        let available = u64::from(s.raw_size)
            .checked_sub(delta)?
            .min(bytes.len() as u64);
        let len = if size == 0 {
            available
        } else {
            u64::from(size).min(available)
        };
        let len = usize::try_from(len).ok()?;
        bytes.get(start..start.checked_add(len)?)
    }

    for s in sections.iter().filter(|s| s.name.starts_with(b".rsrc")) {
        if let Some(region) = region_in(bytes, s, rva, size) {
            return Some(region);
        }
    }
    for s in sections {
        if let Some(region) = region_in(bytes, s, rva, size) {
            return Some(region);
        }
    }
    None
}

/// Read the entries immediately following an `IMAGE_RESOURCE_DIRECTORY`.
fn resource_entries(rsrc: &[u8], dir: usize) -> Option<Vec<ResEntry>> {
    let named = u16le(rsrc, dir.checked_add(12)?)? as usize;
    let ids = u16le(rsrc, dir.checked_add(14)?)? as usize;
    let count = named.checked_add(ids)?;
    let start = dir.checked_add(16)?;
    // Bound the count before allocating so a corrupt directory cannot request
    // an enormous Vec.
    if count > rsrc.len().checked_sub(start)? / 8 {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let off = start.checked_add(i.checked_mul(8)?)?;
        out.push(ResEntry {
            name: u32le(rsrc, off)?,
            target: u32le(rsrc, off.checked_add(4)?)?,
        });
    }
    Some(out)
}

/// Find the subdirectory for a top-level resource type ID (e.g. 3 or 14).
fn type_subdir(rsrc: &[u8], id: u32) -> Option<usize> {
    let entry = resource_entries(rsrc, 0)?
        .into_iter()
        .find(|e| e.name & DIR_HIGH_BIT == 0 && e.name == id && e.target & DIR_HIGH_BIT != 0)?;
    Some((entry.target & DIR_OFFSET_MASK) as usize)
}

/// Walk type -> name -> language for a resource type, collecting every data
/// entry as `(name/id, data)`.
fn collect_resources(rsrc: &[u8], type_dir: usize) -> Vec<(u32, ResData)> {
    let mut out = Vec::new();
    let Some(names) = resource_entries(rsrc, type_dir) else {
        return out;
    };
    for name in names {
        if name.target & DIR_HIGH_BIT == 0 {
            continue;
        }
        let lang_dir = (name.target & DIR_OFFSET_MASK) as usize;
        let Some(langs) = resource_entries(rsrc, lang_dir) else {
            continue;
        };
        for lang in langs {
            if lang.target & DIR_HIGH_BIT != 0 {
                continue;
            }
            let off = lang.target as usize;
            let Some(off_size) = off.checked_add(4) else {
                continue;
            };
            let (Some(rva), Some(size)) = (u32le(rsrc, off), u32le(rsrc, off_size)) else {
                continue;
            };
            if size == 0 {
                continue;
            }
            out.push((name.name, ResData { rva, size }));
        }
    }
    out
}

/// Pick the icon ID referenced by the largest entry of a `GRPICONDIR`.
fn largest_group_icon_id(group: &[u8]) -> Option<u16> {
    let reserved = u16le(group, 0)?;
    let kind = u16le(group, 2)?;
    let count = u16le(group, 4)? as usize;
    if reserved != 0 || kind != 1 || count == 0 {
        return None;
    }
    let mut best: Option<(u32, u16)> = None;
    for i in 0..count {
        let off = 6usize.checked_add(i.checked_mul(14)?)?;
        let w = *group.get(off)?;
        let h = *group.get(off.checked_add(1)?)?;
        let id = u16le(group, off.checked_add(12)?)?;
        // In an ICO, a zero dimension means 256.
        let width = if w == 0 { 256u32 } else { u32::from(w) };
        let height = if h == 0 { 256u32 } else { u32::from(h) };
        let score = width.max(height);
        if best.map_or(true, |(best_score, _)| score > best_score) {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}

/// Decode a 32-bit BGRA or 24-bit BGR icon DIB into top-down RGBA.
fn decode_dib(data: &[u8]) -> Option<PeIcon> {
    // PNG-compressed icons are intentionally unsupported (see module docs).
    if data.starts_with(b"\x89PNG") {
        return None;
    }

    let header_size = u32le(data, 0)? as usize;
    if header_size < 40 || header_size > data.len() {
        return None;
    }
    let raw_width = u32le(data, 4)? as i32;
    let raw_height = u32le(data, 8)? as i32;
    let bit_count = u16le(data, 14)?;
    let compression = u32le(data, 16)?;
    if compression != 0 || raw_width <= 0 || raw_height <= 0 || raw_height % 2 != 0 {
        return None;
    }
    let width = raw_width as u32;
    // Icons store the XOR bitmap height plus the AND mask height.
    let height = (raw_height / 2) as u32;
    let bpp: usize = match bit_count {
        24 => 24,
        32 => 32,
        _ => return None,
    };

    let w = u64::from(width);
    let h = u64::from(height);
    let xor_stride = w.checked_mul(bpp as u64)?.checked_add(31)? / 32 * 4;
    let xor_len = xor_stride.checked_mul(h)?;
    let mask_stride = w.checked_add(31)? / 32 * 4;
    let mask_len = mask_stride.checked_mul(h)?;

    let xor_start = header_size as u64;
    let mask_start = xor_start.checked_add(xor_len)?;
    if mask_start > data.len() as u64 {
        return None;
    }
    // Some icons omit the AND mask entirely; only use it when fully present.
    let has_mask = mask_start.checked_add(mask_len)? <= data.len() as u64;

    let xor_start = header_size;
    let mask_start = usize::try_from(mask_start).ok()?;
    let xor_stride = usize::try_from(xor_stride).ok()?;
    let mask_stride = usize::try_from(mask_stride).ok()?;

    let row_bytes = usize::try_from(width).ok()?.checked_mul(4)?;
    let mut rgba = vec![0u8; usize::try_from(height).ok()?.checked_mul(row_bytes)?];

    for y in 0..height as usize {
        // DIB rows are bottom-up; emit them top-down.
        let src_row = height as usize - 1 - y;
        let xor_row = xor_start.checked_add(src_row.checked_mul(xor_stride)?)?;
        let mask_row = mask_start.checked_add(src_row.checked_mul(mask_stride)?)?;
        for x in 0..width as usize {
            let p = xor_row.checked_add(x.checked_mul(bpp / 8)?)?;
            let blue = *data.get(p)?;
            let green = *data.get(p.checked_add(1)?)?;
            let red = *data.get(p.checked_add(2)?)?;
            let mut alpha = if bpp == 32 {
                *data.get(p.checked_add(3)?)?
            } else {
                255
            };
            if has_mask {
                let mask_byte = *data.get(mask_row.checked_add(x / 8)?)?;
                if mask_byte & (0x80 >> (x % 8)) != 0 {
                    alpha = 0;
                }
            }
            let out = y.checked_mul(row_bytes)?.checked_add(x.checked_mul(4)?)?;
            rgba[out] = red;
            rgba[out + 1] = green;
            rgba[out + 2] = blue;
            rgba[out + 3] = alpha;
        }
    }

    Some(PeIcon {
        width,
        height,
        rgba,
    })
}

/// Extract the first embedded icon from a PE executable as RGBA pixels.
///
/// Returns `None` if `bytes` is not a well-formed PE image, contains no usable
/// icon resource, or contains an icon format this module does not decode
/// (e.g. PNG-compressed icons).
pub fn extract_first_icon(bytes: &[u8]) -> Option<PeIcon> {
    // DOS header: "MZ" and the PE header offset at 0x3C.
    window(bytes, 0, 0x40)?;
    if bytes[0] != b'M' || bytes[1] != b'Z' {
        return None;
    }
    let pe_off = u32le(bytes, 0x3c)? as usize;
    let signature = window(bytes, pe_off, 4)?;
    if signature[0] != b'P' || signature[1] != b'E' || signature[2] != 0 || signature[3] != 0 {
        return None;
    }

    // COFF file header starts just after the PE signature.
    let coff = pe_off.checked_add(4)?;
    let section_count = u16le(bytes, coff.checked_add(2)?)? as usize;
    let optional_size = u16le(bytes, coff.checked_add(16)?)? as usize;
    let optional = coff.checked_add(20)?;
    if optional_size < 2 {
        return None;
    }

    // Data directories: offset 96 in PE32, 112 in PE32+. Entry 2 is resources.
    let (directory_base, count_offset) = match u16le(bytes, optional)? {
        0x10b => (96usize, 92usize),
        0x20b => (112usize, 108usize),
        _ => return None,
    };
    if optional_size < directory_base + 3 * 8 {
        return None;
    }
    let directory_count = u32le(bytes, optional.checked_add(count_offset)?)? as usize;
    if directory_count < 3 {
        return None;
    }
    let resource_rva = u32le(bytes, optional.checked_add(directory_base + 2 * 8)?)?;
    if resource_rva == 0 {
        return None;
    }

    // Section table begins immediately after the optional header.
    let section_table = optional.checked_add(optional_size)?;
    let mut sections = Vec::new();
    for i in 0..section_count {
        let off = section_table.checked_add(i.checked_mul(40)?)?;
        if off.checked_add(40)? > bytes.len() {
            return None;
        }
        let mut name = [0u8; 8];
        name.copy_from_slice(window(bytes, off, 8)?);
        sections.push(Section {
            name,
            virtual_address: u32le(bytes, off.checked_add(12)?)?,
            raw_size: u32le(bytes, off.checked_add(16)?)?,
            raw_offset: u32le(bytes, off.checked_add(20)?)?,
        });
    }

    // Resource directory offsets are relative to the start of the resource
    // data (normally the .rsrc section), located via the section table above.
    let rsrc = rva_region(bytes, &sections, resource_rva, 0)?;
    let icon_type = type_subdir(rsrc, RT_ICON)?;
    let icons = collect_resources(rsrc, icon_type);
    if icons.is_empty() {
        return None;
    }

    // Prefer the group icon directory: it tells us which icon is the largest.
    let chosen = type_subdir(rsrc, RT_GROUP_ICON)
        .and_then(|group_type| collect_resources(rsrc, group_type).into_iter().next())
        .and_then(|(_, group)| rva_region(bytes, &sections, group.rva, group.size))
        .and_then(largest_group_icon_id)
        .and_then(|id| {
            icons
                .iter()
                .find(|(name, _)| *name & DIR_HIGH_BIT == 0 && *name == u32::from(id))
                .map(|(_, data)| *data)
        })
        .or_else(|| icons.first().map(|(_, data)| *data))?;

    let icon_bytes = rva_region(bytes, &sections, chosen.rva, chosen.size)?;
    decode_dib(icon_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSRC_OFFSET: usize = 0x200;
    const RSRC_LEN: usize = 256;
    const DIB_LEN: u32 = 64; // 40-byte header + 16 bytes XOR + 8 bytes AND mask
    const DIB_OFFSET: usize = 0xb4;

    fn w16(b: &mut [u8], off: usize, v: u16) {
        b[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn w32(b: &mut [u8], off: usize, v: u32) {
        b[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Build a resource section with one RT_ICON (id 1) and one
    /// RT_GROUP_ICON (id 1) pointing at it.
    fn build_rsrc() -> Vec<u8> {
        let mut r = vec![0u8; RSRC_LEN];

        // Root directory: two ID entries (type 3 and type 14).
        w16(&mut r, 12, 0); // NumberOfNamedEntries
        w16(&mut r, 14, 2); // NumberOfIdEntries
        w32(&mut r, 0x10, RT_ICON);
        w32(&mut r, 0x14, DIR_HIGH_BIT | 0x20);
        w32(&mut r, 0x18, RT_GROUP_ICON);
        w32(&mut r, 0x1c, DIR_HIGH_BIT | 0x60);

        // RT_ICON type -> name 1 -> language 0x409 -> data entry.
        w16(&mut r, 0x20 + 14, 1);
        w32(&mut r, 0x30, 1);
        w32(&mut r, 0x34, DIR_HIGH_BIT | 0x38);
        w16(&mut r, 0x38 + 14, 1);
        w32(&mut r, 0x48, 0x409);
        w32(&mut r, 0x4c, 0x50);
        w32(&mut r, 0x50, 0x1000 + DIB_OFFSET as u32); // data RVA
        w32(&mut r, 0x54, DIB_LEN); // data size

        // RT_GROUP_ICON type -> name 1 -> language 0x409 -> data entry.
        w16(&mut r, 0x60 + 14, 1);
        w32(&mut r, 0x70, 1);
        w32(&mut r, 0x74, DIR_HIGH_BIT | 0x78);
        w16(&mut r, 0x78 + 14, 1);
        w32(&mut r, 0x88, 0x409);
        w32(&mut r, 0x8c, 0x90);
        w32(&mut r, 0x90, 0x1000 + 0xa0); // group blob RVA
        w32(&mut r, 0x94, 20); // group blob size

        // GRPICONDIR / GRPICONDIRENTRY.
        w16(&mut r, 0xa0, 0); // idReserved
        w16(&mut r, 0xa2, 1); // idType = ICON
        w16(&mut r, 0xa4, 1); // idCount
        let e = 0xa6;
        r[e] = 2; // bWidth
        r[e + 1] = 2; // bHeight
        w16(&mut r, e + 4, 1); // wPlanes
        w16(&mut r, e + 6, 32); // wBitCount
        w32(&mut r, e + 8, DIB_LEN); // dwBytesInRes
        w16(&mut r, e + 12, 1); // nID

        // BITMAPINFOHEADER for a 2x2, 32-bpp icon (height doubled to 4).
        let d = DIB_OFFSET;
        w32(&mut r, d, 40); // biSize
        w32(&mut r, d + 4, 2); // biWidth
        w32(&mut r, d + 8, 4); // biHeight
        w16(&mut r, d + 12, 1); // biPlanes
        w16(&mut r, d + 14, 32); // biBitCount
        w32(&mut r, d + 16, 0); // BI_RGB
        w32(&mut r, d + 20, 16); // biSizeImage

        // XOR rows, bottom-up: bottom row = blue + transparent white,
        // top row = red + half-transparent green (BGRA bytes).
        let pixels: [u8; 16] = [
            255, 0, 0, 255, 255, 255, 255, 0, // bottom
            0, 0, 255, 255, 0, 255, 0, 128, // top
        ];
        r[d + 40..d + 56].copy_from_slice(&pixels);
        // AND mask: two 4-byte rows at d + 56 .. d + 64, all zero so the
        // 32-bit alpha channel determines transparency.
        r
    }

    /// Build a minimal PE32 image with a single `.rsrc` section containing
    /// the synthetic icon from [`build_rsrc`].
    fn synthetic_pe() -> Vec<u8> {
        let rsrc = build_rsrc();
        let mut b = vec![0u8; RSRC_OFFSET + rsrc.len()];
        b[0] = b'M';
        b[1] = b'Z';
        w32(&mut b, 0x3c, 0x80); // e_lfanew
        b[0x80..0x84].copy_from_slice(b"PE\0\0");

        let coff = 0x84;
        w16(&mut b, coff, 0x14c); // IMAGE_FILE_MACHINE_I386
        w16(&mut b, coff + 2, 1); // NumberOfSections
        w16(&mut b, coff + 16, 224); // SizeOfOptionalHeader (PE32)
        w16(&mut b, coff + 18, 0x102); // Characteristics

        let opt = coff + 20;
        w16(&mut b, opt, 0x10b); // PE32 magic
        w32(&mut b, opt + 92, 16); // NumberOfRvaAndSizes
        w32(&mut b, opt + 96 + 2 * 8, 0x1000); // resource data directory RVA
        w32(&mut b, opt + 96 + 2 * 8 + 4, rsrc.len() as u32);

        let sec = opt + 224;
        b[sec..sec + 5].copy_from_slice(b".rsrc");
        w32(&mut b, sec + 8, rsrc.len() as u32); // VirtualSize
        w32(&mut b, sec + 12, 0x1000); // VirtualAddress
        w32(&mut b, sec + 16, rsrc.len() as u32); // SizeOfRawData
        w32(&mut b, sec + 20, RSRC_OFFSET as u32); // PointerToRawData

        b[RSRC_OFFSET..].copy_from_slice(&rsrc);
        b
    }

    #[test]
    fn extracts_32bit_icon_dimensions_and_pixels() {
        let icon = extract_first_icon(&synthetic_pe()).expect("should extract icon");
        assert_eq!(icon.width, 2);
        assert_eq!(icon.height, 2);
        assert_eq!(
            icon.rgba,
            vec![
                255, 0, 0, 255, // top-left: opaque red
                0, 255, 0, 128, // top-right: green, 50% alpha
                0, 0, 255, 255, // bottom-left: opaque blue
                255, 255, 255, 0, // bottom-right: transparent white
            ]
        );
    }

    #[test]
    fn falls_back_to_first_icon_without_group() {
        let mut pe = synthetic_pe();
        // Change the group type ID so no RT_GROUP_ICON is found.
        w32(&mut pe, RSRC_OFFSET + 0x18, 99);
        let icon = extract_first_icon(&pe).expect("should fall back to RT_ICON");
        assert_eq!((icon.width, icon.height), (2, 2));
    }

    #[test]
    fn and_mask_marks_pixel_transparent() {
        let mut pe = synthetic_pe();
        // First AND-mask bit corresponds to the bottom-left (blue) pixel.
        pe[RSRC_OFFSET + DIB_OFFSET + 40 + 16] = 0x80;
        let icon = extract_first_icon(&pe).expect("should extract icon");
        assert_eq!(icon.rgba[11], 0); // bottom-left alpha forced to 0
        assert_eq!(icon.rgba[15], 0); // bottom-right was already transparent
    }

    #[test]
    fn decodes_24bit_bgr_with_mask() {
        let mut pe = synthetic_pe();
        // Group entry: 24 bpp.
        w16(&mut pe, RSRC_OFFSET + 0xa6 + 6, 24);
        let d = RSRC_OFFSET + DIB_OFFSET;
        w16(&mut pe, d + 14, 24); // biBitCount
                                  // XOR rows (bottom-up, 2 padding bytes per 4-byte-aligned row):
                                  // bottom = blue, white; top = red, green.
        let pixels: [u8; 16] = [
            255, 0, 0, 255, 255, 255, 0, 0, // bottom
            0, 0, 255, 0, 255, 0, 0, 0, // top
        ];
        pe[d + 40..d + 56].copy_from_slice(&pixels);
        // AND mask bit for x=1 (bottom-right, white) -> transparent.
        pe[d + 56] = 0x40;

        let icon = extract_first_icon(&pe).expect("should extract 24-bit icon");
        assert_eq!((icon.width, icon.height), (2, 2));
        assert_eq!(
            icon.rgba,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, // top row
                0, 0, 255, 255, 255, 255, 255, 0, // bottom row, white masked out
            ]
        );
    }

    #[test]
    fn png_compressed_icon_returns_none() {
        let mut pe = synthetic_pe();
        pe[RSRC_OFFSET + DIB_OFFSET..RSRC_OFFSET + DIB_OFFSET + 4].copy_from_slice(b"\x89PNG");
        assert!(extract_first_icon(&pe).is_none());
    }

    #[test]
    fn empty_returns_none() {
        assert!(extract_first_icon(&[]).is_none());
    }

    #[test]
    fn truncated_input_returns_none() {
        let pe = synthetic_pe();
        for n in [1, 2, 0x3c, 0x80, 0x84, 0x98, 0x178, 0x200, pe.len() - 1] {
            assert!(extract_first_icon(&pe[..n]).is_none(), "prefix length {n}");
        }
    }

    #[test]
    fn malformed_headers_return_none() {
        let bad_mz = {
            let mut pe = synthetic_pe();
            pe[0] = b'X';
            pe
        };
        assert!(extract_first_icon(&bad_mz).is_none());

        let bad_signature = {
            let mut pe = synthetic_pe();
            pe[0x80] = b'X';
            pe
        };
        assert!(extract_first_icon(&bad_signature).is_none());

        let huge_lfanew = {
            let mut pe = synthetic_pe();
            w32(&mut pe, 0x3c, u32::MAX);
            pe
        };
        assert!(extract_first_icon(&huge_lfanew).is_none());

        let bad_magic = {
            let mut pe = synthetic_pe();
            w16(&mut pe, 0x98, 0x9999);
            pe
        };
        assert!(extract_first_icon(&bad_magic).is_none());

        let no_resources = {
            let mut pe = synthetic_pe();
            w32(&mut pe, 0x98 + 96 + 2 * 8, 0);
            pe
        };
        assert!(extract_first_icon(&no_resources).is_none());
    }
}
