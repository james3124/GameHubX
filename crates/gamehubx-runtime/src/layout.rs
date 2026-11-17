use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeLayout {
    pub root: PathBuf,
}

impl RuntimeLayout {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn component_dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn rootfs(&self) -> PathBuf {
        self.root.join("rootfs")
    }

    pub fn proot(&self) -> PathBuf {
        self.root.join("proot/proot")
    }

    pub fn wine(&self) -> PathBuf {
        self.root.join("wine/bin/wine64")
    }

    pub fn wineserver(&self) -> PathBuf {
        self.root.join("wine/bin/wineserver")
    }

    pub fn pulseaudio(&self) -> PathBuf {
        self.rootfs().join("bin/pulseaudio")
    }

    pub fn box64(&self) -> PathBuf {
        self.root.join("box64/box64")
    }

    pub fn dxvk(&self) -> PathBuf {
        self.root.join("dxvk")
    }

    pub fn vkd3d(&self) -> PathBuf {
        self.root.join("vkd3d")
    }

    pub fn xserver(&self) -> PathBuf {
        self.root.join("xserver")
    }

    /// Runtime files required before a session can start.
    pub fn missing_components(&self) -> Vec<PathBuf> {
        let mut missing = Vec::new();
        for path in [self.proot(), self.wine(), self.wineserver()] {
            if !path.is_file() {
                missing.push(path);
            }
        }
        for path in [self.rootfs(), self.xserver()] {
            if !path.is_dir() {
                missing.push(path);
            }
        }
        missing
    }
}

pub fn is_within(base: &Path, path: &Path) -> bool {
    let base = normalize(base);
    let path = normalize(path);
    path != base && path.starts_with(&base)
}

pub fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_within_normalizes_parent_components() {
        assert!(is_within(
            Path::new("/games/demo"),
            Path::new("/games/demo/a/../game.exe")
        ));
        assert!(!is_within(
            Path::new("/games/demo"),
            Path::new("/games/demo/../../etc/passwd")
        ));
    }

    #[test]
    fn layout_paths_are_under_root() {
        let layout = RuntimeLayout::new("/data/runtime");
        assert_eq!(layout.wine(), Path::new("/data/runtime/wine/bin/wine64"));
        assert_eq!(
            layout.component_dir("dxvk"),
            Path::new("/data/runtime/dxvk")
        );
    }
}
