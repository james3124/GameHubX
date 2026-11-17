//! Translation-layer configuration (Box64 / FEX) for emulated guests.
//!
//! Each config is a plain serde struct so it can be persisted in game settings.
//! [`Box64Config::env_vars`] / [`FexConfig::env_vars`] turn the knobs into the
//! environment-variable pairs that a session launches with. Presets mirror the
//! reference launcher's translation presets (Default / Optimized / Compatible /
//! Conservative) and are exposed through [`EmulationPreset`].

use serde::{Deserialize, Serialize};

/// The four translation presets shared by Box64 and FEX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmulationPreset {
    Default,
    Optimized,
    Compatible,
    Conservative,
}

impl Default for EmulationPreset {
    fn default() -> Self {
        Self::Default
    }
}

impl EmulationPreset {
    /// Stable label used by settings UIs / persistence.
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Optimized => "Optimized",
            Self::Compatible => "Compatible",
            Self::Conservative => "Conservative",
        }
    }

    /// Parse a preset from a (case-insensitive) label.
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("optimized") {
            Some(Self::Optimized)
        } else if value.eq_ignore_ascii_case("compatible") {
            Some(Self::Compatible)
        } else if value.eq_ignore_ascii_case("conservative") {
            Some(Self::Conservative)
        } else if value.eq_ignore_ascii_case("default") {
            Some(Self::Default)
        } else {
            None
        }
    }

    /// All presets, in display order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Default,
            Self::Optimized,
            Self::Compatible,
            Self::Conservative,
        ]
    }
}

/// Box64 dynarec knobs. Booleans map to the canonical `1`/`0` env values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Box64Config {
    pub dynarec: bool,
    pub avx: bool,
    pub tso: bool,
    pub x87: bool,
    pub mmap32: bool,
    pub dynacache: bool,
}

impl Default for Box64Config {
    fn default() -> Self {
        Self::for_preset(EmulationPreset::Default)
    }
}

impl From<EmulationPreset> for Box64Config {
    fn from(preset: EmulationPreset) -> Self {
        Self::for_preset(preset)
    }
}

impl Box64Config {
    pub fn for_preset(preset: EmulationPreset) -> Self {
        match preset {
            EmulationPreset::Default => Self {
                dynarec: true,
                avx: false,
                tso: false,
                x87: true,
                mmap32: false,
                dynacache: true,
            },
            EmulationPreset::Optimized => Self {
                dynarec: true,
                avx: true,
                tso: false,
                x87: true,
                mmap32: true,
                dynacache: true,
            },
            EmulationPreset::Compatible => Self {
                dynarec: true,
                avx: false,
                tso: true,
                x87: true,
                mmap32: true,
                dynacache: false,
            },
            EmulationPreset::Conservative => Self {
                dynarec: false,
                avx: false,
                tso: true,
                x87: true,
                mmap32: true,
                dynacache: false,
            },
        }
    }

    /// Generate the Box64 environment-variable pairs for this config.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let bit = |enabled: bool| -> String {
            if enabled {
                "1".to_string()
            } else {
                "0".to_string()
            }
        };
        vec![
            ("BOX64_DYNAREC".into(), bit(self.dynarec)),
            ("BOX64_AVX".into(), bit(self.avx)),
            ("BOX64_TSO".into(), bit(self.tso)),
            ("BOX64_X87".into(), bit(self.x87)),
            ("BOX64_MMAP32".into(), bit(self.mmap32)),
            ("BOX64_DYNAREC_CACHE".into(), bit(self.dynacache)),
        ]
    }
}

/// FEX translation knobs. `force_sve_width` of `0` means "auto".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FexConfig {
    pub multiblock: bool,
    pub smc_checks: bool,
    pub force_sve_width: u32,
}

impl Default for FexConfig {
    fn default() -> Self {
        Self::for_preset(EmulationPreset::Default)
    }
}

impl From<EmulationPreset> for FexConfig {
    fn from(preset: EmulationPreset) -> Self {
        Self::for_preset(preset)
    }
}

impl FexConfig {
    pub fn for_preset(preset: EmulationPreset) -> Self {
        match preset {
            EmulationPreset::Default => Self {
                multiblock: true,
                smc_checks: true,
                force_sve_width: 0,
            },
            EmulationPreset::Optimized => Self {
                multiblock: true,
                smc_checks: false,
                force_sve_width: 256,
            },
            EmulationPreset::Compatible => Self {
                multiblock: false,
                smc_checks: true,
                force_sve_width: 0,
            },
            EmulationPreset::Conservative => Self {
                multiblock: false,
                smc_checks: true,
                force_sve_width: 128,
            },
        }
    }

    /// Generate the FEX environment-variable pairs for this config.
    pub fn env_vars(&self) -> Vec<(String, String)> {
        let bit = |enabled: bool| -> String {
            if enabled {
                "1".to_string()
            } else {
                "0".to_string()
            }
        };
        vec![
            ("FEX_MULTIBLOCK".into(), bit(self.multiblock)),
            ("FEX_SMCCHECKS".into(), bit(self.smc_checks)),
            ("FEX_FORCESVEWIDTH".into(), self.force_sve_width.to_string()),
        ]
    }
}

/// Convenience: Box64 env pairs for a preset.
pub fn box64_env(preset: EmulationPreset) -> Vec<(String, String)> {
    Box64Config::for_preset(preset).env_vars()
}

/// Convenience: FEX env pairs for a preset.
pub fn fex_env(preset: EmulationPreset) -> Vec<(String, String)> {
    FexConfig::for_preset(preset).env_vars()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn presets_cover_all_four_with_stable_labels() {
        assert_eq!(EmulationPreset::all().len(), 4);
        for preset in EmulationPreset::all() {
            assert_eq!(EmulationPreset::parse(preset.label()), Some(*preset));
            assert_eq!(
                EmulationPreset::parse(&preset.label().to_lowercase()),
                Some(*preset)
            );
        }
        assert_eq!(EmulationPreset::default(), EmulationPreset::Default);
        assert_eq!(EmulationPreset::parse("nonsense"), None);
    }

    #[test]
    fn box64_default_env_has_all_reference_knobs() {
        let env = Box64Config::default().env_vars();
        assert_eq!(env.len(), 6);
        assert_eq!(value(&env, "BOX64_DYNAREC"), Some("1"));
        assert_eq!(value(&env, "BOX64_AVX"), Some("0"));
        assert_eq!(value(&env, "BOX64_TSO"), Some("0"));
        assert_eq!(value(&env, "BOX64_X87"), Some("1"));
        assert_eq!(value(&env, "BOX64_MMAP32"), Some("0"));
        assert_eq!(value(&env, "BOX64_DYNAREC_CACHE"), Some("1"));
    }

    #[test]
    fn box64_presets_change_knobs() {
        let optimized = box64_env(EmulationPreset::Optimized);
        assert_eq!(value(&optimized, "BOX64_AVX"), Some("1"));
        assert_eq!(value(&optimized, "BOX64_MMAP32"), Some("1"));

        let conservative = box64_env(EmulationPreset::Conservative);
        assert_eq!(value(&conservative, "BOX64_DYNAREC"), Some("0"));
        assert_eq!(value(&conservative, "BOX64_DYNAREC_CACHE"), Some("0"));

        let compatible = box64_env(EmulationPreset::Compatible);
        assert_eq!(value(&compatible, "BOX64_TSO"), Some("1"));
    }

    #[test]
    fn fex_env_has_reference_knobs_and_width() {
        let env = fex_env(EmulationPreset::Default);
        assert_eq!(env.len(), 3);
        assert_eq!(value(&env, "FEX_MULTIBLOCK"), Some("1"));
        assert_eq!(value(&env, "FEX_SMCCHECKS"), Some("1"));
        assert_eq!(value(&env, "FEX_FORCESVEWIDTH"), Some("0"));

        let optimized = fex_env(EmulationPreset::Optimized);
        assert_eq!(value(&optimized, "FEX_SMCCHECKS"), Some("0"));
        assert_eq!(value(&optimized, "FEX_FORCESVEWIDTH"), Some("256"));

        let compatible = fex_env(EmulationPreset::Compatible);
        assert_eq!(value(&compatible, "FEX_MULTIBLOCK"), Some("0"));
    }

    #[test]
    fn configs_are_serde_roundtrippable_and_from_preset() {
        let boxed: Box64Config = EmulationPreset::Optimized.into();
        let json = serde_json::to_string(&boxed).unwrap();
        let decoded: Box64Config = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, boxed);

        let fex: FexConfig = EmulationPreset::Conservative.into();
        assert_eq!(fex.force_sve_width, 128);
        let json = serde_json::to_string(&fex).unwrap();
        let decoded: FexConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, fex);
    }
}
