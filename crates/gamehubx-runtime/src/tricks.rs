//! Built-in and user-supplied per-game environment "tricks".
//!
//! Some games need small environment hacks (disabling a buggy OpenSSL feature,
//! turning off a DXGI device manager, etc.). This module ships a small built-in
//! table keyed by a game identifier / app id and lets callers register extra
//! entries at runtime. Everything is local: there is no server or network call.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A built-in trick table entry: `(game id, &[(env key, env value)])`.
type BuiltinEntry = (&'static str, &'static [(&'static str, &'static str)]);

/// Small, deterministic built-in table. Ids are matched case-insensitively.
static BUILTIN: &[BuiltinEntry] = &[
    (
        "openssl-no-sse4",
        &[("OPENSSL_ia32cap", "~0x200000200000000")],
    ),
    (
        "dxgi-no-device-manager",
        &[("WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER", "1")],
    ),
    (
        "wine-no-hw-scheduling",
        &[("WINE_DISABLE_HARDWARE_SCHEDULING", "1")],
    ),
    (
        "1091500",
        &[
            ("WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER", "1"),
            ("WINE_DISABLE_HARDWARE_SCHEDULING", "1"),
        ],
    ),
    ("359550", &[("OPENSSL_ia32cap", "~0x200000200000000")]),
];

type TrickMap = HashMap<String, Vec<(String, String)>>;
static CUSTOM: OnceLock<Mutex<TrickMap>> = OnceLock::new();

fn custom_store() -> &'static Mutex<TrickMap> {
    CUSTOM.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_store() -> std::sync::MutexGuard<'static, TrickMap> {
    // Recover from a poisoned lock so a panicking caller cannot brick tricks.
    custom_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// All built-in game identifiers, in table order.
pub fn builtin_ids() -> &'static [&'static str] {
    // Build the slice once from the static table (leaked for 'static lifetime is
    // unnecessary: the ids are already 'static str constants in BUILTIN).
    IDS.get_or_init(|| BUILTIN.iter().map(|(id, _)| *id).collect())
        .as_slice()
}

static IDS: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Environment hacks from the built-in table only (no custom entries).
pub fn builtin_tricks(id: &str) -> Vec<(String, String)> {
    let mut merged: Vec<(String, String)> = Vec::new();
    for (builtin_id, tricks) in BUILTIN {
        if builtin_id.eq_ignore_ascii_case(id) {
            for (key, value) in *tricks {
                set_env(&mut merged, key, value);
            }
        }
    }
    merged.sort_by(|a, b| a.0.cmp(&b.0));
    merged
}

/// Environment hacks from the runtime custom table only.
pub fn custom_tricks(id: &str) -> Vec<(String, String)> {
    let store = lock_store();
    let mut tricks = store.get(id).cloned().unwrap_or_default();
    tricks.sort_by(|a, b| a.0.cmp(&b.0));
    tricks
}

/// All tricks for a game: built-in entries plus custom ones. Custom entries
/// with the same key override built-in values. The result is sorted by key.
pub fn tricks_for(id: &str) -> Vec<(String, String)> {
    let mut merged = builtin_tricks(id);
    for (key, value) in custom_tricks(id) {
        set_env(&mut merged, &key, &value);
    }
    merged.sort_by(|a, b| a.0.cmp(&b.0));
    merged
}

/// Register (or override) a set of custom env tricks for a game id.
///
/// Empty ids and empty keys are ignored. Existing entries for the same key are
/// replaced so repeated calls behave like "set", not "append".
pub fn add_custom_tricks(id: impl Into<String>, tricks: Vec<(String, String)>) {
    let id = id.into();
    if id.is_empty() {
        return;
    }
    let mut store = lock_store();
    let entry = store.entry(id).or_default();
    for (key, value) in tricks {
        if key.is_empty() {
            continue;
        }
        set_env(entry, &key, &value);
    }
}

/// Register a single custom env trick.
pub fn register_trick(id: impl Into<String>, key: impl Into<String>, value: impl Into<String>) {
    add_custom_tricks(id, vec![(key.into(), value.into())]);
}

/// Remove every custom trick registered for `id`. Returns the removed entries.
pub fn remove_custom_tricks(id: &str) -> Vec<(String, String)> {
    let mut store = lock_store();
    store.remove(id).unwrap_or_default()
}

/// Clear all custom tricks (mainly useful for tests / settings reload).
pub fn clear_custom_tricks() {
    lock_store().clear();
}

fn set_env(env: &mut Vec<(String, String)>, key: &str, value: &str) {
    if let Some(existing) = env.iter_mut().find(|(name, _)| name == key) {
        existing.1 = value.to_string();
    } else {
        env.push((key.to_string(), value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table_exposes_the_three_reference_hacks() {
        let all: Vec<(String, String)> = builtin_ids()
            .iter()
            .flat_map(|id| builtin_tricks(id))
            .collect();
        assert!(all.iter().any(|(key, _)| key == "OPENSSL_ia32cap"));
        assert!(all
            .iter()
            .any(|(key, _)| key == "WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER"));
        assert!(all
            .iter()
            .any(|(key, _)| key == "WINE_DISABLE_HARDWARE_SCHEDULING"));
    }

    #[test]
    fn builtin_lookup_is_case_insensitive_and_sorted() {
        let tricks = builtin_tricks("DXGI-NO-DEVICE-MANAGER");
        assert_eq!(
            tricks,
            vec![(
                "WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER".to_string(),
                "1".to_string()
            )]
        );
        assert!(builtin_tricks("unknown-game").is_empty());
        assert!(builtin_ids().len() >= 3);
    }

    #[test]
    fn custom_tricks_override_builtins_and_are_local() {
        let id = "dxgi-no-device-manager";
        let _ = remove_custom_tricks(id); // clean only this id, never the whole store
        register_trick(id, "WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER", "0");
        register_trick(id, "EXTRA_FLAG", "on");
        let tricks = tricks_for(id);
        assert_eq!(
            tricks
                .iter()
                .find(|(k, _)| k == "WINE_DO_NOT_CREATE_DXGI_DEVICE_MANAGER")
                .map(|(_, v)| v.as_str()),
            Some("0")
        );
        assert_eq!(
            tricks
                .iter()
                .find(|(k, _)| k == "EXTRA_FLAG")
                .map(|(_, v)| v.as_str()),
            Some("on")
        );
        // Custom entries are keyed by id and do not leak to other games.
        assert!(custom_tricks("another-game").is_empty());
        assert_eq!(remove_custom_tricks(id).len(), 2);
        assert_eq!(tricks_for(id).len(), 1);
    }

    #[test]
    fn add_custom_tricks_sorts_and_ignores_empty_keys() {
        let id = "test-app-42";
        let _ = remove_custom_tricks(id);
        add_custom_tricks(
            id,
            vec![
                ("Z_FLAG".into(), "z".into()),
                ("".into(), "ignored".into()),
                ("A_FLAG".into(), "a".into()),
            ],
        );
        let tricks = tricks_for(id);
        assert_eq!(tricks[0].0, "A_FLAG");
        assert_eq!(tricks[1].0, "Z_FLAG");
        assert_eq!(tricks.len(), 2);
        let _ = remove_custom_tricks(id);
    }
}
