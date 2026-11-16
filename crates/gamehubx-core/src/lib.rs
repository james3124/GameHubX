use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameKind {
    PcExe,
    Steam,
    Android,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub kind: GameKind,
    pub label: String,
    pub intro: String,
    pub exe: Option<PathBuf>,
    pub cover: Option<PathBuf>,
    pub container: Option<String>,
    pub created_at: i64,
    pub last_played: Option<i64>,
    pub favorite: bool,
}

impl Game {
    pub fn new_pc_exe(label: impl Into<String>, intro: impl Into<String>, exe: PathBuf) -> Self {
        let id = stable_id(GameKind::PcExe, &exe.to_string_lossy());
        Self {
            id,
            kind: GameKind::PcExe,
            label: label.into(),
            intro: intro.into(),
            exe: Some(exe),
            cover: None,
            container: None,
            created_at: unix_now(),
            last_played: None,
            favorite: false,
        }
    }

    pub fn local_id(&self) -> String {
        format!("local_{:016x}", self.id.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSettings {
    pub width: u32,
    pub height: u32,
    pub fps_cap: u32,
    pub dxvk_version: String,
    pub audio_backend: String,
    pub touch_profile: String,
    pub memory_limit_mb: u32,
    pub cpu_affinity_big: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            fps_cap: 60,
            dxvk_version: "Default".to_string(),
            audio_backend: "Pulse".to_string(),
            touch_profile: "Gamepad".to_string(),
            memory_limit_mb: 0,
            cpu_affinity_big: false,
        }
    }
}

impl GameSettings {
    pub fn resolution_label(&self) -> String {
        format!("{} x {}", self.width, self.height)
    }
}

pub fn stable_id(kind: GameKind, key: &str) -> GameId {
    let tag = match kind {
        GameKind::PcExe => 1u8,
        GameKind::Steam => 2,
        GameKind::Android => 3,
    };
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    hash ^= u64::from(tag);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    GameId(hash)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_path_sensitive() {
        let a = stable_id(GameKind::PcExe, "/games/a.exe");
        let b = stable_id(GameKind::PcExe, "/games/a.exe");
        let c = stable_id(GameKind::PcExe, "/games/b.exe");
        let d = stable_id(GameKind::Android, "/games/a.exe");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn local_id_is_display_safe() {
        let game = Game::new_pc_exe("Test", "", PathBuf::from("/games/test.exe"));
        let id = game.local_id();
        assert!(id.starts_with("local_"));
        assert_eq!(id.len(), "local_".len() + 16);
    }

    #[test]
    fn settings_defaults_are_stable() {
        let settings = GameSettings::default();
        assert_eq!(settings.resolution_label(), "1280 x 720");
        assert_eq!(settings.fps_cap, 60);
        assert_eq!(settings.audio_backend, "Pulse");
        assert_eq!(settings.touch_profile, "Gamepad");
        assert_eq!(settings.memory_limit_mb, 0);
        assert!(!settings.cpu_affinity_big);
    }
}
