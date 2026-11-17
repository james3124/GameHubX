//! Minimal `.reg` parsing / application for container setup.
//!
//! Supports the two value types used by game fix scripts: `REG_SZ` (quoted
//! strings) and `REG_DWORD` (`dword:xxxxxxxx`) under registry key headers such
//! as `[HKEY_CURRENT_USER\Software\...]`. Parsing is tolerant of comments,
//! blank lines, CRLF and the standard Windows Registry header line.
//!
//! [`apply_reg`] does not talk to Wine directly. It renders all patches into a
//! single deterministic `.reg` file inside the container, which the bootstrap
//! pipeline (the `apply-registry` step) imports. Key paths are validated so a
//! malicious script cannot traverse out of the container; no `unsafe` is used.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout::is_within;

/// Standard header emitted at the top of generated `.reg` files.
pub const REG_HEADER: &str = "Windows Registry Editor Version 5.00";
/// File name of the generated patch file inside the container.
pub const REG_FILENAME: &str = "gamehubx-patches.reg";
/// Directory (relative to the container) the patch file is written to.
pub const REG_RELATIVE_DIR: &str = "drive_c/windows";

/// A registry value we know how to write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistryValue {
    Sz(String),
    Dword(u32),
}

/// A single registry value to set under `key`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryPatch {
    /// Full registry key path, e.g. `HKEY_CURRENT_USER\Software\Game`.
    pub key: String,
    /// Value name (empty string means the key's unnamed default value).
    pub name: String,
    pub value: RegistryValue,
}

impl RegistryPatch {
    pub fn sz(key: impl Into<String>, name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            value: RegistryValue::Sz(value.into()),
        }
    }

    pub fn dword(key: impl Into<String>, name: impl Into<String>, value: u32) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            value: RegistryValue::Dword(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryError {
    /// The container directory must be absolute.
    NotAbsolute,
    /// A registry key path is malformed or attempts traversal.
    UnsafeKey,
    /// The resolved output path escapes the container or is a symlink.
    UnsafePath,
    Io,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for RegistryError {}
impl From<io::Error> for RegistryError {
    fn from(_: io::Error) -> Self {
        RegistryError::Io
    }
}

const ROOTS: &[&str] = &[
    "HKEY_CURRENT_USER",
    "HKEY_LOCAL_MACHINE",
    "HKEY_CLASSES_ROOT",
    "HKEY_USERS",
    "HKEY_CURRENT_CONFIG",
];

/// Validate a registry key path.
///
/// Keys must start with a known `HKEY_*` root, must not contain `/`, NUL or
/// control characters, and no path segment may be `.` or `..` (registry keys
/// use `\` as separator, so `..` segments would otherwise allow traversal
/// when a key is mapped onto a filesystem path).
pub fn validate_key(key: &str) -> Result<(), RegistryError> {
    if key.is_empty() || key.chars().any(|c| c == '\0' || c == '/' || c.is_control()) {
        return Err(RegistryError::UnsafeKey);
    }
    let mut segments = key.split('\\');
    let root = segments.next().ok_or(RegistryError::UnsafeKey)?;
    if !ROOTS.iter().any(|known| root.eq_ignore_ascii_case(known)) {
        return Err(RegistryError::UnsafeKey);
    }
    for segment in segments {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(RegistryError::UnsafeKey);
        }
    }
    Ok(())
}

/// Parse the subset of `.reg` files used by game fix scripts.
///
/// Malformed or unsupported lines are skipped rather than failing the whole
/// file; only valid `REG_SZ` / `REG_DWORD` values under a valid key are
/// returned.
pub fn parse_reg(text: &str) -> Vec<RegistryPatch> {
    let mut patches = Vec::new();
    let mut current_key: Option<String> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim().trim_end_matches('\r').trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with("Windows Registry")
            || line.starts_with("REGEDIT")
        {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let key = &line[1..line.len() - 1];
            current_key = if validate_key(key).is_ok() {
                Some(key.to_string())
            } else {
                None
            };
            continue;
        }

        let Some(key) = current_key.as_ref() else {
            continue;
        };
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };

        let name = left.trim();
        let (name, is_unnamed) = if name == "@" {
            (String::new(), true)
        } else if name.starts_with('"') && name.ends_with('"') && name.len() >= 2 {
            (unescape_value(&name[1..name.len() - 1]), false)
        } else {
            continue;
        };
        if !is_unnamed && name.is_empty() {
            continue;
        }

        let right = right.trim();
        let value = if right.starts_with('"') && right.ends_with('"') && right.len() >= 2 {
            RegistryValue::Sz(unescape_value(&right[1..right.len() - 1]))
        } else if let Some(hex) = right
            .strip_prefix("dword:")
            .or_else(|| right.strip_prefix("DWORD:"))
        {
            let hex = hex.trim().trim_start_matches("0x").trim_start_matches("0X");
            match u32::from_str_radix(hex, 16) {
                Ok(number) => RegistryValue::Dword(number),
                Err(_) => continue,
            }
        } else {
            // Unsupported type (hex:, expand_sz, ...) — skip this line.
            continue;
        };
        patches.push(RegistryPatch {
            key: key.clone(),
            name,
            value,
        });
    }
    patches
}

/// Render patches into a deterministic `.reg` file body.
///
/// Patches are grouped by key, duplicate `(key, name)` pairs keep the last
/// value, and output uses `\r\n` line endings with uppercase 8-digit DWORDs.
pub fn render_reg(patches: &[RegistryPatch]) -> String {
    let mut by_key: BTreeMap<&str, BTreeMap<&str, &RegistryValue>> = BTreeMap::new();
    for patch in patches {
        if validate_key(&patch.key).is_err() {
            continue;
        }
        by_key
            .entry(patch.key.as_str())
            .or_default()
            .insert(patch.name.as_str(), &patch.value);
    }

    let mut text = format!("{REG_HEADER}\r\n");
    for (key, values) in by_key {
        text.push_str("\r\n");
        text.push('[');
        text.push_str(key);
        text.push_str("]\r\n");
        for (name, value) in values {
            if name.is_empty() {
                text.push_str("@");
            } else {
                text.push('"');
                text.push_str(&escape_value(name));
                text.push_str("\"");
            }
            text.push('=');
            match value {
                RegistryValue::Sz(value) => {
                    text.push('"');
                    text.push_str(&escape_value(value));
                    text.push_str("\"");
                }
                RegistryValue::Dword(value) => {
                    text.push_str(&format!("dword:{value:08X}"));
                }
            }
            text.push_str("\r\n");
        }
    }
    text
}

/// Write all `patches` as a single deterministic `.reg` file inside the
/// container and return the path of the written file.
///
/// `prefix_dir` must be an absolute path without `..` components. The key
/// paths in the patches are validated and the final output path is checked to
/// remain within the container (and not be an existing symlink), so untrusted
/// patch input cannot write outside the prefix.
pub fn apply_reg(patches: &[RegistryPatch], prefix_dir: &Path) -> Result<PathBuf, RegistryError> {
    if !prefix_dir.is_absolute() {
        return Err(RegistryError::NotAbsolute);
    }
    if prefix_dir
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(RegistryError::UnsafePath);
    }
    for patch in patches {
        validate_key(&patch.key)?;
    }

    let target = prefix_dir.join(REG_RELATIVE_DIR).join(REG_FILENAME);
    if !is_within(prefix_dir, &target) {
        return Err(RegistryError::UnsafePath);
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&target) {
        if metadata.file_type().is_symlink() {
            return Err(RegistryError::UnsafePath);
        }
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = target.with_extension("reg.tmp");
    std::fs::write(&temporary, render_reg(patches))?;
    std::fs::rename(temporary, &target)?;
    Ok(target)
}

fn escape_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn unescape_value(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            unescaped.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => unescaped.push('\\'),
            Some('"') => unescaped.push('"'),
            Some('n') => unescaped.push('\n'),
            Some('r') => unescaped.push('\r'),
            Some(other) => {
                unescaped.push('\\');
                unescaped.push(other);
            }
            None => unescaped.push('\\'),
        }
    }
    unescaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn unique_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "gamehubx-registry-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    const SAMPLE: &str = "Windows Registry Editor Version 5.00\r\n\
\r\n\
; a comment\r\n\
[HKEY_CURRENT_USER\\Software\\Game]\r\n\
\"Name\"=\"My Game\"\r\n\
\"Flags\"=dword:0000002a\r\n\
\"@\"=\"quoted \\\"value\\\"\"\r\n\
\"Unsupported\"=hex:01,02\r\n\
\r\n\
[HKEY_LOCAL_MACHINE\\Software\\Game]\r\n\
@=\"default\"\r\n";

    #[test]
    fn parses_sz_and_dword_and_skips_unsupported() {
        let patches = parse_reg(SAMPLE);
        assert_eq!(patches.len(), 4);
        assert_eq!(
            patches[0],
            RegistryPatch::sz("HKEY_CURRENT_USER\\Software\\Game", "Name", "My Game")
        );
        assert_eq!(
            patches[1],
            RegistryPatch::dword("HKEY_CURRENT_USER\\Software\\Game", "Flags", 42)
        );
        assert_eq!(
            patches[2],
            RegistryPatch::sz("HKEY_CURRENT_USER\\Software\\Game", "@", "quoted \"value\"")
        );
        assert_eq!(
            patches[3],
            RegistryPatch::sz("HKEY_LOCAL_MACHINE\\Software\\Game", "", "default")
        );
    }

    #[test]
    fn render_is_deterministic_and_roundtrips() {
        let patches = vec![
            RegistryPatch::dword("HKEY_CURRENT_USER\\Software\\Z", "B", 1),
            RegistryPatch::sz("HKEY_CURRENT_USER\\Software\\A", "Name", "a\\b\"c"),
            RegistryPatch::dword("HKEY_CURRENT_USER\\Software\\Z", "A", 255),
            RegistryPatch::sz("HKEY_CURRENT_USER\\Software\\A", "Name", "overridden"),
        ];
        let first = render_reg(&patches);
        assert_eq!(first, render_reg(&patches));
        assert!(first.starts_with(&format!("{REG_HEADER}\r\n")));
        assert!(first.contains("dword:000000FF"));
        // The escaped string value with a duplicate is overridden below, so
        // verify escaping via a separate render with no conflicting entry.
        let escaped = render_reg(&[RegistryPatch::sz(
            "HKEY_CURRENT_USER\\Software\\A",
            "Name",
            "a\\b\"c",
        )]);
        assert!(escaped.contains("\"a\\\\b\\\"c\""));

        let reparsed = parse_reg(&first);
        // Three distinct (key,name) pairs remain after the duplicate is dropped:
        // Z/B, Z/A and A/Name (the second A/Name "overridden" wins).
        assert_eq!(reparsed.len(), 3);
        assert_eq!(
            reparsed.iter().find(|p| p.name == "Name").map(|p| &p.value),
            Some(&RegistryValue::Sz("overridden".into()))
        );
    }

    #[test]
    fn validate_key_rejects_traversal_and_bad_roots() {
        assert!(validate_key("HKEY_CURRENT_USER\\Software\\Ok").is_ok());
        assert!(validate_key("hkey_local_machine\\Software\\Ok").is_ok());
        assert!(validate_key("HKEY_CURRENT_USER\\..\\escape").is_err());
        assert!(validate_key("HKEY_CURRENT_USER\\Software\\.\\x").is_err());
        assert!(validate_key("HKEY_CURRENT_USER/Software").is_err());
        assert!(validate_key("NOT_A_ROOT\\Software").is_err());
        assert!(validate_key("HKEY_CURRENT_USER\\Software\\\0bad").is_err());
        assert!(validate_key("").is_err());
    }

    #[test]
    fn apply_reg_writes_inside_container() {
        let root = unique_dir("apply");
        let patches = parse_reg(SAMPLE);
        let written = apply_reg(&patches, &root).unwrap();
        assert_eq!(written, root.join(REG_RELATIVE_DIR).join(REG_FILENAME));
        let text = std::fs::read_to_string(&written).unwrap();
        assert_eq!(text, render_reg(&patches));
        assert!(text.contains("Windows Registry Editor Version 5.00"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn apply_reg_rejects_unsafe_paths_and_keys() {
        let root = unique_dir("unsafe");
        assert_eq!(
            apply_reg(&[], Path::new("relative/prefix")),
            Err(RegistryError::NotAbsolute)
        );
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(
            apply_reg(&[], &root.join("a/../b")),
            Err(RegistryError::UnsafePath)
        );
        let evil = vec![RegistryPatch::sz("HKEY_CURRENT_USER\\..\\escape", "x", "y")];
        assert_eq!(apply_reg(&evil, &root), Err(RegistryError::UnsafeKey));
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn apply_reg_rejects_symlink_target() {
        let root = unique_dir("symlink");
        let dir = root.join(REG_RELATIVE_DIR);
        std::fs::create_dir_all(&dir).unwrap();
        let outside = root.join("outside.reg");
        std::fs::write(&outside, b"original").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(REG_FILENAME)).unwrap();
        assert_eq!(apply_reg(&[], &root), Err(RegistryError::UnsafePath));
        assert_eq!(std::fs::read(&outside).unwrap(), b"original");
        let _ = std::fs::remove_dir_all(root);
    }
}
