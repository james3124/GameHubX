use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

const USER_PREFIX: &str = "xuser-";
const CONFIG: &str = ".container";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinlatorContainer {
    pub id: i32,
    pub name: String,
    pub wine_version: String,
    pub screen_size: String,
    pub root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct WinlatorContainers {
    imagefs_root: PathBuf,
}

#[derive(Deserialize)]
struct RawContainer {
    id: Option<i32>,
    name: Option<String>,
    #[serde(rename = "wineVersion")]
    wine_version: Option<String>,
    #[serde(rename = "screenSize")]
    screen_size: Option<String>,
}

impl WinlatorContainers {
    pub fn new(imagefs_root: impl Into<PathBuf>) -> Self {
        Self {
            imagefs_root: imagefs_root.into(),
        }
    }

    pub fn imagefs_root(&self) -> &Path {
        &self.imagefs_root
    }

    pub fn home_dir(&self) -> PathBuf {
        self.imagefs_root.join("home")
    }

    pub fn list(&self) -> Vec<WinlatorContainer> {
        let home = self.home_dir();
        let mut containers = Vec::new();
        let Ok(entries) = fs::read_dir(&home) else {
            return containers;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            let Some(suffix) = name.strip_prefix(USER_PREFIX) else {
                continue;
            };
            let Ok(dir_id) = suffix.parse::<i32>() else {
                continue;
            };
            let root = entry.path();
            if !root.is_dir() {
                continue;
            }
            let config = root.join(CONFIG);
            if !config.is_file() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&config) else {
                continue;
            };
            let Ok(raw) = serde_json::from_str::<RawContainer>(&text) else {
                continue;
            };
            containers.push(WinlatorContainer {
                id: raw.id.unwrap_or(dir_id),
                name: raw.name.unwrap_or_else(|| format!("Container-{dir_id}")),
                wine_version: raw
                    .wine_version
                    .unwrap_or_else(|| "proton-11.0-7-arm64ec".into()),
                screen_size: raw.screen_size.unwrap_or_else(|| "1280x720".into()),
                root,
            });
        }
        containers.sort_by_key(|container| container.id);
        containers
    }

    pub fn first(&self) -> Option<WinlatorContainer> {
        self.list().into_iter().next()
    }

    pub fn remove(&self, id: i32) -> bool {
        let home = self.home_dir();
        let root = home.join(format!("{USER_PREFIX}{id}"));
        let Ok(home) = home.canonicalize() else {
            return false;
        };
        let Ok(root) = root.canonicalize() else {
            return false;
        };
        if !root.starts_with(&home) || !root.is_dir() {
            return false;
        }
        fs::remove_dir_all(root).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamehubx-containers-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_container(home: &Path, id: i32, body: &str) {
        let dir = home.join(format!("xuser-{id}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CONFIG), body).unwrap();
    }

    #[test]
    fn lists_and_parses_containers() {
        let root = temp("list");
        let home = root.join("home");
        write_container(
            &home,
            1,
            r#"{"id":1,"name":"GameHubX","screenSize":"1920x1080"}"#,
        );
        write_container(&home, 2, r#"{"id":2,"name":"Second"}"#);
        fs::create_dir_all(home.join("xuser-3")).unwrap();
        let containers = WinlatorContainers::new(&root).list();
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0].name, "GameHubX");
        assert_eq!(containers[0].screen_size, "1920x1080");
        assert_eq!(containers[1].wine_version, "proton-11.0-7-arm64ec");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_to_remove_outside_home() {
        let root = temp("remove");
        let containers = WinlatorContainers::new(&root);
        assert!(!containers.remove(1));
        fs::create_dir_all(root.join("home/xuser-1")).unwrap();
        fs::write(root.join("home/xuser-1/.container"), "{}").unwrap();
        assert!(containers.remove(1));
        assert!(!root.join("home/xuser-1").exists());
        let _ = fs::remove_dir_all(root);
    }
}
