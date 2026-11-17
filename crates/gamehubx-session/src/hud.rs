//! Reference-session HUD model.
//!
//! This module owns the small, serde-friendly values a HUD overlay renders and
//! a bounded history used for sparklines (notably the FPS graph). It performs
//! no rendering and has no platform dependencies: a producer pushes
//! [`HudSnapshot`]s and the UI reads the latest value or iterates the ring.

use serde::{Deserialize, Serialize};

/// Number of snapshots retained by [`HudHistory`]. At 60 samples/second this
/// is roughly two seconds of history for the FPS graph.
pub const HUD_HISTORY_CAPACITY: usize = 120;

/// One instant of HUD telemetry. All fields are optional where a source may be
/// unavailable (for example GPU utilization on a device without a counter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudSnapshot {
    /// Frames per second.
    pub fps: u32,
    /// Proportional set size in kilobytes.
    pub pss_kb: u64,
    /// Battery charge percentage, when reported.
    pub battery_pct: Option<u8>,
    /// CPU utilization percentage, when reported.
    pub cpu_pct: Option<u8>,
    /// GPU utilization percentage, when reported.
    pub gpu_pct: Option<u8>,
    /// Network throughput in kilobits per second, when reported.
    pub net_kbps: Option<u64>,
}

/// User-facing HUD toggles.
///
/// `transparency` is a percentage in `0..=100`: `0` is fully transparent and
/// `100` is fully opaque. [`HudConfig::transparency_clamped`] enforces that
/// range for values coming from untrusted configuration files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HudConfig {
    pub show_fps_graph: bool,
    pub show_cpu: bool,
    pub show_gpu: bool,
    pub show_ram: bool,
    pub show_power: bool,
    pub show_net: bool,
    pub dual_battery: bool,
    pub transparency: u8,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            show_fps_graph: true,
            show_cpu: true,
            show_gpu: true,
            show_ram: true,
            show_power: true,
            show_net: true,
            dual_battery: false,
            transparency: 85,
        }
    }
}

impl HudConfig {
    /// Returns `transparency` clamped to `0..=100`.
    pub fn transparency_clamped(&self) -> u8 {
        self.transparency.min(100)
    }
}

/// A fixed-capacity ring of [`HudSnapshot`]s, oldest to newest.
///
/// Pushing into a full ring overwrites the oldest sample. The backing storage
/// is a fixed array, so the history never allocates and cannot grow.
#[derive(Clone, Debug)]
pub struct HudHistory {
    slots: [Option<HudSnapshot>; HUD_HISTORY_CAPACITY],
    head: usize,
    len: usize,
}

impl Default for HudHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl HudHistory {
    pub fn new() -> Self {
        Self {
            slots: [None; HUD_HISTORY_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    pub const fn capacity(&self) -> usize {
        HUD_HISTORY_CAPACITY
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len == HUD_HISTORY_CAPACITY
    }

    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
        self.head = 0;
        self.len = 0;
    }

    /// Append a snapshot, evicting the oldest one when the ring is full.
    pub fn push(&mut self, snapshot: HudSnapshot) {
        let index = (self.head + self.len) % HUD_HISTORY_CAPACITY;
        self.slots[index] = Some(snapshot);
        if self.len == HUD_HISTORY_CAPACITY {
            self.head = (self.head + 1) % HUD_HISTORY_CAPACITY;
        } else {
            self.len += 1;
        }
    }

    /// The most recently pushed snapshot, if any.
    pub fn latest(&self) -> Option<HudSnapshot> {
        if self.len == 0 {
            return None;
        }
        let index = (self.head + self.len - 1) % HUD_HISTORY_CAPACITY;
        self.slots[index]
    }

    /// Arithmetic mean of the retained FPS samples, or `0.0` when empty.
    pub fn average_fps(&self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        let total: u64 = self.iter().map(|snapshot| snapshot.fps as u64).sum();
        total as f32 / self.len as f32
    }

    /// Iterate snapshots oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &HudSnapshot> + '_ {
        let split = (self.head + self.len).min(HUD_HISTORY_CAPACITY);
        let first = &self.slots[self.head..split];
        let second_len = self.len.saturating_sub(split - self.head);
        let second = &self.slots[..second_len];
        first.iter().chain(second.iter()).filter_map(Option::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(fps: u32) -> HudSnapshot {
        HudSnapshot {
            fps,
            pss_kb: 1024,
            battery_pct: Some(75),
            cpu_pct: Some(20),
            gpu_pct: None,
            net_kbps: Some(900),
        }
    }

    #[test]
    fn snapshot_serde_round_trip() {
        let original = snapshot(60);
        let json = serde_json::to_string(&original).unwrap();
        let decoded: HudSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn config_defaults_and_serde_round_trip() {
        let config = HudConfig::default();
        assert!(config.show_fps_graph && config.show_cpu && config.show_gpu);
        assert!(config.show_ram && config.show_power && config.show_net);
        assert!(!config.dual_battery);
        assert_eq!(config.transparency_clamped(), 85);

        let json = serde_json::to_string(&config).unwrap();
        let decoded: HudConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, config);

        let partial: HudConfig =
            serde_json::from_str(r#"{"show_fps_graph":false,"transparency":200}"#).unwrap();
        assert!(!partial.show_fps_graph);
        assert_eq!(partial.transparency, 200);
        assert_eq!(partial.transparency_clamped(), 100);
    }

    #[test]
    fn empty_history_has_no_samples() {
        let history = HudHistory::new();
        assert_eq!(history.capacity(), HUD_HISTORY_CAPACITY);
        assert!(history.is_empty());
        assert_eq!(history.latest(), None);
        assert_eq!(history.average_fps(), 0.0);
        assert_eq!(history.iter().count(), 0);
    }

    #[test]
    fn history_pushes_and_averages_in_order() {
        let mut history = HudHistory::new();
        history.push(snapshot(30));
        history.push(snapshot(60));
        history.push(snapshot(90));

        assert_eq!(history.len(), 3);
        assert_eq!(history.latest(), Some(snapshot(90)));
        assert_eq!(history.average_fps(), 60.0);
        let fps: Vec<u32> = history.iter().map(|sample| sample.fps).collect();
        assert_eq!(fps, vec![30, 60, 90]);
    }

    #[test]
    fn history_is_bounded_and_drops_oldest() {
        let mut history = HudHistory::new();
        for fps in 0..(HUD_HISTORY_CAPACITY as u32 * 2) {
            history.push(snapshot(fps));
        }

        assert_eq!(history.len(), HUD_HISTORY_CAPACITY);
        assert!(history.is_full());
        assert_eq!(
            history.latest(),
            Some(snapshot(HUD_HISTORY_CAPACITY as u32 * 2 - 1))
        );
        assert_eq!(history.iter().count(), HUD_HISTORY_CAPACITY);
        let first = history.iter().next().unwrap();
        assert_eq!(first.fps, HUD_HISTORY_CAPACITY as u32);
    }

    #[test]
    fn clear_empties_the_ring() {
        let mut history = HudHistory::new();
        for fps in 0..10 {
            history.push(snapshot(fps));
        }
        history.clear();
        assert!(history.is_empty());
        assert_eq!(history.latest(), None);
        assert_eq!(history.iter().count(), 0);

        history.push(snapshot(120));
        assert_eq!(history.latest(), Some(snapshot(120)));
    }
}
