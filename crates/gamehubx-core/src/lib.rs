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
    // Extended per-game settings. These back the reference Game Settings rows
    // that used to be static labels. `#[serde(default)]` keeps older persisted
    // JSON loadable when fields are added.
    #[serde(default = "default_container_language")]
    pub container_language: String,
    #[serde(default)]
    pub mangohud: bool,
    #[serde(default = "default_directx_hud")]
    pub directx_hud: String,
    #[serde(default)]
    pub disable_wm: bool,
    #[serde(default)]
    pub env_vars: Vec<(String, String)>,
    #[serde(default)]
    pub command_line: String,
    #[serde(default = "default_true")]
    pub auto_keyboard: bool,
    #[serde(default)]
    pub disable_gfx_plugin: bool,
    #[serde(default = "default_compat_layer")]
    pub compat_layer: String,
    #[serde(default = "default_translation_params")]
    pub translation_params: String,
    #[serde(default = "default_dinput_library")]
    pub dinput_library: String,
    #[serde(default = "default_gpu_driver")]
    pub gpu_driver: String,
    #[serde(default = "default_surface_format")]
    pub surface_format: String,
    #[serde(default = "default_vkd3d_version")]
    pub vkd3d_version: String,
    #[serde(default)]
    pub log_server: bool,
    #[serde(default = "default_wine_debug")]
    pub wine_debug_params: String,
    #[serde(default)]
    pub cpu_core_limit: u32,
    #[serde(default = "default_overlay_opacity")]
    pub overlay_opacity: u8,
    #[serde(default)]
    pub rts_gestures: bool,
    #[serde(default = "default_config_preset")]
    pub config_preset: String,
}

fn default_container_language() -> String {
    "en_US".to_string()
}
fn default_directx_hud() -> String {
    "Disable".to_string()
}
fn default_true() -> bool {
    true
}
fn default_compat_layer() -> String {
    "proton10.0-arm64x-2".to_string()
}
fn default_translation_params() -> String {
    "Game Presets".to_string()
}
fn default_dinput_library() -> String {
    "Prefer Native".to_string()
}
fn default_gpu_driver() -> String {
    "System Driver".to_string()
}
fn default_surface_format() -> String {
    "RGBA8".to_string()
}
fn default_vkd3d_version() -> String {
    "Default".to_string()
}
fn default_wine_debug() -> String {
    "-all".to_string()
}
fn default_overlay_opacity() -> u8 {
    100
}
fn default_config_preset() -> String {
    "Balanced".to_string()
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
            container_language: default_container_language(),
            mangohud: false,
            directx_hud: default_directx_hud(),
            disable_wm: false,
            env_vars: Vec::new(),
            command_line: String::new(),
            auto_keyboard: default_true(),
            disable_gfx_plugin: false,
            compat_layer: default_compat_layer(),
            translation_params: default_translation_params(),
            dinput_library: default_dinput_library(),
            gpu_driver: default_gpu_driver(),
            surface_format: default_surface_format(),
            vkd3d_version: default_vkd3d_version(),
            log_server: false,
            wine_debug_params: default_wine_debug(),
            cpu_core_limit: 0,
            overlay_opacity: default_overlay_opacity(),
            rts_gestures: false,
            config_preset: default_config_preset(),
        }
    }
}

impl GameSettings {
    pub fn resolution_label(&self) -> String {
        format!("{} x {}", self.width, self.height)
    }

    /// True when any performance HUD overlay (MangoHUD or DirectX/DXVK HUD)
    /// is enabled. Drives both the session env and the in-app HUD layer.
    pub fn hud_enabled(&self) -> bool {
        self.mangohud || self.directx_hud != "Disable"
    }

    /// Map the DirectX HUD choice to the `DXVK_HUD` environment value.
    pub fn dxvk_hud_value(&self) -> &'static str {
        match self.directx_hud.as_str() {
            "FPS" => "fps",
            "Full" => "full",
            _ => "0",
        }
    }

    /// Apply a named compatibility preset. Presets are GPU-vendor agnostic,
    /// known-good bundles that mirror the reference's "recommended config"
    /// idea without depending on a server or proprietary database.
    pub fn apply_preset(&mut self, name: &str) -> bool {
        match name {
            "Balanced" => {
                self.width = 1280;
                self.height = 720;
                self.fps_cap = 60;
                self.dxvk_version = "DXVK".to_string();
                self.directx_hud = "FPS".to_string();
                self.mangohud = false;
                self.disable_wm = false;
                self.wine_debug_params = "-all".to_string();
                self.disable_gfx_plugin = false;
            }
            "Performance" => {
                self.width = 960;
                self.height = 540;
                self.fps_cap = 60;
                self.dxvk_version = "DXVK".to_string();
                self.directx_hud = "FPS".to_string();
                self.mangohud = true;
                self.disable_wm = true;
                self.wine_debug_params = "-all".to_string();
                self.disable_gfx_plugin = true;
            }
            "Compatibility" => {
                self.width = 1280;
                self.height = 720;
                self.fps_cap = 30;
                self.dxvk_version = "Default".to_string();
                self.directx_hud = "Disable".to_string();
                self.mangohud = false;
                self.disable_wm = false;
                self.wine_debug_params = "fixme-all".to_string();
                self.disable_gfx_plugin = true;
            }
            _ => return false,
        }
        self.config_preset = name.to_string();
        true
    }
}

/// Cycle options for the per-game settings rows. Kept in the model so the UI
/// and any headless consumer share the exact same value set.
pub const CONTAINER_LANGUAGES: &[&str] = &["en_US", "zh_CN", "ja_JP", "ru_RU"];
pub const DIRECTX_HUD_OPTIONS: &[&str] = &["Disable", "FPS", "Full"];
pub const COMPAT_LAYERS: &[&str] = &["proton10.0-arm64x-2", "wine10.0-arm64ec", "wine9.0-arm64ec"];
pub const TRANSLATION_PRESETS: &[&str] = &[
    "Game Presets",
    "Box64 Optimized",
    "FEX Compatible",
    "Conservative",
];
pub const DINPUT_OPTIONS: &[&str] = &["Prefer Native", "Prefer Wine", "Disabled"];
pub const GPU_DRIVER_OPTIONS: &[&str] = &["System Driver", "Turnip", "VirGL"];
pub const SURFACE_FORMAT_OPTIONS: &[&str] = &["RGBA8", "BGRA8", "RGB565"];
pub const VKD3D_OPTIONS: &[&str] = &["Default", "VKD3D", "Disabled"];
pub const CONFIG_PRESETS: &[&str] = &["Balanced", "Performance", "Compatibility"];

/// Advance `current` to the next option in `options`, wrapping around.
pub fn cycle_option(options: &[&str], current: &str) -> String {
    let position = options
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    options[(position + 1) % options.len()].to_string()
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
        assert_eq!(settings.container_language, "en_US");
        assert_eq!(settings.directx_hud, "Disable");
        assert_eq!(settings.dxvk_hud_value(), "0");
        assert_eq!(settings.config_preset, "Balanced");
        assert!(!settings.hud_enabled());
    }

    #[test]
    fn extended_settings_serde_roundtrip() {
        let mut settings = GameSettings::default();
        settings
            .env_vars
            .push(("LIBGL_ALWAYS_SOFTWARE".into(), "1".into()));
        settings.mangohud = true;
        settings.directx_hud = "Full".into();
        settings.cpu_core_limit = 4;
        settings.rts_gestures = true;
        let json = serde_json::to_string(&settings).unwrap();
        let loaded: GameSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn presets_change_settings_and_hud() {
        let mut settings = GameSettings::default();
        assert!(settings.apply_preset("Performance"));
        assert_eq!(settings.fps_cap, 60);
        assert_eq!(settings.width, 960);
        assert!(settings.mangohud);
        assert!(settings.hud_enabled());
        assert_eq!(settings.dxvk_hud_value(), "fps");
        assert!(settings.apply_preset("Compatibility"));
        assert!(!settings.hud_enabled());
        assert_eq!(settings.config_preset, "Compatibility");
        assert!(!settings.apply_preset("nope"));
    }

    #[test]
    fn cycle_options_wraps() {
        assert_eq!(cycle_option(DIRECTX_HUD_OPTIONS, "Disable"), "FPS");
        assert_eq!(cycle_option(DIRECTX_HUD_OPTIONS, "Full"), "Disable");
        assert_eq!(cycle_option(DIRECTX_HUD_OPTIONS, "unknown"), "FPS");
    }
}
