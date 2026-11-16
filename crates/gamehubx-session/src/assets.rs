use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const MARKER: &str = ".ready";
pub const IMAGEFS: &str = "imagefs.tar.zst";
pub const PROTON: &str = "proton-11.0-7-arm64ec.tar.zst";
pub const CONTAINER_PATTERN: &str = "container_pattern_common.tzst";
pub const PROTON_SHA256: &str = "1ef0bb9efceb2278933b3fa2bfd374999b0d9a7c74045b67b9af8a9b5260f734";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetError {
    Io,
    ProtonHashMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeAssets {
    root: PathBuf,
}

impl RuntimeAssets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }

    pub fn missing(&self) -> Vec<&'static str> {
        [MARKER, IMAGEFS, PROTON, CONTAINER_PATTERN]
            .into_iter()
            .filter(|name| !self.path(name).is_file())
            .collect()
    }

    pub fn marker_commit(&self) -> Option<String> {
        fs::read_to_string(self.path(MARKER))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    pub fn mark_ready(&self, commit: &str) -> Result<(), AssetError> {
        fs::create_dir_all(&self.root).map_err(|_| AssetError::Io)?;
        fs::write(self.path(MARKER), format!("{commit}\n")).map_err(|_| AssetError::Io)
    }

    pub fn verify_proton(&self) -> Result<(), AssetError> {
        let mut file = File::open(self.path(PROTON)).map_err(|_| AssetError::Io)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|_| AssetError::Io)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual.eq_ignore_ascii_case(PROTON_SHA256) {
            Ok(())
        } else {
            Err(AssetError::ProtonHashMismatch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamehubx-assets-{name}-{}-{}",
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
    fn reports_missing_and_ready() {
        let dir = temp("ready");
        let assets = RuntimeAssets::new(&dir);
        assert_eq!(
            assets.missing(),
            vec![MARKER, IMAGEFS, PROTON, CONTAINER_PATTERN]
        );
        fs::write(assets.path(IMAGEFS), b"x").unwrap();
        fs::write(assets.path(PROTON), b"x").unwrap();
        fs::write(assets.path(CONTAINER_PATTERN), b"x").unwrap();
        assets.mark_ready("commit").unwrap();
        assert!(assets.is_ready());
        assert_eq!(assets.marker_commit().as_deref(), Some("commit"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verifies_proton_hash() {
        let dir = temp("hash");
        let assets = RuntimeAssets::new(&dir);
        fs::write(assets.path(PROTON), b"abc").unwrap();
        assert_eq!(assets.verify_proton(), Err(AssetError::ProtonHashMismatch));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_proton_is_io() {
        let assets = RuntimeAssets::new(temp("missing"));
        assert_eq!(assets.verify_proton(), Err(AssetError::Io));
    }
}
