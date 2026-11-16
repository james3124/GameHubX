//! Mirrors the reference's `WineComponentInstaller`: merge a component into a
//! container (`drive_c/windows` or container root) and remove it later.
//!
//! Copying is restricted to regular files/directories; symlinks are rejected so
//! a component cannot escape the container. Every merged file is recorded in
//! `<container>/.components/<name>.files` for exact removal.

use std::fs;
use std::path::{Path, PathBuf};

use crate::layout::{is_within, RuntimeLayout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeError {
    NotAbsolute,
    UnsafeComponentName,
    UnsafePath,
    SymlinkUnsupported,
    UnsupportedEntryType,
    MissingComponent,
    Io,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MergeError {}

impl From<std::io::Error> for MergeError {
    fn from(_: std::io::Error) -> Self {
        MergeError::Io
    }
}

pub fn merge_component(
    component_dir: &Path,
    container_dir: &Path,
    name: &str,
) -> Result<usize, MergeError> {
    if !component_dir.is_absolute() || !container_dir.is_absolute() {
        return Err(MergeError::NotAbsolute);
    }
    if !safe_name(name) {
        return Err(MergeError::UnsafeComponentName);
    }
    if !component_dir.is_dir() {
        return Err(MergeError::MissingComponent);
    }

    let target_root = if component_dir.join("drive_c").is_dir() {
        container_dir.to_path_buf()
    } else {
        container_dir.join("drive_c/windows")
    };
    let mut installed = Vec::new();
    copy_tree(
        component_dir,
        component_dir,
        &target_root,
        container_dir,
        &mut installed,
    )?;
    write_marker(container_dir, name, &installed)?;
    Ok(installed.len())
}

pub fn remove_component(container_dir: &Path, name: &str) -> Result<usize, MergeError> {
    if !container_dir.is_absolute() {
        return Err(MergeError::NotAbsolute);
    }
    if !safe_name(name) {
        return Err(MergeError::UnsafeComponentName);
    }
    let marker = marker_path(container_dir, name);
    let text = match fs::read_to_string(&marker) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };

    let mut removed = 0;
    for line in text.lines() {
        let relative = Path::new(line);
        if relative.is_absolute() {
            return Err(MergeError::UnsafePath);
        }
        let path = container_dir.join(relative);
        if !is_within(container_dir, &path) {
            return Err(MergeError::UnsafePath);
        }
        match fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let _ = fs::remove_file(&marker);
    Ok(removed)
}

pub fn apply_dxvk(
    layout: &RuntimeLayout,
    container_dir: &Path,
    enabled: bool,
) -> Result<bool, MergeError> {
    if enabled {
        let dxvk = layout.dxvk();
        if !dxvk.is_dir() {
            return Err(MergeError::MissingComponent);
        }
        merge_component(&dxvk, container_dir, "dxvk")?;
        Ok(true)
    } else {
        remove_component(container_dir, "dxvk")?;
        Ok(false)
    }
}

fn copy_tree(
    root: &Path,
    dir: &Path,
    target_root: &Path,
    container: &Path,
    installed: &mut Vec<PathBuf>,
) -> Result<(), MergeError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(MergeError::SymlinkUnsupported);
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| MergeError::UnsafePath)?;
        if metadata.is_dir() {
            copy_tree(root, &path, target_root, container, installed)?;
        } else if metadata.is_file() {
            let destination = target_root.join(relative);
            if !is_within(container, &destination) {
                return Err(MergeError::UnsafePath);
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &destination)?;
            installed.push(
                destination
                    .strip_prefix(container)
                    .map_err(|_| MergeError::UnsafePath)?
                    .to_path_buf(),
            );
        } else {
            return Err(MergeError::UnsupportedEntryType);
        }
    }
    Ok(())
}

fn write_marker(container: &Path, name: &str, installed: &[PathBuf]) -> Result<(), MergeError> {
    let dir = container.join(".components");
    fs::create_dir_all(&dir)?;
    let mut text = String::new();
    for path in installed {
        text.push_str(&path.to_string_lossy());
        text.push('\n');
    }
    fs::write(dir.join(format!("{name}.files")), text)?;
    Ok(())
}

fn marker_path(container: &Path, name: &str) -> PathBuf {
    container.join(".components").join(format!("{name}.files"))
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gamehubx-component-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn merges_drive_c_component_and_removes_it() {
        let root = temp_root("drive-c");
        let component = root.join("component");
        let container = root.join("container");
        fs::create_dir_all(component.join("drive_c/windows/system32")).unwrap();
        fs::write(component.join("drive_c/windows/system32/d3d11.dll"), b"dll").unwrap();
        fs::create_dir_all(&container).unwrap();

        let merged = merge_component(&component, &container, "dxvk").unwrap();
        assert_eq!(merged, 1);
        let installed = container.join("drive_c/windows/system32/d3d11.dll");
        assert_eq!(fs::read(&installed).unwrap(), b"dll");
        assert!(container.join(".components/dxvk.files").is_file());

        assert_eq!(remove_component(&container, "dxvk").unwrap(), 1);
        assert!(!installed.exists());
        assert!(!container.join(".components/dxvk.files").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn merges_flat_component_into_windows() {
        let root = temp_root("flat");
        let component = root.join("component");
        let container = root.join("container");
        fs::create_dir_all(&component).unwrap();
        fs::write(component.join("dinput.dll"), b"dll").unwrap();
        fs::create_dir_all(&container).unwrap();

        assert_eq!(
            merge_component(&component, &container, "dinput").unwrap(),
            1
        );
        assert_eq!(
            fs::read(container.join("drive_c/windows/dinput.dll")).unwrap(),
            b"dll"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks() {
        let root = temp_root("symlink");
        let component = root.join("component");
        let container = root.join("container");
        fs::create_dir_all(&component).unwrap();
        fs::create_dir_all(&container).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", component.join("escape.dll")).unwrap();
        assert_eq!(
            merge_component(&component, &container, "evil"),
            Err(MergeError::SymlinkUnsupported)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_unsafe_names_and_missing_component() {
        let root = temp_root("names");
        let component = root.join("component");
        let container = root.join("container");
        fs::create_dir_all(&container).unwrap();
        assert_eq!(
            merge_component(&component, &container, "../wine"),
            Err(MergeError::UnsafeComponentName)
        );
        assert_eq!(
            merge_component(&component, &container, "wine"),
            Err(MergeError::MissingComponent)
        );
        assert_eq!(remove_component(&container, "wine").unwrap(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn applies_and_removes_dxvk() {
        let root = temp_root("dxvk");
        let layout = RuntimeLayout::new(root.join("runtime"));
        fs::create_dir_all(layout.dxvk().join("drive_c/windows/system32")).unwrap();
        fs::write(
            layout.dxvk().join("drive_c/windows/system32/d3d11.dll"),
            b"dxvk",
        )
        .unwrap();
        let container = root.join("container");
        fs::create_dir_all(&container).unwrap();

        assert!(apply_dxvk(&layout, &container, true).unwrap());
        let installed = container.join("drive_c/windows/system32/d3d11.dll");
        assert_eq!(fs::read(&installed).unwrap(), b"dxvk");

        assert!(!apply_dxvk(&layout, &container, false).unwrap());
        assert!(!installed.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_dxvk_component_is_reported() {
        let root = temp_root("dxvk-missing");
        let layout = RuntimeLayout::new(root.join("runtime"));
        let container = root.join("container");
        fs::create_dir_all(&container).unwrap();
        assert_eq!(
            apply_dxvk(&layout, &container, true),
            Err(MergeError::MissingComponent)
        );
        let _ = fs::remove_dir_all(root);
    }
}
