//! Resumable HTTPS download engine and its persisted task model.
//!
//! Downloads are written to `<dest>.part`, resumed with a Range request, and
//! only moved to `dest` after the declared size and optional SHA-256 verify.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

const CHUNK: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadSpec {
    pub title: String,
    pub url: String,
    pub dest: PathBuf,
    pub bytes_total: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
}
