//! Resumable HTTPS download engine and its persisted task model.
//!
//! Downloads are written to `<dest>.part`, resumed with a Range request, and
//! only moved to `dest` after the declared size and optional SHA-256 verify.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "downloading" => Self::Downloading,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSpec {
    pub title: String,
    pub url: String,
    pub dest: PathBuf,
    pub bytes_total: u64,
    pub sha256: Option<String>,
    /// Groups several specs into one batch; `None` means a standalone task.
    #[serde(default)]
    pub group_id: Option<String>,
    /// Higher values are dequeued first by `next_queued_by_priority`.
    #[serde(default)]
    pub priority: i32,
    /// Unpack an archive after the download completes.
    #[serde(default)]
    pub auto_unzip: bool,
    /// Install the unpacked payload without user confirmation.
    #[serde(default)]
    pub silent_install: bool,
    /// Names of the sub-files belonging to this group, relative to `dest`'s parent.
    #[serde(default)]
    pub sub_files: Vec<String>,
}

impl Default for DownloadSpec {
    fn default() -> Self {
        Self {
            title: String::new(),
            url: String::new(),
            dest: PathBuf::new(),
            bytes_total: 0,
            sha256: None,
            group_id: None,
            priority: 0,
            auto_unzip: false,
            silent_install: false,
            sub_files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub dest: PathBuf,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub status: DownloadStatus,
    pub sha256: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub auto_unzip: bool,
    #[serde(default)]
    pub silent_install: bool,
    #[serde(default)]
    pub sub_files: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadError {
    InsecureUrl,
    BadRequest,
    Io,
    Http,
    Cancelled,
    SizeMismatch,
    HashMismatch,
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InsecureUrl => "only HTTPS URLs are allowed",
            Self::BadRequest => "the server rejected the download request",
            Self::Io => "could not write the downloaded file",
            Self::Http => "network error",
            Self::Cancelled => "download cancelled",
            Self::SizeMismatch => "downloaded size did not match the manifest",
            Self::HashMismatch => "SHA-256 did not match the manifest",
        })
    }
}

impl std::error::Error for DownloadError {}

impl From<std::io::Error> for DownloadError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut path = dest.as_os_str().to_os_string();
    path.push(".part");
    PathBuf::from(path)
}

/// Streams an HTTPS URL into `spec.dest`, resuming an existing `.part`.
pub fn download(
    spec: &DownloadSpec,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<u64, DownloadError> {
    if !valid_https(&spec.url) {
        return Err(DownloadError::InsecureUrl);
    }
    if let Some(parent) = spec.dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let part = part_path(&spec.dest);
    let mut existing = part.metadata().map(|meta| meta.len()).unwrap_or(0);
    if spec.bytes_total > 0 && existing > spec.bytes_total {
        let _ = fs::remove_file(&part);
        existing = 0;
    }

    if spec.bytes_total > 0 && existing == spec.bytes_total {
        verify_and_commit(&part, &spec.dest, spec.bytes_total, spec.sha256.as_deref())?;
        progress(existing, existing);
        return Ok(existing);
    }

    let mut request = ureq::get(&spec.url)
        .set("Accept-Encoding", "identity")
        .timeout(Duration::from_secs(60));
    if existing > 0 {
        request = request.set("Range", &format!("bytes={existing}-"));
    }
    let response = match request.call() {
        Ok(response) => response,
        Err(ureq::Error::Status(416, _)) => {
            let _ = fs::remove_file(&part);
            return Err(DownloadError::BadRequest);
        }
        Err(ureq::Error::Status(_, _)) | Err(ureq::Error::Transport(_)) => {
            return Err(DownloadError::Http);
        }
    };
    if !valid_https(response.get_url()) {
        return Err(DownloadError::InsecureUrl);
    }

    let (mut file, start) = if response.status() == 206 {
        (
            OpenOptions::new().create(true).append(true).open(&part)?,
            existing,
        )
    } else if response.status() == 200 {
        (File::create(&part)?, 0)
    } else {
        return Err(DownloadError::BadRequest);
    };

    let remaining = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let total = if spec.bytes_total > 0 {
        spec.bytes_total
    } else if remaining > 0 {
        start.saturating_add(remaining)
    } else {
        0
    };

    let mut reader = response.into_reader();
    let mut done = start;
    let mut buffer = [0u8; CHUNK];
    loop {
        if cancel.load(Ordering::Relaxed) {
            file.flush()?;
            file.sync_all()?;
            return Err(DownloadError::Cancelled);
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        done = done.saturating_add(read as u64);
        if total > 0 && done > total {
            let _ = fs::remove_file(&part);
            return Err(DownloadError::SizeMismatch);
        }
        progress(done, total);
    }
    file.flush()?;
    file.sync_all()?;

    if spec.bytes_total > 0 && done != spec.bytes_total {
        let _ = fs::remove_file(&part);
        return Err(DownloadError::SizeMismatch);
    }
    verify_and_commit(&part, &spec.dest, done, spec.sha256.as_deref())?;
    progress(done, total.max(done));
    Ok(done)
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify_and_commit(
    part: &Path,
    dest: &Path,
    size: u64,
    expected_sha256: Option<&str>,
) -> Result<(), DownloadError> {
    let actual_size = part.metadata()?.len();
    if size > 0 && actual_size != size {
        return Err(DownloadError::SizeMismatch);
    }
    if let Some(expected) = expected_sha256 {
        if !sha256_file(part)?.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(part);
            return Err(DownloadError::HashMismatch);
        }
    }
    fs::rename(part, dest)?;
    Ok(())
}

fn valid_https(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > "https://".len()
        && !url.chars().any(char::is_whitespace)
}

// --- Minimal, dependency-free MD5 -----------------------------------------
// Reference manifests carry a legacy MD5 `file_checksum`. This compact
// implementation avoids pulling in another crate; SHA-256 (sha2) remains the
// primary integrity check for downloads.

const MD5_S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const MD5_K: [u32; 64] = [
    0xd76a_a478,
    0xe8c7_b756,
    0x2420_70db,
    0xc1bd_ceee,
    0xf57c_0faf,
    0x4787_c62a,
    0xa830_4613,
    0xfd46_9501,
    0x6980_98d8,
    0x8b44_f7af,
    0xffff_5bb1,
    0x895c_d7be,
    0x6b90_1122,
    0xfd98_7193,
    0xa679_438e,
    0x49b4_0821,
    0xf61e_2562,
    0xc040_b340,
    0x265e_5a51,
    0xe9b6_c7aa,
    0xd62f_105d,
    0x0244_1453,
    0xd8a1_e681,
    0xe7d3_fbc8,
    0x21e1_cde6,
    0xc337_07d6,
    0xf4d5_0d87,
    0x455a_14ed,
    0xa9e3_e905,
    0xfcef_a3f8,
    0x676f_02d9,
    0x8d2a_4c8a,
    0xfffa_3942,
    0x8771_f681,
    0x6d9d_6122,
    0xfde5_380c,
    0xa4be_ea44,
    0x4bde_cfa9,
    0xf6bb_4b60,
    0xbebf_bc70,
    0x289b_7ec6,
    0xeaa1_27fa,
    0xd4ef_3085,
    0x0488_1d05,
    0xd9d4_d039,
    0xe6db_99e5,
    0x1fa2_7cf8,
    0xc4ac_5665,
    0xf429_2244,
    0x432a_ff97,
    0xab94_23a7,
    0xfc93_a039,
    0x655b_59c3,
    0x8f0c_cc92,
    0xffef_f47d,
    0x8584_5dd1,
    0x6fa8_7e4f,
    0xfe2c_e6e0,
    0xa301_4314,
    0x4e08_11a1,
    0xf753_7e82,
    0xbd3a_f235,
    0x2ad7_d2bb,
    0xeb86_d391,
];

struct Md5 {
    state: [u32; 4],
    block: [u8; 64],
    block_len: usize,
    len: u64,
}

impl Md5 {
    fn new() -> Self {
        Self {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            block: [0u8; 64],
            block_len: 0,
            len: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        if self.block_len > 0 {
            let take = (64 - self.block_len).min(data.len());
            self.block[self.block_len..self.block_len + take].copy_from_slice(&data[..take]);
            self.block_len += take;
            data = &data[take..];
            if self.block_len == 64 {
                let block = self.block;
                self.compress(&block);
                self.block_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress(&block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.block[..data.len()].copy_from_slice(data);
            self.block_len = data.len();
        }
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            let at = i * 4;
            *word = u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
        }
        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let previous = d;
            d = c;
            c = b;
            b = b.wrapping_add(
                a.wrapping_add(f)
                    .wrapping_add(MD5_K[i])
                    .wrapping_add(m[g])
                    .rotate_left(MD5_S[i]),
            );
            a = previous;
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    fn finish(mut self) -> [u8; 16] {
        let bit_len = self.len.wrapping_mul(8);
        let pad_len = if self.block_len < 56 {
            56 - self.block_len
        } else {
            120 - self.block_len
        };
        let mut padding = [0u8; 64];
        padding[0] = 0x80;
        self.update(&padding[..pad_len]);
        self.update(&bit_len.to_le_bytes());
        let mut digest = [0u8; 16];
        for (i, word) in self.state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        digest
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Computes the MD5 checksum of a file, used for legacy manifest checksums.
pub fn md5_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Md5::new();
    let mut buffer = [0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finish()))
}

/// Verifies size and SHA-256 for a batch of already-completed download tasks.
/// Relative `dest` paths are resolved against `root`; absolute paths are used
/// as-is. Returns `(task_id, is_valid)` in the same order as `tasks`; tasks
/// that are not `Completed` are always reported as invalid.
pub fn verify_download_group(tasks: &[DownloadTask], root: &Path) -> Vec<(i64, bool)> {
    tasks
        .iter()
        .map(|task| {
            let valid = task.status == DownloadStatus::Completed
                && verify_complete_file(
                    &root.join(&task.dest),
                    task.bytes_total,
                    task.sha256.as_deref(),
                );
            (task.id, valid)
        })
        .collect()
}

fn verify_complete_file(path: &Path, expected_size: u64, expected_sha256: Option<&str>) -> bool {
    let meta = match fs::metadata(path) {
        Ok(meta) if meta.is_file() => meta,
        _ => return false,
    };
    if expected_size > 0 && meta.len() != expected_size {
        return false;
    }
    match expected_sha256 {
        Some(expected) => sha256_file(path)
            .map(|actual| actual.eq_ignore_ascii_case(expected))
            .unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamehubx-download-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_insecure_urls() {
        let dir = temp_dir("url");
        let spec = DownloadSpec {
            title: "x".into(),
            url: "http://example.com/x".into(),
            dest: dir.join("x"),
            bytes_total: 1,
            sha256: None,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        assert_eq!(
            download(&spec, &cancel, |_, _| {}),
            Err(DownloadError::InsecureUrl)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hashes_and_commits() {
        let dir = temp_dir("commit");
        let part = dir.join("x.part");
        fs::write(&part, b"abc").unwrap();
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&part).unwrap(), expected);
        verify_and_commit(&part, &dir.join("x"), 3, Some(expected)).unwrap();
        assert!(dir.join("x").is_file());
        assert!(!part.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn removes_part_on_hash_mismatch() {
        let dir = temp_dir("hash");
        let part = dir.join("x.part");
        fs::write(&part, b"abc").unwrap();
        let error = verify_and_commit(&part, &dir.join("x"), 3, Some(&"0".repeat(64)));
        assert_eq!(error, Err(DownloadError::HashMismatch));
        assert!(!part.exists());
        assert!(!dir.join("x").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn status_roundtrip() {
        for status in [
            DownloadStatus::Queued,
            DownloadStatus::Downloading,
            DownloadStatus::Paused,
            DownloadStatus::Completed,
            DownloadStatus::Failed,
        ] {
            assert_eq!(DownloadStatus::from_str(status.as_str()), status);
        }
    }

    #[test]
    fn md5_known_vectors() {
        let dir = temp_dir("md5");
        let vectors: [(&[u8], &str); 4] = [
            (b"", "d41d8cd98f00b204e9800998ecf8427e"),
            (b"abc", "900150983cd24fb0d6963f7d28e17f72"),
            (b"message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                b"abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
        ];
        for (index, (data, expected)) in vectors.iter().enumerate() {
            let path = dir.join(format!("v{index}"));
            fs::write(&path, data).unwrap();
            assert_eq!(md5_file(&path).unwrap(), *expected);
        }
        // Payload that spans more than one 64-byte block.
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        let path = dir.join("long");
        fs::write(&path, long).unwrap();
        assert_eq!(md5_file(&path).unwrap(), "8215ef0796a20bcaaae116d3876c664a");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verifies_download_group() {
        let dir = temp_dir("group");
        let good = dir.join("good.bin");
        fs::write(&good, b"abc").unwrap();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let bad_hash = dir.join("bad_hash.bin");
        fs::write(&bad_hash, b"abc").unwrap();
        let bad_size = dir.join("bad_size.bin");
        fs::write(&bad_size, b"ab").unwrap();

        fn task(
            id: i64,
            dest: PathBuf,
            status: DownloadStatus,
            size: u64,
            sha: Option<&str>,
        ) -> DownloadTask {
            DownloadTask {
                id,
                title: format!("t{id}"),
                url: "https://example.com/x".into(),
                dest,
                bytes_done: size,
                bytes_total: size,
                status,
                sha256: sha.map(str::to_string),
                error: None,
                created_at: id,
                completed_at: None,
                group_id: None,
                priority: 0,
                auto_unzip: false,
                silent_install: false,
                sub_files: Vec::new(),
            }
        }

        let tasks = vec![
            task(1, good.clone(), DownloadStatus::Completed, 3, Some(sha)),
            task(
                2,
                bad_hash,
                DownloadStatus::Completed,
                3,
                Some(&"0".repeat(64)),
            ),
            task(3, bad_size, DownloadStatus::Completed, 3, None),
            task(
                4,
                dir.join("missing.bin"),
                DownloadStatus::Completed,
                3,
                None,
            ),
            task(5, good, DownloadStatus::Queued, 3, Some(sha)),
        ];
        assert_eq!(
            verify_download_group(&tasks, &dir),
            vec![(1, true), (2, false), (3, false), (4, false), (5, false)]
        );
        let _ = fs::remove_dir_all(dir);
    }
}
