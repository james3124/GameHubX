//! Runtime component installer: fetch -> size/hash verify -> safe extract -> mark.
//!
//! A component is never marked installed unless both the declared size and the
//! SHA-256 verify. Extraction rejects absolute paths, traversal, escaping
//! symlinks/hardlinks and unknown entry types.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::layout::{is_within, RuntimeLayout};
use crate::manifest::Component;
use crate::verify;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError {
    NoSpace,
    SizeMismatch,
    HashMismatch,
    UnsafeEntryPath,
    UnsafeEntryTarget,
    UnsupportedEntryType,
    UnsafeComponentName,
    EntryTooLarge,
    VersionConflict,
    Io,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for InstallError {}

impl From<std::io::Error> for InstallError {
    fn from(_: std::io::Error) -> Self {
        InstallError::Io
    }
}

const MAX_ENTRY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub trait ComponentSource {
    fn fetch(&self, component: &Component, dest: &Path) -> Result<u64, InstallError>;
}

/// HTTPS downloader; the manifest validator rejects non-HTTPS URLs.
pub struct HttpSource;

impl ComponentSource for HttpSource {
    fn fetch(&self, component: &Component, dest: &Path) -> Result<u64, InstallError> {
        let response = ureq::get(&component.url)
            .call()
            .map_err(|_| InstallError::Io)?;
        let mut file = fs::File::create(dest)?;
        let mut reader = response.into_reader().take(component.size_bytes + 1);
        std::io::copy(&mut reader, &mut file)?;
        file.flush()?;
        file.sync_all()?;
        Ok(file.metadata()?.len())
    }
}

/// Local pack source: archives are `<dir>/<component-name>.tar.gz`.
pub struct LocalSource {
    dir: PathBuf,
}

impl LocalSource {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl ComponentSource for LocalSource {
    fn fetch(&self, component: &Component, dest: &Path) -> Result<u64, InstallError> {
        if !safe_component_name(&component.name) {
            return Err(InstallError::UnsafeComponentName);
        }
        let source = self.dir.join(format!("{}.tar.gz", component.name));
        Ok(fs::copy(source, dest)?)
    }
}

fn safe_component_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledMarker {
    pub name: String,
    pub version: String,
    pub sha256: String,
}

fn marker_path(layout: &RuntimeLayout, name: &str) -> PathBuf {
    layout
        .component_dir(name)
        .parent()
        .unwrap_or(layout.root.as_path())
        .join(format!("{name}.installed"))
}

pub fn installed_marker(layout: &RuntimeLayout, name: &str) -> Option<InstalledMarker> {
    let text = fs::read_to_string(marker_path(layout, name)).ok()?;
    let mut parts = text.split_whitespace();
    Some(InstalledMarker {
        name: parts.next()?.to_string(),
        version: parts.next()?.to_string(),
        sha256: parts.next()?.to_string(),
    })
}

fn write_marker(layout: &RuntimeLayout, component: &Component) -> Result<(), InstallError> {
    let path = marker_path(layout, &component.name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "{} {} {}",
            component.name, component.version, component.sha256
        ),
    )?;
    Ok(())
}

pub fn free_bytes(path: &Path) -> u64 {
    let mut probe = path.to_path_buf();
    loop {
        match fs2::free_space(&probe) {
            Ok(free) => return free,
            Err(_) => {
                if !probe.pop() {
                    return 0;
                }
            }
        }
    }
}

pub fn preflight_space(
    layout: &RuntimeLayout,
    components: &[Component],
    safety_margin_bytes: u64,
) -> Result<(), InstallError> {
    let needed: u64 = components
        .iter()
        .map(|component| component.size_bytes)
        .sum::<u64>()
        + safety_margin_bytes;
    if free_bytes(&layout.root) < needed {
        return Err(InstallError::NoSpace);
    }
    Ok(())
}

pub fn install_component(
    layout: &RuntimeLayout,
    source: &dyn ComponentSource,
    component: &Component,
) -> Result<(), InstallError> {
    if let Some(marker) = installed_marker(layout, &component.name) {
        if marker.version == component.version && marker.sha256 == component.sha256 {
            return Ok(());
        }
        return Err(InstallError::VersionConflict);
    }

    preflight_space(layout, std::slice::from_ref(component), 64 * 1024 * 1024)?;

    let temp_dir = layout.root.join("tmp");
    fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join(format!("{}.download", component.name));
    let fetched = source.fetch(component, &temp_file)?;

    if fetched != component.size_bytes {
        let _ = fs::remove_file(&temp_file);
        return Err(InstallError::SizeMismatch);
    }
    if !verify::verify_file(&temp_file, &component.sha256)? {
        let _ = fs::remove_file(&temp_file);
        return Err(InstallError::HashMismatch);
    }

    let dest = layout.component_dir(&component.name);
    fs::create_dir_all(&dest)?;
    let archive_file = fs::File::open(&temp_file)?;
    let result = extract_tar_gz(archive_file, &dest);
    let _ = fs::remove_file(&temp_file);
    result?;
    write_marker(layout, component)?;
    Ok(())
}

pub fn extract_tar_gz(archive: impl Read, dest: &Path) -> Result<(), InstallError> {
    let gz = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(gz);
    let entries = tar.entries().map_err(|_| InstallError::Io)?;

    for entry in entries {
        let mut entry = entry.map_err(|_| InstallError::Io)?;
        let header = entry.header();

        match header.entry_type() {
            tar::EntryType::Regular
            | tar::EntryType::Symlink
            | tar::EntryType::Link
            | tar::EntryType::Directory => {}
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => continue,
            _ => return Err(InstallError::UnsupportedEntryType),
        }

        let raw_path = entry.path().map_err(|_| InstallError::Io)?.into_owned();
        if raw_path.is_absolute() {
            return Err(InstallError::UnsafeEntryPath);
        }
        let target = dest.join(&raw_path);
        if !is_within(dest, &target) {
            return Err(InstallError::UnsafeEntryPath);
        }

        match header.entry_type() {
            tar::EntryType::Symlink | tar::EntryType::Link => {
                let link_target = header
                    .link_name()
                    .map_err(|_| InstallError::Io)?
                    .ok_or(InstallError::UnsafeEntryTarget)?;
                let resolved = target
                    .parent()
                    .ok_or(InstallError::UnsafeEntryTarget)?
                    .join(link_target);
                if !is_within(dest, &resolved) {
                    return Err(InstallError::UnsafeEntryTarget);
                }
            }
            _ => {}
        }

        let entry_size = header.size().map_err(|_| InstallError::Io)?;
        if entry_size > MAX_ENTRY_BYTES {
            return Err(InstallError::EntryTooLarge);
        }

        match header.entry_type() {
            tar::EntryType::Directory => {
                fs::create_dir_all(&target)?;
            }
            tar::EntryType::Symlink => {
                let link_target = header
                    .link_name()
                    .map_err(|_| InstallError::Io)?
                    .ok_or(InstallError::UnsafeEntryTarget)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                std::os::unix::fs::symlink(&link_target, &target)?;
                #[cfg(not(unix))]
                return Err(InstallError::UnsupportedEntryType);
            }
            tar::EntryType::Link => {
                let link_target = header
                    .link_name()
                    .map_err(|_| InstallError::Io)?
                    .ok_or(InstallError::UnsafeEntryTarget)?;
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let source_link = dest.join(&link_target);
                fs::hard_link(source_link, &target)?;
            }
            _ => {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut file = fs::File::create(&target)?;
                let mut limited = (&mut entry).take(MAX_ENTRY_BYTES + 1);
                std::io::copy(&mut limited, &mut file)?;
                file.flush()?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RuntimeManifest;
    use std::io::Cursor;

    fn layout(name: &str) -> RuntimeLayout {
        RuntimeLayout::new(
            std::env::temp_dir().join(format!("gamehubx-test-{}-{name}", std::process::id())),
        )
    }

    fn component(name: &str, bytes: &[u8]) -> Component {
        Component {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            url: "https://example.invalid/pack.tar.gz".to_string(),
            sha256: sha256_hex_of(bytes),
            size_bytes: bytes.len() as u64,
        }
    }

    fn sha256_hex_of(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn raw_tar_entry(name: &str, content: &[u8]) -> Vec<u8> {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        let size_octal = format!("{:011o}\0", content.len());
        header[124..124 + size_octal.len()].copy_from_slice(size_octal.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        let chksum = format!("{:06o}\0 ", sum);
        header[148..156].copy_from_slice(chksum.as_bytes());
        let mut out = header.to_vec();
        out.extend_from_slice(content);
        let pad = 512 - (content.len() % 512);
        out.extend(std::iter::repeat_n(0u8, pad));
        out.extend(std::iter::repeat_n(0u8, 1024));
        out
    }

    fn gz(bytes: &[u8]) -> Vec<u8> {
        use flate2::{write::GzEncoder, Compression};
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    struct MemorySource(Vec<u8>);

    impl ComponentSource for MemorySource {
        fn fetch(&self, _component: &Component, dest: &Path) -> Result<u64, InstallError> {
            fs::write(dest, &self.0)?;
            Ok(self.0.len() as u64)
        }
    }

    #[test]
    fn valid_manifest_components_are_installable() {
        let manifest = RuntimeManifest {
            schema: 1,
            pack_version: "2026.11.0".to_string(),
            components: vec![component("wine", b"x")],
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn installs_and_marks_component() {
        let layout = layout("installs");
        let archive = tar_gz(&[("bin/wine64", b"MZ...")]);
        let comp = component("wine", &archive);
        install_component(&layout, &MemorySource(archive.clone()), &comp).expect("install");
        let marker = installed_marker(&layout, "wine").expect("marker");
        assert_eq!(marker.version, "1.0.0");
        assert_eq!(marker.sha256, sha256_hex_of(&archive));
        let installed = layout.component_dir("wine").join("bin/wine64");
        assert_eq!(fs::read(&installed).unwrap(), b"MZ...");
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn second_install_is_idempotent_no_silent_update() {
        let layout = layout("idempotent");
        let archive = tar_gz(&[("bin/wine64", b"MZ")]);
        let comp = component("wine", &archive);
        install_component(&layout, &MemorySource(archive.clone()), &comp).expect("first");
        install_component(&layout, &MemorySource(archive.clone()), &comp).expect("idempotent");
        let other = tar_gz(&[("bin/wine64", b"EVIL")]);
        let bad = component("wine", &other);
        assert_eq!(
            install_component(&layout, &MemorySource(other), &bad),
            Err(InstallError::VersionConflict)
        );
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn hash_mismatch_rejected_and_temp_cleaned() {
        let layout = layout("hash");
        let archive = tar_gz(&[("bin/wine64", b"MZ")]);
        let mut comp = component("wine", &archive);
        comp.sha256 = "0".repeat(64);
        assert_eq!(
            install_component(&layout, &MemorySource(archive), &comp),
            Err(InstallError::HashMismatch)
        );
        assert!(installed_marker(&layout, "wine").is_none());
        let leftovers = fs::read_dir(layout.root.join("tmp"))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(leftovers, 0, "temp download must be cleaned up");
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn size_mismatch_rejected() {
        let layout = layout("size");
        let archive = tar_gz(&[("bin/wine64", b"MZ")]);
        let mut comp = component("wine", &archive);
        comp.size_bytes = archive.len() as u64 + 1;
        assert_eq!(
            install_component(&layout, &MemorySource(archive), &comp),
            Err(InstallError::SizeMismatch)
        );
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn traversal_entry_rejected() {
        let layout = layout("traversal");
        let archive = gz(&raw_tar_entry("../escape.txt", b"gotcha"));
        let comp = component("rootfs", &archive);
        assert_eq!(
            install_component(&layout, &MemorySource(archive), &comp),
            Err(InstallError::UnsafeEntryPath)
        );
        assert!(installed_marker(&layout, "rootfs").is_none());
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn symlink_escape_rejected() {
        let layout = layout("symlink");
        let mut header = [0u8; 512];
        header[..6].copy_from_slice(b"escape");
        header[100..108].copy_from_slice(b"0000777\0");
        header[124..135].copy_from_slice(b"00000000000");
        header[156] = b'2';
        header[157..168].copy_from_slice(b"00000000000");
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[157..157 + 14].copy_from_slice(b"../../escape\0\0");
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        let chksum = format!("{:06o}\0 ", sum);
        header[148..156].copy_from_slice(chksum.as_bytes());
        let mut raw = header.to_vec();
        raw.extend(std::iter::repeat_n(0u8, 1024));
        let archive = gz(&raw);
        let comp = component("rootfs", &archive);
        assert_eq!(
            install_component(&layout, &MemorySource(archive), &comp),
            Err(InstallError::UnsafeEntryTarget)
        );
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn preflight_rejects_when_no_space() {
        let layout = layout("preflight");
        let comp = component("wine", b"x");
        assert_eq!(
            preflight_space(&layout, &[comp], u64::MAX / 2),
            Err(InstallError::NoSpace)
        );
    }

    #[test]
    fn extract_directly_into_dest_works() {
        let dir = std::env::temp_dir().join(format!("gamehubx-extract-{}", std::process::id()));
        let archive = tar_gz(&[("a/b.txt", b"hello")]);
        extract_tar_gz(Cursor::new(&archive), &dir).expect("extract");
        assert_eq!(fs::read(dir.join("a/b.txt")).unwrap(), b"hello");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn local_source_installs_pack_archive() {
        let layout = layout("local");
        let pack = layout.root.join("pack");
        fs::create_dir_all(&pack).unwrap();
        let archive = tar_gz(&[("bin/wine64", b"MZ")]);
        fs::write(pack.join("wine.tar.gz"), &archive).unwrap();
        let comp = component("wine", &archive);
        install_component(&layout, &LocalSource::new(&pack), &comp).expect("install");
        assert!(installed_marker(&layout, "wine").is_some());
        assert_eq!(
            fs::read(layout.component_dir("wine").join("bin/wine64")).unwrap(),
            b"MZ"
        );
        std::fs::remove_dir_all(&layout.root).ok();
    }

    #[test]
    fn local_source_rejects_unsafe_names() {
        let layout = layout("local-unsafe");
        fs::create_dir_all(&layout.root).unwrap();
        let mut comp = component("wine", b"x");
        comp.name = "../evil".into();
        let dest = layout.root.join("out");
        assert_eq!(
            LocalSource::new(&layout.root).fetch(&comp, &dest),
            Err(InstallError::UnsafeComponentName)
        );
        std::fs::remove_dir_all(&layout.root).ok();
    }
}
