//! Neutral mapping of profile binding strings to abstract actions.
//!
//! A *binding* is the string stored in a profile element (for example
//! `GAMEPAD_BUTTON_A`, `KEY_SPACE` or `MOUSE_LEFT`).  This module turns those
//! strings into a platform-independent [`Action`] using a small, built-in
//! X11-style keycode table plus gamepad button/axis tables.  Nothing here
//! depends on Android, SDL or any other input backend: consumers of this crate
//! are free to translate the resulting keycodes/button indices to their own
//! injection layer.

/// A neutral, backend-independent input action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// A keyboard key, identified by an X11-style keycode.
    KeyCode(u32),
    /// A digital gamepad button, identified by a stable index (see
    /// [`GAMEPAD_BUTTONS`]).  The index also doubles as a bit position in
    /// [`crate::state::GamepadState::buttons`].
    GamepadButton(u16),
    /// An analog gamepad axis and the sign to apply to the incoming magnitude.
    /// Axis indices are `0..4`: left X, left Y, right X, right Y.
    GamepadAxis(u16, f32),
    /// A mouse button.  `0` = left, `1` = right, `2` = middle.
    MouseButton(u8),
    /// A relative mouse movement (see `InputEvent::MouseMove`).
    MouseMove,
    /// Unbound, unknown or explicitly disabled binding.
    None,
}

impl Action {
    /// Returns `true` for an unbound/unknown action.
    pub fn is_none(self) -> bool {
        matches!(self, Action::None)
    }

    /// A short, stable name for the action kind (handy for diagnostics).
    pub fn kind_str(self) -> &'static str {
        match self {
            Action::KeyCode(_) => "key",
            Action::GamepadButton(_) => "gamepad_button",
            Action::GamepadAxis(_, _) => "gamepad_axis",
            Action::MouseButton(_) => "mouse_button",
            Action::MouseMove => "mouse_move",
            Action::None => "none",
        }
    }
}

/// Gamepad button indices used by [`Action::GamepadButton`] and by the
/// bitmask in [`crate::state::GamepadState`].
pub const BTN_A: u16 = 0;
pub const BTN_B: u16 = 1;
pub const BTN_X: u16 = 2;
pub const BTN_Y: u16 = 3;
pub const BTN_L1: u16 = 4;
pub const BTN_R1: u16 = 5;
pub const BTN_L2: u16 = 6;
pub const BTN_R2: u16 = 7;
pub const BTN_BACK: u16 = 8;
pub const BTN_START: u16 = 9;
pub const BTN_LEFT_THUMB: u16 = 10;
pub const BTN_RIGHT_THUMB: u16 = 11;
pub const BTN_GUIDE: u16 = 12;
pub const BTN_DPAD_UP: u16 = 13;
pub const BTN_DPAD_RIGHT: u16 = 14;
pub const BTN_DPAD_DOWN: u16 = 15;
pub const BTN_DPAD_LEFT: u16 = 16;

/// Gamepad axis indices: left X, left Y, right X, right Y.
pub const AXIS_LEFT_X: u16 = 0;
pub const AXIS_LEFT_Y: u16 = 1;
pub const AXIS_RIGHT_X: u16 = 2;
pub const AXIS_RIGHT_Y: u16 = 3;

/// Gamepad button names and their stable index.  Names are matched
/// case-insensitively after trimming.
pub const GAMEPAD_BUTTONS: &[(&str, u16)] = &[
    ("GAMEPAD_BUTTON_A", BTN_A),
    ("GAMEPAD_A", BTN_A),
    ("GAMEPAD_BUTTON_B", BTN_B),
    ("GAMEPAD_B", BTN_B),
    ("GAMEPAD_BUTTON_X", BTN_X),
    ("GAMEPAD_X", BTN_X),
    ("GAMEPAD_BUTTON_Y", BTN_Y),
    ("GAMEPAD_Y", BTN_Y),
    ("GAMEPAD_BUTTON_L1", BTN_L1),
    ("GAMEPAD_L1", BTN_L1),
    ("GAMEPAD_BUTTON_R1", BTN_R1),
    ("GAMEPAD_R1", BTN_R1),
    ("GAMEPAD_BUTTON_L2", BTN_L2),
    ("GAMEPAD_L2", BTN_L2),
    ("GAMEPAD_BUTTON_R2", BTN_R2),
    ("GAMEPAD_R2", BTN_R2),
    ("GAMEPAD_BUTTON_BACK", BTN_BACK),
    ("GAMEPAD_BUTTON_SELECT", BTN_BACK),
    ("GAMEPAD_BACK", BTN_BACK),
    ("GAMEPAD_BUTTON_START", BTN_START),
    ("GAMEPAD_START", BTN_START),
    ("GAMEPAD_BUTTON_LEFT_THUMB", BTN_LEFT_THUMB),
    ("GAMEPAD_BUTTON_L3", BTN_LEFT_THUMB),
    ("GAMEPAD_L3", BTN_LEFT_THUMB),
    ("GAMEPAD_BUTTON_RIGHT_THUMB", BTN_RIGHT_THUMB),
    ("GAMEPAD_BUTTON_R3", BTN_RIGHT_THUMB),
    ("GAMEPAD_R3", BTN_RIGHT_THUMB),
    ("GAMEPAD_BUTTON_GUIDE", BTN_GUIDE),
    ("GAMEPAD_BUTTON_HOME", BTN_GUIDE),
    ("GAMEPAD_HOME", BTN_GUIDE),
    ("GAMEPAD_DPAD_UP", BTN_DPAD_UP),
    ("GAMEPAD_DPAD_RIGHT", BTN_DPAD_RIGHT),
    ("GAMEPAD_DPAD_DOWN", BTN_DPAD_DOWN),
    ("GAMEPAD_DPAD_LEFT", BTN_DPAD_LEFT),
];

/// Gamepad axis names, their axis index and the sign applied to a magnitude.
///
/// Vertical axes use `+1.0` for *up*, matching the friendly thumbstick names
/// below; consumers can flip the sign if their injection layer uses screen
/// coordinates.
pub const GAMEPAD_AXES: &[(&str, u16, f32)] = &[
    ("GAMEPAD_LEFT_THUMB_RIGHT", AXIS_LEFT_X, 1.0),
    ("GAMEPAD_LEFT_THUMB_LEFT", AXIS_LEFT_X, -1.0),
    ("GAMEPAD_LEFT_THUMB_UP", AXIS_LEFT_Y, 1.0),
    ("GAMEPAD_LEFT_THUMB_DOWN", AXIS_LEFT_Y, -1.0),
    ("GAMEPAD_RIGHT_THUMB_RIGHT", AXIS_RIGHT_X, 1.0),
    ("GAMEPAD_RIGHT_THUMB_LEFT", AXIS_RIGHT_X, -1.0),
    ("GAMEPAD_RIGHT_THUMB_UP", AXIS_RIGHT_Y, 1.0),
    ("GAMEPAD_RIGHT_THUMB_DOWN", AXIS_RIGHT_Y, -1.0),
    ("GAMEPAD_AXIS_LEFT_X", AXIS_LEFT_X, 1.0),
    ("GAMEPAD_AXIS_LEFT_Y", AXIS_LEFT_Y, 1.0),
    ("GAMEPAD_AXIS_RIGHT_X", AXIS_RIGHT_X, 1.0),
    ("GAMEPAD_AXIS_RIGHT_Y", AXIS_RIGHT_Y, 1.0),
];

/// Built-in X11-style keycode table.
///
/// Keycodes follow the Xorg `evdev` convention (Linux input keycode + 8), so
/// `KEY_A` is 38, `KEY_SPACE` is 65 and `KEY_ESC` is 9.  The table contains
/// roughly a hundred entries covering letters, digits, function keys,
/// modifiers, navigation, the keypad and common media keys, plus a few
/// convenient aliases.
pub const X11_KEYCODES: &[(&str, u32)] = &[
    // Function keys
    ("KEY_ESC", 9),
    ("KEY_F1", 67),
    ("KEY_F2", 68),
    ("KEY_F3", 69),
    ("KEY_F4", 70),
    ("KEY_F5", 71),
    ("KEY_F6", 72),
    ("KEY_F7", 73),
    ("KEY_F8", 74),
    ("KEY_F9", 75),
    ("KEY_F10", 76),
    ("KEY_F11", 95),
    ("KEY_F12", 96),
    ("KEY_F13", 191),
    ("KEY_F14", 192),
    ("KEY_F15", 193),
    ("KEY_F16", 194),
    ("KEY_F17", 195),
    ("KEY_F18", 196),
    ("KEY_F19", 197),
    ("KEY_F20", 198),
    ("KEY_F21", 199),
    ("KEY_F22", 200),
    ("KEY_F23", 201),
    ("KEY_F24", 202),
    // Digits and symbols
    ("KEY_0", 19),
    ("KEY_1", 10),
    ("KEY_2", 11),
    ("KEY_3", 12),
    ("KEY_4", 13),
    ("KEY_5", 14),
    ("KEY_6", 15),
    ("KEY_7", 16),
    ("KEY_8", 17),
    ("KEY_9", 18),
    ("KEY_MINUS", 20),
    ("KEY_EQUAL", 21),
    ("KEY_GRAVE", 49),
    // Letters
    ("KEY_A", 38),
    ("KEY_B", 56),
    ("KEY_C", 54),
    ("KEY_D", 40),
    ("KEY_E", 26),
    ("KEY_F", 41),
    ("KEY_G", 42),
    ("KEY_H", 43),
    ("KEY_I", 31),
    ("KEY_J", 44),
    ("KEY_K", 45),
    ("KEY_L", 46),
    ("KEY_M", 58),
    ("KEY_N", 57),
    ("KEY_O", 32),
    ("KEY_P", 33),
    ("KEY_Q", 24),
    ("KEY_R", 27),
    ("KEY_S", 39),
    ("KEY_T", 28),
    ("KEY_U", 30),
    ("KEY_V", 55),
    ("KEY_W", 25),
    ("KEY_X", 53),
    ("KEY_Y", 29),
    ("KEY_Z", 52),
    // Punctuation
    ("KEY_LEFTBRACE", 34),
    ("KEY_RIGHTBRACE", 35),
    ("KEY_SEMICOLON", 47),
    ("KEY_APOSTROPHE", 48),
    ("KEY_COMMA", 59),
    ("KEY_DOT", 60),
    ("KEY_SLASH", 61),
    ("KEY_BACKSLASH", 51),
    ("KEY_102ND", 92),
    // Editing and whitespace
    ("KEY_TAB", 23),
    ("KEY_ENTER", 36),
    ("KEY_RETURN", 36),
    ("KEY_BACKSPACE", 22),
    ("KEY_SPACE", 65),
    // Modifiers and locks
    ("KEY_LEFTCTRL", 37),
    ("KEY_LCTRL", 37),
    ("KEY_RIGHTCTRL", 105),
    ("KEY_RCTRL", 105),
    ("KEY_LEFTSHIFT", 50),
    ("KEY_LSHIFT", 50),
    ("KEY_RIGHTSHIFT", 62),
    ("KEY_RSHIFT", 62),
    ("KEY_LEFTALT", 64),
    ("KEY_LALT", 64),
    ("KEY_RIGHTALT", 108),
    ("KEY_RALT", 108),
    ("KEY_CAPSLOCK", 66),
    ("KEY_LEFTMETA", 133),
    ("KEY_LWIN", 133),
    ("KEY_RIGHTMETA", 134),
    ("KEY_RWIN", 134),
    ("KEY_MENU", 135),
    ("KEY_NUMLOCK", 77),
    ("KEY_SCROLLLOCK", 78),
    ("KEY_PAUSE", 127),
    ("KEY_CONTROL", 37),
    ("KEY_SHIFT", 50),
    ("KEY_ALT", 64),
    // Navigation
    ("KEY_INSERT", 118),
    ("KEY_HOME", 110),
    ("KEY_PAGEUP", 112),
    ("KEY_DELETE", 119),
    ("KEY_END", 115),
    ("KEY_PAGEDOWN", 117),
    ("KEY_UP", 111),
    ("KEY_DOWN", 116),
    ("KEY_LEFT", 113),
    ("KEY_RIGHT", 114),
    // Keypad
    ("KEY_KP0", 90),
    ("KEY_KP1", 87),
    ("KEY_KP2", 88),
    ("KEY_KP3", 89),
    ("KEY_KP4", 83),
    ("KEY_KP5", 84),
    ("KEY_KP6", 85),
    ("KEY_KP7", 79),
    ("KEY_KP8", 80),
    ("KEY_KP9", 81),
    ("KEY_KPDOT", 91),
    ("KEY_KPPLUS", 86),
    ("KEY_KPMINUS", 82),
    ("KEY_KPASTERISK", 63),
    ("KEY_KPSLASH", 106),
    ("KEY_KPENTER", 104),
    // Media and system
    ("KEY_MUTE", 121),
    ("KEY_VOLUMEDOWN", 122),
    ("KEY_VOLUMEUP", 123),
    ("KEY_POWER", 124),
    ("KEY_SLEEP", 150),
    ("KEY_NEXTSONG", 171),
    ("KEY_PREVIOUSSONG", 173),
    ("KEY_PLAYPAUSE", 172),
    ("KEY_STOPCD", 174),
    ("KEY_EJECTCD", 175),
];

/// Look up an X11-style keycode by binding name (case-insensitive).
pub fn x11_keycode(name: &str) -> Option<u32> {
    let key = name.trim().to_ascii_uppercase();
    X11_KEYCODES
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, code)| *code)
}

/// Look up a gamepad button index by binding name (case-insensitive).
pub fn gamepad_button(name: &str) -> Option<u16> {
    let key = name.trim().to_ascii_uppercase();
    GAMEPAD_BUTTONS
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, code)| *code)
}

/// Look up a gamepad axis/sign pair by binding name (case-insensitive).
pub fn gamepad_axis(name: &str) -> Option<(u16, f32)> {
    let key = name.trim().to_ascii_uppercase();
    GAMEPAD_AXES
        .iter()
        .find(|(candidate, _, _)| *candidate == key)
        .map(|(_, axis, sign)| (*axis, *sign))
}

/// Resolve a binding string to a neutral [`Action`].
///
/// Unknown, empty and `NONE`/`NULL` bindings resolve to [`Action::None`].
/// Matching is case-insensitive and ignores surrounding whitespace.
pub fn map_binding(binding: &str) -> Action {
    let key = binding.trim().to_ascii_uppercase();
    if key.is_empty() || key == "NONE" || key == "NULL" || key == "UNKNOWN" {
        return Action::None;
    }

    if let Some(button) = gamepad_button(&key) {
        return Action::GamepadButton(button);
    }
    if let Some((axis, sign)) = gamepad_axis(&key) {
        return Action::GamepadAxis(axis, sign);
    }
    if let Some(code) = x11_keycode(&key) {
        return Action::KeyCode(code);
    }

    match key.as_str() {
        "MOUSE_LEFT" | "MOUSE_BUTTON_LEFT" | "MOUSE_BUTTON_1" => Action::MouseButton(0),
        "MOUSE_RIGHT" | "MOUSE_BUTTON_RIGHT" | "MOUSE_BUTTON_2" => Action::MouseButton(1),
        "MOUSE_MIDDLE" | "MOUSE_BUTTON_MIDDLE" | "MOUSE_BUTTON_3" => Action::MouseButton(2),
        "MOUSE_MOVE" | "MOUSE_MOVE_RELATIVE" | "MOUSE" => Action::MouseMove,
        _ => Action::None,
    }
}

/// Returns `true` when `binding` is empty/disabled or maps to a known action.
/// Used by plan validation to reject typos in binding strings.
pub fn is_valid_binding(binding: &str) -> bool {
    let trimmed = binding.trim();
    trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("NONE")
        || trimmed.eq_ignore_ascii_case("NULL")
        || !map_binding(trimmed).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gamepad_buttons() {
        assert_eq!(
            map_binding("GAMEPAD_BUTTON_A"),
            Action::GamepadButton(BTN_A)
        );
        assert_eq!(
            map_binding("GAMEPAD_DPAD_UP"),
            Action::GamepadButton(BTN_DPAD_UP)
        );
        assert_eq!(
            map_binding("GAMEPAD_BUTTON_R1"),
            Action::GamepadButton(BTN_R1)
        );
        assert_eq!(map_binding("gamepad_start"), Action::GamepadButton(9));
    }

    #[test]
    fn maps_gamepad_axes_with_signs() {
        assert_eq!(
            map_binding("GAMEPAD_LEFT_THUMB_RIGHT"),
            Action::GamepadAxis(AXIS_LEFT_X, 1.0)
        );
        assert_eq!(
            map_binding("GAMEPAD_LEFT_THUMB_LEFT"),
            Action::GamepadAxis(AXIS_LEFT_X, -1.0)
        );
        assert_eq!(
            map_binding("GAMEPAD_RIGHT_THUMB_UP"),
            Action::GamepadAxis(AXIS_RIGHT_Y, 1.0)
        );
        assert_eq!(
            map_binding("GAMEPAD_RIGHT_THUMB_DOWN"),
            Action::GamepadAxis(AXIS_RIGHT_Y, -1.0)
        );
    }

    #[test]
    fn maps_x11_keys() {
        assert_eq!(map_binding("KEY_A"), Action::KeyCode(38));
        assert_eq!(map_binding("KEY_SPACE"), Action::KeyCode(65));
        assert_eq!(map_binding("KEY_ESC"), Action::KeyCode(9));
        assert_eq!(map_binding("key_control"), Action::KeyCode(37));
        assert_eq!(map_binding("KEY_KPENTER"), Action::KeyCode(104));
    }

    #[test]
    fn maps_mouse_and_none() {
        assert_eq!(map_binding("MOUSE_LEFT"), Action::MouseButton(0));
        assert_eq!(map_binding("MOUSE_RIGHT"), Action::MouseButton(1));
        assert_eq!(map_binding("MOUSE_MIDDLE"), Action::MouseButton(2));
        assert_eq!(map_binding("MOUSE_MOVE"), Action::MouseMove);
        assert_eq!(map_binding("NONE"), Action::None);
        assert_eq!(map_binding(""), Action::None);
        assert_eq!(map_binding("  null  "), Action::None);
        assert_eq!(map_binding("NOT_A_REAL_KEY"), Action::None);
    }

    #[test]
    fn x11_table_is_comprehensive_and_unique() {
        assert!(X11_KEYCODES.len() >= 100, "table has too few entries");
        let mut seen = std::collections::HashSet::new();
        for (name, code) in X11_KEYCODES {
            assert!(seen.insert(*name), "duplicate name in table: {name}");
            assert!((9..=255).contains(code), "implausible keycode for {name}");
        }
    }

    #[test]
    fn binding_validation() {
        assert!(is_valid_binding("KEY_A"));
        assert!(is_valid_binding("GAMEPAD_BUTTON_A"));
        assert!(is_valid_binding(""));
        assert!(is_valid_binding("NONE"));
        assert!(!is_valid_binding("KEY_NOPE"));
    }

    #[test]
    fn action_helpers() {
        assert!(Action::None.is_none());
        assert!(!Action::KeyCode(1).is_none());
        assert_eq!(Action::GamepadAxis(0, 1.0).kind_str(), "gamepad_axis");
    }
}
