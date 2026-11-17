pub mod backup;
pub mod component;
pub mod container;
pub mod installer;
pub mod layout;
pub mod manifest;
pub mod session;
pub mod verify;

pub use backup::{backup_container, restore_container, BackupError};
pub use component::{apply_dxvk, merge_component, remove_component, MergeError};
pub use container::{Container, ContainerError, ContainerManager, ContainerMetadata};
pub use installer::{
    install_component, installed_marker, ComponentSource, HttpSource, InstallError,
    InstalledMarker, LocalSource,
};
pub use layout::{is_within, RuntimeLayout};
pub use manifest::{Component, ManifestError, RuntimeManifest};
pub use session::{
    session_settings_json, AudioBackend, CommandSpec, DisplayPlan, SessionConfig, SessionError,
    SessionExtras, SessionPlan,
};
pub use verify::{sha256_hex, verify_file, verify_reader};

#[cfg(test)]
mod p2_tests {
    use crate::container::{ContainerManager, ContainerMetadata};
    use crate::installer::{install_component, installed_marker, ComponentSource, InstallError};
    use crate::layout::RuntimeLayout;
    use crate::manifest::Component;
    use gamehubx_core::GameId;
    use std::path::Path;

    struct MemorySource(Vec<u8>);

    impl ComponentSource for MemorySource {
        fn fetch(&self, _component: &Component, dest: &Path) -> Result<u64, InstallError> {
            std::fs::write(dest, &self.0)?;
            Ok(self.0.len() as u64)
        }
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

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    #[test]
    fn install_verify_then_create_prefix() {
        let root = std::env::temp_dir().join(format!(
            "gamehubx-p2-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let layout = RuntimeLayout::new(root.join("runtime"));
        let archive = tar_gz(&[("bin/wine64", b"MZ...")]);
        let component = Component {
            name: "wine".into(),
            version: "10.0".into(),
            url: "https://example.invalid/wine.tar.gz".into(),
            sha256: sha256_hex(&archive),
            size_bytes: archive.len() as u64,
        };
        install_component(&layout, &MemorySource(archive), &component).expect("install");
        let marker = installed_marker(&layout, "wine").expect("marker");
        assert_eq!(marker.sha256, component.sha256);

        let containers = ContainerManager::new(root.join("containers")).unwrap();
        let metadata =
            ContainerMetadata::new("Sample Game", layout.wine().display().to_string(), "proton");
        let container = containers.create(GameId(42), metadata).unwrap();
        assert!(container.root.join("drive_c").is_dir());
        assert_eq!(containers.list().unwrap().len(), 1);
        assert!(containers.delete(GameId(42)).unwrap());

        let _ = std::fs::remove_dir_all(root);
    }
}
