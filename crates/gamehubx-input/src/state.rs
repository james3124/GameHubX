//! A small virtual gamepad state used to accumulate mapped input.
//!
//! The state deliberately keeps three representations in sync: four analog
//! axes, a digital button bitmask and four explicit D-pad booleans.  The D-pad
//! booleans also occupy bits 13..16 of the mask via [`GamepadState::set_button`],
//! so consumers can work with whichever representation suits them.

use crate::action::{Action, BTN_DPAD_DOWN, BTN_DPAD_LEFT, BTN_DPAD_RIGHT, BTN_DPAD_UP};

/// Number of analog axes exposed by the virtual gamepad.
pub const AXIS_COUNT: usize = 4;
/// D-pad direction indices.
pub const DPAD_UP: usize = 0;
pub const DPAD_RIGHT: usize = 1;
pub const DPAD_DOWN: usize = 2;
pub const DPAD_LEFT: usize = 3;

/// Snapshot of a virtual gamepad: analog axes, digital buttons and D-pad.
#[derive(Clone, Debug, PartialEq)]
pub struct GamepadState {
    /// Left X, left Y, right X, right Y, each clamped to `-1.0..=1.0`.
    pub axes: [f32; AXIS_COUNT],
    /// Bitmask of pressed gamepad buttons.  Bit `n` corresponds to button
    /// index `n` from the gamepad table.
    pub buttons: u32,
    /// D-pad directions: up, right, down, left.
    pub dpad: [bool; 4],
}

impl Default for GamepadState {
    fn default() -> Self {
        Self {
            axes: [0.0; AXIS_COUNT],
            buttons: 0,
            dpad: [false; 4],
        }
    }
}

impl GamepadState {
    /// Creates a neutral gamepad state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets an analog axis, clamping the value to `-1.0..=1.0`.
    /// Out-of-range indices and non-finite values are ignored.
    pub fn set_axis(&mut self, index: usize, value: f32) {
        if index < AXIS_COUNT && value.is_finite() {
            self.axes[index] = value.clamp(-1.0, 1.0);
        }
    }

    /// Reads an axis, returning `0.0` for an out-of-range index.
    pub fn axis(&self, index: usize) -> f32 {
        self.axes.get(index).copied().unwrap_or(0.0)
    }

    /// Presses or releases a gamepad button by index.  Indices >= 32 are
    /// ignored because the mask is 32 bits wide.
    pub fn set_button(&mut self, code: u16, pressed: bool) {
        if (code as usize) < u32::BITS as usize {
            let mask = 1u32 << code;
            if pressed {
                self.buttons |= mask;
            } else {
                self.buttons &= !mask;
            }
        }
    }

    /// Returns whether a gamepad button is currently pressed.
    pub fn button(&self, code: u16) -> bool {
        (code as usize) < u32::BITS as usize && (self.buttons & (1u32 << code)) != 0
    }

    /// Sets or clears one D-pad direction (see the `DPAD_*` constants).
    pub fn set_dpad(&mut self, direction: usize, pressed: bool) {
        if direction < self.dpad.len() {
            self.dpad[direction] = pressed;
        }
    }

    /// Returns one D-pad direction, or `false` for an invalid index.
    pub fn dpad(&self, direction: usize) -> bool {
        self.dpad.get(direction).copied().unwrap_or(false)
    }

    /// Releases everything: axes to zero, empty button mask, D-pad cleared.
    pub fn release_all(&mut self) {
        self.axes = [0.0; AXIS_COUNT];
        self.buttons = 0;
        self.dpad = [false; 4];
    }

    /// Applies a resolved [`Action`] to the state.
    ///
    /// `value` is the analog magnitude (`0.0..=1.0`) reported by an element.
    /// For axis actions the sign from the binding is applied and *added* to the
    /// current axis value, so opposing directions naturally cancel; callers
    /// should reset the frame (e.g. via [`GamepadState::release_all`] or by
    /// setting axes explicitly) before accumulating a new frame.  D-pad button
    /// actions update both the boolean and the matching bitmask entry.
    pub fn apply(&mut self, action: Action, value: f32) {
        if !value.is_finite() {
            return;
        }
        match action {
            Action::GamepadButton(code) => {
                let pressed = value > 0.0;
                self.set_button(code, pressed);
                match code {
                    BTN_DPAD_UP => self.set_dpad(DPAD_UP, pressed),
                    BTN_DPAD_RIGHT => self.set_dpad(DPAD_RIGHT, pressed),
                    BTN_DPAD_DOWN => self.set_dpad(DPAD_DOWN, pressed),
                    BTN_DPAD_LEFT => self.set_dpad(DPAD_LEFT, pressed),
                    _ => {}
                }
            }
            Action::GamepadAxis(axis, sign) => {
                let index = axis as usize;
                if index < AXIS_COUNT {
                    let combined = self.axes[index] + sign * value;
                    self.set_axis(index, combined);
                }
            }
            // Keyboard/mouse actions have no representation on the virtual
            // gamepad and are intentionally ignored here.
            _ => {}
        }
    }

    /// Applies a binding string directly (convenience wrapper around
    /// [`GamepadState::apply`]).
    pub fn apply_binding(&mut self, binding: &str, value: f32) {
        self.apply(crate::action::map_binding(binding), value);
    }

    /// Returns `true` when no button, axis or D-pad direction is active.
    pub fn is_neutral(&self) -> bool {
        self.buttons == 0
            && self.dpad.iter().all(|pressed| !pressed)
            && self.axes.iter().all(|value| value.abs() < f32::EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{AXIS_LEFT_X, AXIS_LEFT_Y, AXIS_RIGHT_X, BTN_A, BTN_B, BTN_L1};

    #[test]
    fn new_state_is_neutral() {
        let state = GamepadState::new();
        assert_eq!(state.axes, [0.0; AXIS_COUNT]);
        assert_eq!(state.buttons, 0);
        assert_eq!(state.dpad, [false; 4]);
        assert!(state.is_neutral());
        assert_eq!(state, GamepadState::default());
    }

    #[test]
    fn set_axis_clamps_and_ignores_bad_input() {
        let mut state = GamepadState::new();
        state.set_axis(AXIS_LEFT_X as usize, 0.5);
        assert_eq!(state.axis(AXIS_LEFT_X as usize), 0.5);
        state.set_axis(AXIS_LEFT_X as usize, 4.2);
        assert_eq!(state.axis(AXIS_LEFT_X as usize), 1.0);
        state.set_axis(AXIS_LEFT_Y as usize, -4.2);
        assert_eq!(state.axis(AXIS_LEFT_Y as usize), -1.0);
        state.set_axis(99, 0.7);
        state.set_axis(0, f32::NAN);
        assert_eq!(state.axis(99), 0.0);
        assert_eq!(state.axis(AXIS_LEFT_X as usize), 1.0);
    }

    #[test]
    fn set_button_updates_bitmask() {
        let mut state = GamepadState::new();
        state.set_button(BTN_A, true);
        state.set_button(BTN_L1, true);
        assert!(state.button(BTN_A));
        assert!(state.button(BTN_L1));
        assert!(!state.button(BTN_B));
        assert_eq!(state.buttons, (1u32 << BTN_A) | (1u32 << BTN_L1));
        state.set_button(BTN_A, false);
        assert!(!state.button(BTN_A));
        state.set_button(32, true);
        state.set_button(10_000, true);
        assert!(!state.is_neutral());
    }

    #[test]
    fn dpad_booleans() {
        let mut state = GamepadState::new();
        state.set_dpad(DPAD_UP, true);
        assert!(state.dpad(DPAD_UP));
        assert!(!state.dpad(DPAD_DOWN));
        state.set_dpad(99, true);
        assert!(!state.dpad(99));
    }

    #[test]
    fn apply_actions() {
        let mut state = GamepadState::new();
        state.apply(Action::GamepadButton(BTN_A), 1.0);
        assert!(state.button(BTN_A));

        state.apply(crate::action::map_binding("GAMEPAD_DPAD_LEFT"), 1.0);
        assert!(state.dpad(DPAD_LEFT));
        assert!(state.button(crate::action::BTN_DPAD_LEFT));

        state.apply(crate::action::map_binding("GAMEPAD_LEFT_THUMB_RIGHT"), 0.4);
        state.apply(crate::action::map_binding("GAMEPAD_LEFT_THUMB_LEFT"), 0.15);
        let expected = 0.25;
        assert!((state.axis(AXIS_LEFT_X as usize) - expected).abs() < 1e-6);

        state.apply(Action::KeyCode(38), 1.0);
        state.apply(Action::MouseButton(0), 1.0);
        assert_eq!(state.axes[AXIS_RIGHT_X as usize], 0.0);
    }

    #[test]
    fn apply_binding_and_release_all() {
        let mut state = GamepadState::new();
        state.apply_binding("GAMEPAD_BUTTON_B", 1.0);
        state.apply_binding("GAMEPAD_RIGHT_THUMB_UP", 0.8);
        assert!(!state.is_neutral());
        state.release_all();
        assert!(state.is_neutral());
        assert_eq!(state.axes, [0.0; AXIS_COUNT]);
        assert_eq!(state.buttons, 0);
        assert_eq!(state.dpad, [false; 4]);
    }
}
