use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeManifest {
    pub schema: u32,
    pub pack_version: String,
    pub components: Vec<Component>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestError {
    UnsupportedSchema,
    EmptyPackVersion,
    NoComponents,
    DuplicateName,
    BadComponent,
    UnsafeName,
    BadSha256,
    InsecureUrl,
    MissingRequired,
    InvalidJson,
}

impl RuntimeManifest {
    pub const SCHEMA: u32 = 1;

    pub fn from_json(text: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(text).map_err(|_| ManifestError::InvalidJson)
    }

    pub fn to_json_pretty(&self) -> Result<String, ManifestError> {
        serde_json::to_string_pretty(self).map_err(|_| ManifestError::InvalidJson)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema != Self::SCHEMA {
            return Err(ManifestError::UnsupportedSchema);
        }
        if self.pack_version.trim().is_empty() {
            return Err(ManifestError::EmptyPackVersion);
        }
        if self.components.is_empty() {
            return Err(ManifestError::NoComponents);
        }
        let mut names = std::collections::HashSet::new();
        for component in &self.components {
            if component.name.trim().is_empty()
                || component.version.trim().is_empty()
                || component.size_bytes == 0
            {
                return Err(ManifestError::BadComponent);
            }
            if !component
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            {
                return Err(ManifestError::UnsafeName);
            }
            if !names.insert(component.name.as_str()) {
                return Err(ManifestError::DuplicateName);
            }
            if !valid_sha256(&component.sha256) {
                return Err(ManifestError::BadSha256);
            }
            if !(component.url.starts_with("https://") || component.url.starts_with("local://")) {
                return Err(ManifestError::InsecureUrl);
            }
        }
        Ok(())
    }

    pub fn validate_required(&self, required: &[&str]) -> Result<(), ManifestError> {
        self.validate()?;
        for name in required {
            if !self
                .components
                .iter()
                .any(|component| component.name.as_str() == *name)
            {
                return Err(ManifestError::MissingRequired);
            }
        }
        Ok(())
    }

    pub fn component(&self, name: &str) -> Option<&Component> {
        self.components
            .iter()
            .find(|component| component.name == name)
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(name: &str) -> Component {
        Component {
            name: name.into(),
            version: "1.0".into(),
            url: format!("https://example.com/{name}.tar.gz"),
            sha256: "a".repeat(64),
            size_bytes: 1024,
        }
    }

    fn manifest() -> RuntimeManifest {
        RuntimeManifest {
            schema: RuntimeManifest::SCHEMA,
            pack_version: "0.1.0".into(),
            components: vec![component("rootfs"), component("wine")],
        }
    }

    #[test]
    fn valid_manifest_passes() {
        assert_eq!(manifest().validate(), Ok(()));
        assert_eq!(manifest().validate_required(&["rootfs", "wine"]), Ok(()));
    }

    #[test]
    fn rejects_duplicate_names() {
        let mut value = manifest();
        value.components.push(component("wine"));
        assert_eq!(value.validate(), Err(ManifestError::DuplicateName));
    }

    #[test]
    fn rejects_bad_hash_and_http() {
        let mut value = manifest();
        value.components[0].sha256 = "xyz".into();
        assert_eq!(value.validate(), Err(ManifestError::BadSha256));

        let mut value = manifest();
        value.components[0].url = "http://example.com/rootfs.tar.gz".into();
        assert_eq!(value.validate(), Err(ManifestError::InsecureUrl));

        let mut value = manifest();
        value.components[0].url = "local://rootfs.tar.gz".into();
        assert_eq!(value.validate(), Ok(()));
    }

    #[test]
    fn requires_named_components() {
        assert_eq!(
            manifest().validate_required(&["rootfs", "wine", "box64"]),
            Err(ManifestError::MissingRequired)
        );
    }

    #[test]
    fn rejects_unsafe_component_names() {
        let mut value = manifest();
        value.components[0].name = "../rootfs".into();
        assert_eq!(value.validate(), Err(ManifestError::UnsafeName));
    }

    #[test]
    fn json_roundtrip_and_invalid_json() {
        let value = manifest();
        let text = value.to_json_pretty().unwrap();
        assert_eq!(RuntimeManifest::from_json(&text).unwrap(), value);
        assert_eq!(
            RuntimeManifest::from_json("{").unwrap_err(),
            ManifestError::InvalidJson
        );
    }
}
