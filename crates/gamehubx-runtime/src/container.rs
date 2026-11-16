use std::fs;
use std::path::{Path, PathBuf};

use gamehubx_core::GameId;
use serde::{Deserialize, Serialize};

use crate::layout::is_within;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerMetadata {
    pub name: String,
    pub wine_path: String,
    pub tag: String,
    pub created_at: i64,
}

impl ContainerMetadata {
    pub fn new(
        name: impl Into<String>,
        wine_path: impl Into<String>,
        tag: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            wine_path: wine_path.into(),
            tag: tag.into(),
            created_at: unix_now(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub id: GameId,
    pub root: PathBuf,
    pub metadata: ContainerMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerError {
    NotAbsoluteRoot,
    Io,
    NotFound,
    AlreadyExists,
    InvalidMetadata,
    UnsafePath,
}

impl std::fmt::Display for ContainerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContainerError {}

impl From<std::io::Error> for ContainerError {
    fn from(_: std::io::Error) -> Self {
        ContainerError::Io
    }
}

#[derive(Debug)]
pub struct ContainerManager {
    root: PathBuf,
}

impl ContainerManager {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ContainerError> {
        let root = root.as_ref().to_path_buf();
        if !root.is_absolute() {
            return Err(ContainerError::NotAbsoluteRoot);
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn container_dir(&self, id: GameId) -> PathBuf {
        self.root.join(format!("{:016x}", id.0))
    }

    pub fn create(
        &self,
        id: GameId,
        metadata: ContainerMetadata,
    ) -> Result<Container, ContainerError> {
        let dir = self.container_dir(id);
        if !is_within(&self.root, &dir) {
            return Err(ContainerError::UnsafePath);
        }
        if dir.exists() {
            return Err(ContainerError::AlreadyExists);
        }
        fs::create_dir_all(&dir)?;
        ensure_layout(&dir)?;
        write_metadata(&dir, &metadata)?;
        Ok(Container {
            id,
            root: dir,
            metadata,
        })
    }

    pub fn open(&self, id: GameId) -> Result<Container, ContainerError> {
        let dir = self.container_dir(id);
        if !is_within(&self.root, &dir) {
            return Err(ContainerError::UnsafePath);
        }
        let metadata = read_metadata(&dir)?;
        Ok(Container {
            id,
            root: dir,
            metadata,
        })
    }

    /// Restores the container layout, and recreates missing/corrupt metadata
    /// from `fallback`.
    pub fn repair(
        &self,
        id: GameId,
        fallback: ContainerMetadata,
    ) -> Result<Container, ContainerError> {
        let dir = self.container_dir(id);
        if !is_within(&self.root, &dir) {
            return Err(ContainerError::UnsafePath);
        }
        fs::create_dir_all(&dir)?;
        ensure_layout(&dir)?;
        let metadata = match read_metadata(&dir) {
            Ok(metadata) => metadata,
            Err(ContainerError::NotFound) | Err(ContainerError::InvalidMetadata) => {
                write_metadata(&dir, &fallback)?;
                fallback
            }
            Err(error) => return Err(error),
        };
        Ok(Container {
            id,
            root: dir,
            metadata,
        })
    }

    pub fn delete(&self, id: GameId) -> Result<bool, ContainerError> {
        let dir = self.container_dir(id);
        if !is_within(&self.root, &dir) {
            return Err(ContainerError::UnsafePath);
        }
        match fs::remove_dir_all(&dir) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub fn list(&self) -> Result<Vec<Container>, ContainerError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut containers = Vec::new();
        for entry in entries.flatten() {
            if !entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
            {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(raw) = u64::from_str_radix(&name, 16) else {
                continue;
            };
            if let Ok(container) = self.open(GameId(raw)) {
                containers.push(container);
            }
        }
        containers.sort_by_key(|container| (container.metadata.created_at, container.id.0));
        Ok(containers)
    }
}

fn ensure_layout(dir: &Path) -> Result<(), ContainerError> {
    fs::create_dir_all(dir.join("drive_c"))?;
    fs::create_dir_all(dir.join("dosdevices"))?;

    #[cfg(unix)]
    {
        let link = dir.join("dosdevices").join("c:");
        if fs::symlink_metadata(&link).is_err() {
            std::os::unix::fs::symlink("../drive_c", &link)?;
        }
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir.join("dosdevices").join("c:"))?;
    }
    Ok(())
}

fn metadata_path(dir: &Path) -> PathBuf {
    dir.join("metadata.json")
}

fn write_metadata(dir: &Path, metadata: &ContainerMetadata) -> Result<(), ContainerError> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|_| ContainerError::InvalidMetadata)?;
    let tmp = dir.join("metadata.json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, metadata_path(dir))?;
    Ok(())
}

fn read_metadata(dir: &Path) -> Result<ContainerMetadata, ContainerError> {
    let text = match fs::read_to_string(metadata_path(dir)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ContainerError::NotFound)
        }
        Err(error) => return Err(error.into()),
    };
    serde_json::from_str(&text).map_err(|_| ContainerError::InvalidMetadata)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gamehubx-container-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn metadata(label: &str) -> ContainerMetadata {
        ContainerMetadata {
            name: label.into(),
            wine_path: "/runtime/wine".into(),
            tag: "proton".into(),
            created_at: 1,
        }
    }

    #[test]
    fn create_open_list_delete() {
        let root = temp_root("lifecycle");
        let manager = ContainerManager::new(&root).unwrap();
        let created = manager.create(GameId(7), metadata("Demo")).unwrap();
        assert_eq!(created.metadata.name, "Demo");
        assert!(created.root.join("drive_c").is_dir());
        assert!(created.root.join("dosdevices").is_dir());
        #[cfg(unix)]
        assert!(fs::symlink_metadata(created.root.join("dosdevices").join("c:")).is_ok());

        let opened = manager.open(GameId(7)).unwrap();
        assert_eq!(opened.metadata, created.metadata);
        assert_eq!(manager.list().unwrap().len(), 1);

        assert!(manager.delete(GameId(7)).unwrap());
        assert_eq!(manager.open(GameId(7)), Err(ContainerError::NotFound));
        assert!(!manager.delete(GameId(7)).unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn create_is_not_silent_overwrite() {
        let root = temp_root("duplicate");
        let manager = ContainerManager::new(&root).unwrap();
        manager.create(GameId(1), metadata("First")).unwrap();
        assert_eq!(
            manager.create(GameId(1), metadata("Second")),
            Err(ContainerError::AlreadyExists)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repair_restores_layout_and_metadata() {
        let root = temp_root("repair");
        let manager = ContainerManager::new(&root).unwrap();
        let created = manager.create(GameId(2), metadata("Original")).unwrap();
        fs::remove_dir_all(created.root.join("drive_c")).unwrap();
        fs::remove_file(created.root.join("metadata.json")).unwrap();

        let repaired = manager.repair(GameId(2), metadata("Repaired")).unwrap();
        assert!(repaired.root.join("drive_c").is_dir());
        assert_eq!(repaired.metadata.name, "Repaired");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repair_replaces_corrupt_metadata() {
        let root = temp_root("corrupt");
        let manager = ContainerManager::new(&root).unwrap();
        let created = manager.create(GameId(3), metadata("Original")).unwrap();
        fs::write(created.root.join("metadata.json"), b"not json").unwrap();
        assert_eq!(
            manager.open(GameId(3)),
            Err(ContainerError::InvalidMetadata)
        );
        let repaired = manager.repair(GameId(3), metadata("Fallback")).unwrap();
        assert_eq!(repaired.metadata.name, "Fallback");
        assert_eq!(manager.open(GameId(3)).unwrap().metadata.name, "Fallback");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn root_must_be_absolute() {
        assert_eq!(
            ContainerManager::new("relative/root").unwrap_err(),
            ContainerError::NotAbsoluteRoot
        );
    }
}
