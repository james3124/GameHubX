//! Serde-backed data structures for advanced control plans: rapid fire,
//! recorded macros and RTS-style touch gestures.
//!
//! These are deliberately pure data + validation helpers.  The runtime
//! [`crate::Engine`] keeps its existing shape; a backend can wire these plans
//! in by feeding resolved [`crate::Action`]s into
//! [`crate::state::GamepadState`] or by replaying macro events as
//! [`crate::InputEvent`]s.

use serde::{Deserialize, Serialize};

use crate::action::{is_valid_binding, map_binding, Action};
use crate::engine::InputEvent;

/// Errors returned by plan validation and recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    EmptyName,
    InvalidBinding,
    BadInterval,
    BadPreheat,
    BadOffset,
    EmptyMacro,
    UnsortedTimestamps,
    BadValue,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PlanError {}

/// A rapid-fire ("shooting") plan bound to profile buttons.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShootingPlan {
    #[serde(default = "default_none")]
    pub fire_button: String,
    #[serde(default = "default_none")]
    pub toggle_button: String,
    #[serde(default = "default_none")]
    pub hotkey: String,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default)]
    pub preheat_ms: u64,
    #[serde(default)]
    pub offset: f32,
}

fn default_none() -> String {
    "NONE".to_string()
}

fn default_interval_ms() -> u64 {
    100
}

impl Default for ShootingPlan {
    fn default() -> Self {
        Self {
            fire_button: default_none(),
            toggle_button: default_none(),
            hotkey: default_none(),
            interval_ms: default_interval_ms(),
            preheat_ms: 0,
            offset: 0.0,
        }
    }
}

impl ShootingPlan {
    /// Checks that the fire button is bound, optional bindings are valid and
    /// timing values are within sane bounds.
    pub fn validate(&self) -> Result<(), PlanError> {
        if !is_bound(&self.fire_button) {
            return Err(PlanError::InvalidBinding);
        }
        if !is_valid_binding(&self.toggle_button) || !is_valid_binding(&self.hotkey) {
            return Err(PlanError::InvalidBinding);
        }
        if !(1..=60_000).contains(&self.interval_ms) {
            return Err(PlanError::BadInterval);
        }
        if self.preheat_ms > 10_000 {
            return Err(PlanError::BadPreheat);
        }
        if !self.offset.is_finite() || !(-1.0..=1.0).contains(&self.offset) {
            return Err(PlanError::BadOffset);
        }
        Ok(())
    }

    /// The resolved action for the fire button.
    pub fn fire_action(&self) -> Action {
        map_binding(&self.fire_button)
    }
}

fn is_bound(binding: &str) -> bool {
    let trimmed = binding.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case("NONE")
        && !trimmed.eq_ignore_ascii_case("NULL")
        && is_valid_binding(trimmed)
}

/// The kind of input recorded in a [`MacroPoint`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroEventKind {
    #[serde(rename = "PRESS", alias = "press", alias = "Press")]
    Press,
    #[serde(rename = "RELEASE", alias = "release", alias = "Release")]
    Release,
    #[serde(rename = "AXIS", alias = "axis", alias = "Axis")]
    Axis,
    #[serde(rename = "MOUSE_MOVE", alias = "mouseMove", alias = "MouseMove")]
    MouseMove,
}

/// One timestamped point of a recorded macro.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroPoint {
    /// Milliseconds since the start of the recording.
    pub t_ms: u64,
    pub kind: MacroEventKind,
    /// Binding string for press/release/axis events (ignored for mouse moves).
    #[serde(default)]
    pub binding: String,
    /// Analog magnitude for axis events; for mouse moves this is the X delta.
    #[serde(default)]
    pub value: f32,
}

impl MacroPoint {
    pub fn new(t_ms: u64, kind: MacroEventKind, binding: impl Into<String>, value: f32) -> Self {
        Self {
            t_ms,
            kind,
            binding: binding.into(),
            value,
        }
    }

    /// Converts this point into a neutral input event.
    pub fn to_event(&self) -> InputEvent {
        match self.kind {
            MacroEventKind::Press => InputEvent::Press(self.binding.clone()),
            MacroEventKind::Release => InputEvent::Release(self.binding.clone()),
            MacroEventKind::Axis => InputEvent::Axis(self.binding.clone(), self.value),
            MacroEventKind::MouseMove => InputEvent::MouseMove {
                dx: self.value,
                dy: 0.0,
                speed: 1.0,
            },
        }
    }
}

/// A recorded sequence of timestamped points, with optional looping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Macro {
    #[serde(default)]
    pub name: String,
    /// Number of extra playbacks after the first one. `0` plays once.
    #[serde(default)]
    pub loop_count: u32,
    #[serde(default)]
    pub points: Vec<MacroPoint>,
}

impl Default for Macro {
    fn default() -> Self {
        Self {
            name: String::new(),
            loop_count: 0,
            points: Vec::new(),
        }
    }
}

impl Macro {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            loop_count: 0,
            points: Vec::new(),
        }
    }

    /// Appends a recorded point. Timestamps must be non-decreasing.
    pub fn record(
        &mut self,
        t_ms: u64,
        kind: MacroEventKind,
        binding: impl Into<String>,
        value: f32,
    ) -> Result<(), PlanError> {
        if self.points.last().is_some_and(|last| t_ms < last.t_ms) {
            return Err(PlanError::UnsortedTimestamps);
        }
        self.points
            .push(MacroPoint::new(t_ms, kind, binding, value));
        Ok(())
    }

    /// Timestamp of the last point (the macro duration).
    pub fn duration_ms(&self) -> u64 {
        self.points.last().map(|point| point.t_ms).unwrap_or(0)
    }

    /// Converts all points to input events in recording order.
    pub fn to_events(&self) -> Vec<InputEvent> {
        self.points.iter().map(MacroPoint::to_event).collect()
    }

    /// Replays points that became due between `last_elapsed_ms` and
    /// `elapsed_ms`, advancing `cursor` to the next point.  This is a
    /// frame-friendly incremental replay primitive.
    pub fn replay_at(&self, elapsed_ms: u64, cursor: &mut usize) -> Vec<InputEvent> {
        let mut events = Vec::new();
        while *cursor < self.points.len() && self.points[*cursor].t_ms <= elapsed_ms {
            events.push(self.points[*cursor].to_event());
            *cursor += 1;
        }
        events
    }

    /// Validates names, bindings, ordering and values.
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.name.trim().is_empty() {
            return Err(PlanError::EmptyName);
        }
        if self.points.is_empty() {
            return Err(PlanError::EmptyMacro);
        }
        let mut previous = 0;
        for point in &self.points {
            if point.t_ms < previous {
                return Err(PlanError::UnsortedTimestamps);
            }
            previous = point.t_ms;
            if !point.value.is_finite() {
                return Err(PlanError::BadValue);
            }
            match point.kind {
                MacroEventKind::Press | MacroEventKind::Release => {
                    if !is_bound(&point.binding) {
                        return Err(PlanError::InvalidBinding);
                    }
                }
                MacroEventKind::Axis => {
                    if !is_bound(&point.binding) || !(-1.0..=1.0).contains(&point.value) {
                        return Err(PlanError::InvalidBinding);
                    }
                }
                MacroEventKind::MouseMove => {}
            }
        }
        Ok(())
    }
}

/// Gestures recognised by the RTS gesture layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RtsGesture {
    Tap,
    DoubleTap,
    LongPress,
    Drag,
    Pinch,
    TwoFingerDrag,
}

impl RtsGesture {
    pub const ALL: [RtsGesture; 6] = [
        RtsGesture::Tap,
        RtsGesture::DoubleTap,
        RtsGesture::LongPress,
        RtsGesture::Drag,
        RtsGesture::Pinch,
        RtsGesture::TwoFingerDrag,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RtsGesture::Tap => "TAP",
            RtsGesture::DoubleTap => "DOUBLE_TAP",
            RtsGesture::LongPress => "LONG_PRESS",
            RtsGesture::Drag => "DRAG",
            RtsGesture::Pinch => "PINCH",
            RtsGesture::TwoFingerDrag => "TWO_FINGER_DRAG",
        }
    }
}

/// An enable flag paired with the binding action string for one gesture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureBinding {
    pub enabled: bool,
    pub action: String,
}

impl Default for GestureBinding {
    fn default() -> Self {
        Self {
            enabled: false,
            action: "NONE".to_string(),
        }
    }
}

impl GestureBinding {
    pub fn new(enabled: bool, action: impl Into<String>) -> Self {
        Self {
            enabled,
            action: action.into(),
        }
    }
}

/// Configuration for RTS-style gestures.  Every gesture is independently
/// enabled and mapped to a binding string.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtsGestureConfig {
    #[serde(default)]
    pub tap: GestureBinding,
    #[serde(default)]
    pub double_tap: GestureBinding,
    #[serde(default)]
    pub long_press: GestureBinding,
    #[serde(default)]
    pub drag: GestureBinding,
    #[serde(default)]
    pub pinch: GestureBinding,
    #[serde(default)]
    pub two_finger_drag: GestureBinding,
}

impl Default for RtsGestureConfig {
    fn default() -> Self {
        Self {
            tap: GestureBinding::default(),
            double_tap: GestureBinding::default(),
            long_press: GestureBinding::default(),
            drag: GestureBinding::default(),
            pinch: GestureBinding::default(),
            two_finger_drag: GestureBinding::default(),
        }
    }
}

impl RtsGestureConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enables a gesture and assigns its action string.
    pub fn set(&mut self, gesture: RtsGesture, action: impl Into<String>) {
        *self.binding_mut(gesture) = GestureBinding::new(true, action);
    }

    pub fn binding(&self, gesture: RtsGesture) -> &GestureBinding {
        match gesture {
            RtsGesture::Tap => &self.tap,
            RtsGesture::DoubleTap => &self.double_tap,
            RtsGesture::LongPress => &self.long_press,
            RtsGesture::Drag => &self.drag,
            RtsGesture::Pinch => &self.pinch,
            RtsGesture::TwoFingerDrag => &self.two_finger_drag,
        }
    }

    pub fn binding_mut(&mut self, gesture: RtsGesture) -> &mut GestureBinding {
        match gesture {
            RtsGesture::Tap => &mut self.tap,
            RtsGesture::DoubleTap => &mut self.double_tap,
            RtsGesture::LongPress => &mut self.long_press,
            RtsGesture::Drag => &mut self.drag,
            RtsGesture::Pinch => &mut self.pinch,
            RtsGesture::TwoFingerDrag => &mut self.two_finger_drag,
        }
    }

    /// Resolved action for a gesture, or `Action::None` when disabled/unbound.
    pub fn action(&self, gesture: RtsGesture) -> Action {
        let binding = self.binding(gesture);
        if binding.enabled {
            map_binding(&binding.action)
        } else {
            Action::None
        }
    }

    /// All currently enabled gestures and their resolved actions.
    pub fn enabled_actions(&self) -> Vec<(RtsGesture, Action)> {
        RtsGesture::ALL
            .into_iter()
            .filter(|gesture| self.binding(*gesture).enabled)
            .map(|gesture| (gesture, self.action(gesture)))
            .collect()
    }

    /// Enabled gestures must point at a real, non-disabled binding.
    pub fn validate(&self) -> Result<(), PlanError> {
        for gesture in RtsGesture::ALL {
            let binding = self.binding(gesture);
            if binding.enabled && !is_bound(&binding.action) {
                return Err(PlanError::InvalidBinding);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shooting_plan_defaults_and_validation() {
        let plan = ShootingPlan::default();
        assert_eq!(plan.fire_button, "NONE");
        assert_eq!(plan.interval_ms, 100);
        assert_eq!(plan.validate(), Err(PlanError::InvalidBinding));

        let valid = ShootingPlan {
            fire_button: "MOUSE_LEFT".into(),
            toggle_button: "KEY_SPACE".into(),
            hotkey: "KEY_1".into(),
            interval_ms: 80,
            preheat_ms: 200,
            offset: 0.25,
        };
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(valid.fire_action(), Action::MouseButton(0));

        let mut bad = valid.clone();
        bad.interval_ms = 0;
        assert_eq!(bad.validate(), Err(PlanError::BadInterval));
        bad.interval_ms = 80;
        bad.preheat_ms = 20_000;
        assert_eq!(bad.validate(), Err(PlanError::BadPreheat));
        bad.preheat_ms = 0;
        bad.offset = 3.0;
        assert_eq!(bad.validate(), Err(PlanError::BadOffset));
        bad.offset = 0.0;
        bad.hotkey = "BOGUS".into();
        assert_eq!(bad.validate(), Err(PlanError::InvalidBinding));
    }

    #[test]
    fn shooting_plan_deserializes_partial_json() {
        let plan: ShootingPlan =
            serde_json::from_str(r#"{"fireButton":"GAMEPAD_BUTTON_A"}"#).unwrap();
        assert_eq!(plan.interval_ms, 100);
        assert_eq!(plan.preheat_ms, 0);
        assert_eq!(plan.validate(), Ok(()));
    }

    #[test]
    fn macro_records_and_replays() {
        let mut macro_plan = Macro::new("combo");
        assert_eq!(
            macro_plan.record(0, MacroEventKind::Press, "KEY_A", 1.0),
            Ok(())
        );
        assert_eq!(
            macro_plan.record(50, MacroEventKind::Release, "KEY_A", 0.0),
            Ok(())
        );
        assert_eq!(
            macro_plan.record(60, MacroEventKind::Axis, "GAMEPAD_LEFT_THUMB_RIGHT", 0.5),
            Ok(())
        );
        assert_eq!(
            macro_plan.record(70, MacroEventKind::MouseMove, "", 12.0),
            Ok(())
        );
        assert_eq!(
            macro_plan.record(10, MacroEventKind::Press, "KEY_B", 1.0),
            Err(PlanError::UnsortedTimestamps)
        );
        assert_eq!(macro_plan.duration_ms(), 70);
        assert_eq!(macro_plan.validate(), Ok(()));

        let events = macro_plan.to_events();
        assert_eq!(
            events,
            vec![
                InputEvent::Press("KEY_A".into()),
                InputEvent::Release("KEY_A".into()),
                InputEvent::Axis("GAMEPAD_LEFT_THUMB_RIGHT".into(), 0.5),
                InputEvent::MouseMove {
                    dx: 12.0,
                    dy: 0.0,
                    speed: 1.0
                },
            ]
        );

        let mut cursor = 0;
        assert_eq!(macro_plan.replay_at(40, &mut cursor).len(), 1);
        assert_eq!(macro_plan.replay_at(100, &mut cursor).len(), 3);
        assert!(macro_plan.replay_at(100, &mut cursor).is_empty());
    }

    #[test]
    fn macro_validation_rejects_bad_data() {
        assert_eq!(Macro::new("").validate(), Err(PlanError::EmptyName));
        assert_eq!(Macro::new("x").validate(), Err(PlanError::EmptyMacro));
        let mut macro_plan = Macro::new("x");
        macro_plan
            .record(0, MacroEventKind::Press, "NOPE", 1.0)
            .unwrap();
        assert_eq!(macro_plan.validate(), Err(PlanError::InvalidBinding));
        macro_plan.points.clear();
        macro_plan
            .record(0, MacroEventKind::Axis, "KEY_A", 9.0)
            .unwrap();
        assert_eq!(macro_plan.validate(), Err(PlanError::InvalidBinding));
        macro_plan.points[0].value = f32::NAN;
        assert_eq!(macro_plan.validate(), Err(PlanError::BadValue));
    }

    #[test]
    fn macro_json_roundtrip() {
        let mut macro_plan = Macro::new("double tap");
        macro_plan.loop_count = 2;
        macro_plan
            .record(0, MacroEventKind::Press, "MOUSE_LEFT", 1.0)
            .unwrap();
        macro_plan
            .record(30, MacroEventKind::Release, "MOUSE_LEFT", 0.0)
            .unwrap();
        let json = serde_json::to_string(&macro_plan).unwrap();
        let parsed: Macro = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, macro_plan);
        assert_eq!(parsed.validate(), Ok(()));
    }

    #[test]
    fn rts_gesture_config_defaults_and_actions() {
        let config = RtsGestureConfig::default();
        assert_eq!(config.validate(), Ok(()));
        assert!(config.enabled_actions().is_empty());
        for gesture in RtsGesture::ALL {
            assert!(!config.binding(gesture).enabled);
            assert_eq!(config.action(gesture), Action::None);
        }

        let mut config = RtsGestureConfig::new();
        config.set(RtsGesture::Tap, "MOUSE_LEFT");
        config.set(RtsGesture::DoubleTap, "MOUSE_RIGHT");
        config.set(RtsGesture::Pinch, "KEY_KPPLUS");
        assert_eq!(config.action(RtsGesture::Tap), Action::MouseButton(0));
        assert_eq!(config.action(RtsGesture::DoubleTap), Action::MouseButton(1));
        assert_eq!(config.enabled_actions().len(), 3);
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn rts_gesture_config_validation() {
        let mut config = RtsGestureConfig::new();
        config.set(RtsGesture::Drag, "NOT_A_KEY");
        assert_eq!(config.validate(), Err(PlanError::InvalidBinding));
        config.set(RtsGesture::Drag, "NONE");
        assert_eq!(config.validate(), Err(PlanError::InvalidBinding));
        config.set(RtsGesture::Drag, "GAMEPAD_BUTTON_A");
        assert_eq!(config.validate(), Ok(()));

        assert_eq!(RtsGesture::TwoFingerDrag.as_str(), "TWO_FINGER_DRAG");
        assert_eq!(RtsGesture::ALL.len(), 6);
    }

    #[test]
    fn rts_gesture_config_json_roundtrip() {
        let mut config = RtsGestureConfig::new();
        config.set(RtsGesture::LongPress, "KEY_SPACE");
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("longPress"));
        assert!(json.contains("twoFingerDrag"));
        let parsed: RtsGestureConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }
}
