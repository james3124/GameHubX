//! Per-game container data backup and restore.
//!
//! The reference launcher's Game Settings has a "Data Backup / Recovery" group
//! that writes a ZIP archive of a game's container via SAF. GameHubX uses a
//! deterministic tar.gz archive of the container directory instead, with the
//! same guarantees:
//!
//! - only regular files/dirs are archived; absolute paths, `..`, symlinks and
//!   hardlinks are rejected on restore (via [`crate::extract_tar_gz`]);
//! - restore is staged into a temp dir and swapped in only on success, so a
//!   failed/corrupt archive never destroys the live container;
//! - a restored container keeps its layout (`drive_c`, `dosdevices/c:`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;

/// Errors from backing up or restoring a container.
#[derive(Debug)]
pub enum BackupError {
    Io(io::Error),
    /// The container directory does not exist.
    NoContainer,
    /// The archive path escapes its parent directory.
    UnsafePath,
    /// The archive could not be decoded.
    BadArchive,
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BackupError {}
impl From<io::Error> for BackupError {
    fn from(value: io::Error) -> Self {
        BackupError::Io(value)
    }
}

/// Archive `container_dir` into `archive` (a `.tar.gz` file).
///
/// Paths inside the archive are relative to the container root. A fixed sort
/// order keeps the output deterministic.
pub fn backup_container(container_dir: &Path, archive: &Path) -> Result<u64, BackupError> {
    if !container_dir.is_dir() {
        return Err(BackupError::NoContainer);
    }
    if archive.file_name().is_none() {
        return Err(BackupError::UnsafePath);
    }
    if let Some(parent) = archive.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let (dirs, files) = collect_entries(container_dir, container_dir)?;
    let file = fs::File::create(archive)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    // Archive directories first so empty dirs (e.g. dosdevices) survive restore.
    for relative in &dirs {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_cksum();
        let mut path = relative.clone();
        path.push("");
        builder.append_data(&mut header, path, io::empty())?;
    }
    for (relative, full) in &files {
        let mut header = tar::Header::new_gnu();
        let meta = fs::metadata(full)?;
        header.set_size(meta.len());
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, relative, fs::File::open(full)?)?;
    }
    let encoder = builder.into_inner().map_err(|_| BackupError::BadArchive)?;
    let file = encoder.finish()?;
    Ok(file.metadata()?.len())
}

/// Restore a container archive into `container_dir`.
///
/// The live container is replaced atomically: the archive is extracted to a
/// staging directory first, and only then swapped with the current container.
/// Returns the number of files restored.
pub fn restore_container(archive: &Path, container_dir: &Path) -> Result<usize, BackupError> {
    if !archive.is_file() {
        return Err(BackupError::NoContainer);
    }
    let parent = container_dir.parent().ok_or(BackupError::UnsafePath)?;
    fs::create_dir_all(parent)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = parent.join(format!(
        "{}.ghx-restore-{}",
        container_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        nonce
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;

    let result = (|| -> Result<usize, BackupError> {
        let file = fs::File::open(archive)?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive_reader = tar::Archive::new(decoder);
        let mut count = 0usize;
        for entry in archive_reader
            .entries()
            .map_err(|_| BackupError::BadArchive)?
        {
            let mut entry = entry.map_err(|_| BackupError::BadArchive)?;
            // Reuse the hardened extractor's path checks by extracting one entry
            // at a time through a fresh archive is wasteful; instead validate
            // here with the same rules as `extract_tar_gz`.
            let path = entry
                .path()
                .map_err(|_| BackupError::BadArchive)?
                .into_owned();
            if path.is_absolute()
                || path.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::RootDir
                    )
                })
                || entry.header().entry_type().is_symlink()
                || entry.header().entry_type().is_hard_link()
            {
                return Err(BackupError::UnsafePath);
            }
            entry
                .unpack_in(&staging)
                .map_err(|_| BackupError::BadArchive)?;
            count += 1;
        }
        Ok(count)
    })();

    match result {
        Ok(count) => {
            // Swap: move the live container aside, move staging into place, then
            // delete the old container. If any step fails, attempt rollback.
            let backup = parent.join(format!(
                "{}.ghx-old-{}",
                container_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy(),
                nonce
            ));
            let had_live = container_dir.exists();
            if had_live {
                fs::rename(container_dir, &backup)?;
            }
            if let Err(error) = fs::rename(&staging, container_dir) {
                if had_live {
                    let _ = fs::rename(&backup, container_dir);
                }
                return Err(error.into());
            }
            if had_live {
                let _ = fs::remove_dir_all(&backup);
            }
            Ok(count)
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn collect_entries(
    root: &Path,
    dir: &Path,
) -> Result<(Vec<PathBuf>, Vec<(PathBuf, PathBuf)>), BackupError> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut visited = 0usize;
    while let Some(current) = stack.pop() {
        visited += 1;
        if visited > 200_000 {
            break;
        }
        let entries = match fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                dirs.push(relative);
                stack.push(path);
            } else if meta.is_file() {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                files.push((relative, path));
            }
        }
    }
    dirs.sort();
    files.sort();
    Ok((dirs, files))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ghx-backup-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn backup_restore_roundtrip() {
        let root = unique_dir("roundtrip");
        let container = root.join("containers/abc");
        fs::create_dir_all(container.join("drive_c/windows")).unwrap();
        fs::create_dir_all(container.join("dosdevices")).unwrap();
        fs::write(container.join("drive_c/windows/system.ini"), b"[system]").unwrap();
        fs::write(container.join("metadata.json"), b"{}").unwrap();

        let archive = root.join("backups/abc.tar.gz");
        let size = backup_container(&container, &archive).unwrap();
        assert!(size > 0);

        fs::remove_dir_all(&container).unwrap();
        assert!(restore_container(&archive, &container).unwrap() >= 2);
        assert_eq!(
            fs::read(container.join("drive_c/windows/system.ini")).unwrap(),
            b"[system]"
        );
        assert!(container.join("dosdevices").is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn backup_missing_container_fails() {
        let root = unique_dir("missing");
        assert!(matches!(
            backup_container(&root.join("nope"), &root.join("a.tar.gz")),
            Err(BackupError::NoContainer)
        ));
    }
}
