//! Base-container / patch-version bookkeeping.
//!
//! A "base" container is the pristine image that per-game containers are
//! branched from. The marker file `.base-container` marks such a container and
//! `.patch-version` tracks which patch level its layered data has reached.
//! All paths are derived from a caller-supplied absolute directory and are
//! validated before use; there is no `unsafe`.

use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::layout::is_within;

/// Marker file identifying a base container.
pub const BASE_CONTAINER_MARKER: &str = ".base-container";
/// File holding the decimal patch version.
pub const PATCH_VERSION_FILE: &str = ".patch-version";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerPatchError {
    /// The directory must be absolute.
    NotAbsolute,
    /// The directory or resolved file path is unsafe.
    UnsafePath,
    Io,
}

impl std::fmt::Display for ContainerPatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for ContainerPatchError {}
impl From<std::io::Error> for ContainerPatchError {
    fn from(_: std::io::Error) -> Self {
        ContainerPatchError::Io
    }
}

/// Path of the patch-version file inside `dir`.
pub fn patch_version_path(dir: &Path) -> Result<PathBuf, ContainerPatchError> {
    file_in(dir, PATCH_VERSION_FILE)
}

/// Path of the base-container marker inside `dir`.
pub fn base_marker_path(dir: &Path) -> Result<PathBuf, ContainerPatchError> {
    file_in(dir, BASE_CONTAINER_MARKER)
}

/// Read the patch version of a container.
///
/// Returns `None` if the directory or file cannot be read, or if the file does
/// not contain a plain decimal `u32`. Missing/corrupt data is never fatal.
pub fn read_patch_version(dir: &Path) -> Option<u32> {
    let path = patch_version_path(dir).ok()?;
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse::<u32>().ok()
}

/// Atomically write the patch version of a container, creating `dir` if needed.
pub fn write_patch_version(dir: &Path, version: u32) -> Result<(), ContainerPatchError> {
    let path = patch_version_path(dir)?;
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{PATCH_VERSION_FILE}.tmp"));
    fs::write(&temporary, version.to_string())?;
    fs::rename(temporary, path)?;
    Ok(())
}

/// Whether `dir` is marked as a base container.
///
/// A symlink at the marker path does not count, so an untrusted container
/// cannot point the marker outside its own directory.
pub fn is_base_container(dir: &Path) -> bool {
    match base_marker_path(dir) {
        Ok(path) => fs::symlink_metadata(path)
            .map(|metadata| metadata.file_type().is_file())
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Mark (or unmark) `dir` as a base container.
pub fn set_base_container(dir: &Path, is_base: bool) -> Result<(), ContainerPatchError> {
    let path = base_marker_path(dir)?;
    if !is_base {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{BASE_CONTAINER_MARKER}.tmp"));
    fs::write(&temporary, b"1")?;
    fs::rename(temporary, path)?;
    Ok(())
}

/// Convenience wrapper for [`set_base_container`] with `is_base = true`.
pub fn mark_base_container(dir: &Path) -> Result<(), ContainerPatchError> {
    set_base_container(dir, true)
}

fn file_in(dir: &Path, name: &str) -> Result<PathBuf, ContainerPatchError> {
    if !dir.is_absolute() {
        return Err(ContainerPatchError::NotAbsolute);
    }
    if dir
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ContainerPatchError::UnsafePath);
    }
    let path = dir.join(name);
    if !is_within(dir, &path) {
        return Err(ContainerPatchError::UnsafePath);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gamehubx-base-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn patch_version_roundtrip_and_missing() {
        let dir = unique_dir("version");
        assert_eq!(read_patch_version(&dir), None);
        write_patch_version(&dir, 7).unwrap();
        assert_eq!(read_patch_version(&dir), Some(7));
        write_patch_version(&dir, 12).unwrap();
        assert_eq!(read_patch_version(&dir), Some(12));
        assert_eq!(fs::read(patch_version_path(&dir).unwrap()).unwrap(), b"12");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_patch_version_reads_as_none() {
        let dir = unique_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(patch_version_path(&dir).unwrap(), b"not-a-number").unwrap();
        assert_eq!(read_patch_version(&dir), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn base_marker_lifecycle() {
        let dir = unique_dir("marker");
        assert!(!is_base_container(&dir));
        mark_base_container(&dir).unwrap();
        assert!(is_base_container(&dir));
        assert!(base_marker_path(&dir).unwrap().is_file());
        set_base_container(&dir, false).unwrap();
        assert!(!is_base_container(&dir));
        // Removing twice is a no-op.
        set_base_container(&dir, false).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_relative_and_traversal_paths() {
        assert_eq!(
            write_patch_version(Path::new("relative/container"), 1),
            Err(ContainerPatchError::NotAbsolute)
        );
        let dir = unique_dir("unsafe");
        let traversal = dir.join("container/../escape");
        assert_eq!(
            write_patch_version(&traversal, 1),
            Err(ContainerPatchError::UnsafePath)
        );
        assert_eq!(read_patch_version(Path::new("relative")), None);
        assert!(!is_base_container(Path::new("relative")));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_marker_is_not_a_base_container() {
        let dir = unique_dir("symlink-marker");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("real-marker");
        fs::write(&target, b"1").unwrap();
        std::os::unix::fs::symlink(&target, base_marker_path(&dir).unwrap()).unwrap();
        assert!(!is_base_container(&dir));
        let _ = fs::remove_dir_all(dir);
    }
}
