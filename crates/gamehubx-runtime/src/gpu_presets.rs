//! Lightweight GPU-to-preset recommendation.
//!
//! Maps a vendor/device description string (e.g. from `lspci` or the Android
//! GPU info API) to one of the three graphics config preset names used by
//! settings UIs. This is advisory only: nothing else in the runtime is wired
//! to it.

/// Balanced preset name (safe default for unknown GPUs and Intel iGPUs).
pub const PRESET_BALANCED: &str = "Balanced";
/// Performance preset name (discrete NVIDIA / AMD GPUs).
pub const PRESET_PERFORMANCE: &str = "Performance";
/// Compatibility preset name (Qualcomm / Adreno mobile GPUs).
pub const PRESET_COMPATIBILITY: &str = "Compatibility";

/// All preset names this module can return.
pub const fn preset_names() -> &'static [&'static str] {
    &[PRESET_BALANCED, PRESET_PERFORMANCE, PRESET_COMPATIBILITY]
}

/// Recommend a config preset for a GPU vendor/device string.
///
/// Matching is case-insensitive substring matching, in priority order:
///
/// - `nvidia`, `geforce`, `rtx`, `gtx` → [`PRESET_PERFORMANCE`]
/// - `amd`, `radeon`, `rx ` → [`PRESET_PERFORMANCE`]
/// - `qualcomm`, `adreno` → [`PRESET_COMPATIBILITY`]
/// - `intel`, `iris`, `uhd graphics`, `arc ` → [`PRESET_BALANCED`]
/// - anything else → [`PRESET_BALANCED`]
pub fn recommend_for_gpu(vendor_device: &str) -> &'static str {
    let lower = vendor_device.to_lowercase();
    if contains_any(&lower, &["nvidia", "geforce", "quadro", "rtx ", "gtx "]) {
        return PRESET_PERFORMANCE;
    }
    if contains_any(
        &lower,
        &[
            "amd",
            "advanced micro devices",
            "radeon",
            "rx 5",
            "rx 6",
            "rx 7",
        ],
    ) {
        return PRESET_PERFORMANCE;
    }
    if contains_any(&lower, &["qualcomm", "adreno"]) {
        return PRESET_COMPATIBILITY;
    }
    if contains_any(&lower, &["intel", "iris", "uhd graphics", "arc "]) {
        return PRESET_BALANCED;
    }
    PRESET_BALANCED
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvidia_and_amd_get_performance() {
        assert_eq!(
            recommend_for_gpu("NVIDIA GeForce RTX 3080"),
            PRESET_PERFORMANCE
        );
        assert_eq!(
            recommend_for_gpu("nvidia corporation gp104 [geforce gtx 1080]"),
            PRESET_PERFORMANCE
        );
        assert_eq!(
            recommend_for_gpu("AMD Radeon RX 6700 XT"),
            PRESET_PERFORMANCE
        );
        assert_eq!(
            recommend_for_gpu("Advanced Micro Devices"),
            PRESET_PERFORMANCE
        );
    }

    #[test]
    fn qualcomm_gets_compatibility() {
        assert_eq!(
            recommend_for_gpu("Qualcomm Adreno 740"),
            PRESET_COMPATIBILITY
        );
        assert_eq!(recommend_for_gpu("ADRENO (TM) 6xx"), PRESET_COMPATIBILITY);
    }

    #[test]
    fn intel_and_unknown_get_balanced() {
        assert_eq!(
            recommend_for_gpu("Intel Corporation Iris Xe Graphics"),
            PRESET_BALANCED
        );
        assert_eq!(recommend_for_gpu("Intel UHD Graphics 630"), PRESET_BALANCED);
        assert_eq!(recommend_for_gpu("Mali-G78 MP14"), PRESET_BALANCED);
        assert_eq!(recommend_for_gpu(""), PRESET_BALANCED);
        assert_eq!(preset_names().len(), 3);
    }
}
