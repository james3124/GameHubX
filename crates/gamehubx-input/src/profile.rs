use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    #[serde(default = "default_cursor_speed")]
    pub cursor_speed: f32,
    #[serde(default)]
    pub game_id: String,
    pub elements: Vec<Element>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementType {
    #[serde(rename = "D_PAD")]
    DPad,
    #[serde(rename = "BUTTON")]
    Button,
    #[serde(rename = "STICK")]
    Stick,
    #[serde(rename = "RANGE_BUTTON")]
    RangeButton,
    /// Relative trackpad: dragging emits mouse-move deltas scaled by
    /// `cursor_speed`, matching the reference's `TRACKPAD` control element.
    #[serde(rename = "TRACKPAD")]
    Trackpad,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    #[serde(rename = "CIRCLE")]
    Circle,
    #[serde(rename = "SQUARE")]
    Square,
    #[serde(rename = "RECT")]
    Rect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    #[serde(rename = "type")]
    pub kind: ElementType,
    pub shape: Shape,
    #[serde(default)]
    pub bindings: Vec<String>,
    #[serde(default = "default_scale")]
    pub scale: f32,
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub toggle_switch: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub icon_id: i32,
    #[serde(default)]
    pub range: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    InvalidJson,
    EmptyName,
    BadCursorSpeed,
    NoElements,
    BadCoordinate,
    BadScale,
    NoBindings,
    TooFewBindings,
    EmptyBinding,
    Io,
}

impl std::fmt::Display for InputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for InputError {}

impl From<std::io::Error> for InputError {
    fn from(_: std::io::Error) -> Self {
        InputError::Io
    }
}

impl Profile {
    pub fn parse(text: &str) -> Result<Self, InputError> {
        let profile: Profile = serde_json::from_str(text).map_err(|_| InputError::InvalidJson)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String, InputError> {
        serde_json::to_string_pretty(self).map_err(|_| InputError::InvalidJson)
    }

    pub fn validate(&self) -> Result<(), InputError> {
        if self.name.trim().is_empty() {
            return Err(InputError::EmptyName);
        }
        if !self.cursor_speed.is_finite() || self.cursor_speed <= 0.0 {
            return Err(InputError::BadCursorSpeed);
        }
        if self.elements.is_empty() {
            return Err(InputError::NoElements);
        }
        for element in &self.elements {
            if !element.x.is_finite()
                || !element.y.is_finite()
                || !(0.0..=1.0).contains(&element.x)
                || !(0.0..=1.0).contains(&element.y)
            {
                return Err(InputError::BadCoordinate);
            }
            if !element.scale.is_finite() || element.scale <= 0.0 {
                return Err(InputError::BadScale);
            }
            if element.bindings.is_empty() {
                return Err(InputError::NoBindings);
            }
            let minimum = match element.kind {
                ElementType::Button | ElementType::RangeButton => 1,
                ElementType::DPad | ElementType::Stick => 4,
                ElementType::Trackpad => 2,
            };
            if element.bindings.len() < minimum {
                return Err(InputError::TooFewBindings);
            }
            if element
                .bindings
                .iter()
                .any(|binding| binding.trim().is_empty())
            {
                return Err(InputError::EmptyBinding);
            }
        }
        Ok(())
    }

    pub fn load_dir(dir: &Path) -> Vec<Profile> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut profiles: Vec<Profile> = entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case("icp"))
                    .unwrap_or(false)
            })
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|text| Profile::parse(&text).ok())
            .collect();
        profiles.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.name.cmp(&b.name)));
        profiles
    }
}

pub fn builtin_profiles() -> Vec<Profile> {
    vec![gamepad_profile(), keyboard_mouse_profile()]
}

pub fn builtin_names() -> Vec<String> {
    builtin_profiles()
        .into_iter()
        .map(|profile| profile.name)
        .collect()
}

fn gamepad_profile() -> Profile {
    Profile {
        id: 1,
        name: "Gamepad".into(),
        cursor_speed: 1.0,
        game_id: String::new(),
        elements: vec![
            element(
                ElementType::DPad,
                0.20,
                0.72,
                1.0,
                &[
                    "GAMEPAD_DPAD_UP",
                    "GAMEPAD_DPAD_RIGHT",
                    "GAMEPAD_DPAD_DOWN",
                    "GAMEPAD_DPAD_LEFT",
                ],
            ),
            element(
                ElementType::Stick,
                0.10,
                0.35,
                1.0,
                &[
                    "GAMEPAD_LEFT_THUMB_UP",
                    "GAMEPAD_LEFT_THUMB_RIGHT",
                    "GAMEPAD_LEFT_THUMB_DOWN",
                    "GAMEPAD_LEFT_THUMB_LEFT",
                ],
            ),
            element(
                ElementType::Stick,
                0.70,
                0.72,
                1.0,
                &[
                    "GAMEPAD_RIGHT_THUMB_UP",
                    "GAMEPAD_RIGHT_THUMB_RIGHT",
                    "GAMEPAD_RIGHT_THUMB_DOWN",
                    "GAMEPAD_RIGHT_THUMB_LEFT",
                ],
            ),
            element(ElementType::Button, 0.85, 0.75, 1.0, &["GAMEPAD_BUTTON_A"]),
            element(ElementType::Button, 0.90, 0.68, 1.0, &["GAMEPAD_BUTTON_B"]),
            element(ElementType::Button, 0.80, 0.68, 1.0, &["GAMEPAD_BUTTON_X"]),
            element(ElementType::Button, 0.85, 0.61, 1.0, &["GAMEPAD_BUTTON_Y"]),
            element(ElementType::Button, 0.80, 0.12, 1.0, &["GAMEPAD_BUTTON_L1"]),
            element(ElementType::Button, 0.90, 0.12, 1.0, &["GAMEPAD_BUTTON_R1"]),
        ],
    }
}

fn keyboard_mouse_profile() -> Profile {
    Profile {
        id: 2,
        name: "Keyboard & Mouse".into(),
        cursor_speed: 1.0,
        game_id: String::new(),
        elements: vec![
            element(ElementType::Button, 0.10, 0.55, 1.0, &["KEY_A"]),
            element(ElementType::Button, 0.10, 0.45, 1.0, &["KEY_W"]),
            element(ElementType::Button, 0.16, 0.55, 1.0, &["KEY_D"]),
            element(ElementType::Button, 0.10, 0.65, 1.0, &["KEY_S"]),
            element(ElementType::Button, 0.20, 0.80, 1.0, &["KEY_SPACE"]),
            element(ElementType::Button, 0.85, 0.70, 1.0, &["MOUSE_LEFT"]),
            element(ElementType::Button, 0.90, 0.78, 1.0, &["MOUSE_RIGHT"]),
            element(ElementType::Button, 0.45, 0.10, 1.0, &["KEY_ESC"]),
        ],
    }
}

fn element(kind: ElementType, x: f32, y: f32, scale: f32, bindings: &[&str]) -> Element {
    Element {
        kind,
        shape: Shape::Circle,
        bindings: bindings
            .iter()
            .map(|binding| (*binding).to_string())
            .collect(),
        scale,
        x,
        y,
        toggle_switch: false,
        text: String::new(),
        icon_id: 0,
        range: None,
    }
}

fn default_cursor_speed() -> f32 {
    1.0
}

fn default_scale() -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "id": 7,
      "name": "Sample",
      "cnName": "ignored",
      "cursorSpeed": 1,
      "gameId": "",
      "elements": [
        {
          "type": "BUTTON",
          "shape": "CIRCLE",
          "bindings": ["GAMEPAD_BUTTON_A", "NONE", "NONE", "NONE"],
          "scale": 1,
          "x": 0.8,
          "y": 0.7,
          "toggleSwitch": false,
          "text": "",
          "iconId": 0
        },
        {
          "type": "D_PAD",
          "shape": "CIRCLE",
          "bindings": ["GAMEPAD_DPAD_UP", "GAMEPAD_DPAD_RIGHT", "GAMEPAD_DPAD_DOWN", "GAMEPAD_DPAD_LEFT"],
          "scale": 1,
          "x": 0.2,
          "y": 0.7,
          "toggleSwitch": false,
          "text": "",
          "iconId": 0
        }
      ]
    }"#;

    #[test]
    fn parses_reference_style_profile() {
        let profile = Profile::parse(SAMPLE).unwrap();
        assert_eq!(profile.id, 7);
        assert_eq!(profile.name, "Sample");
        assert_eq!(profile.elements.len(), 2);
        assert_eq!(profile.elements[0].kind, ElementType::Button);
        assert_eq!(profile.elements[0].bindings[0], "GAMEPAD_BUTTON_A");
        assert_eq!(profile.elements[1].kind, ElementType::DPad);
    }

    #[test]
    fn rejects_bad_profiles() {
        assert_eq!(Profile::parse("{}").unwrap_err(), InputError::InvalidJson);
        let mut profile = Profile::parse(SAMPLE).unwrap();
        profile.name.clear();
        assert_eq!(profile.validate(), Err(InputError::EmptyName));

        let mut profile = Profile::parse(SAMPLE).unwrap();
        profile.elements[0].x = 1.5;
        assert_eq!(profile.validate(), Err(InputError::BadCoordinate));

        let mut profile = Profile::parse(SAMPLE).unwrap();
        profile.elements[1].bindings.truncate(2);
        assert_eq!(profile.validate(), Err(InputError::TooFewBindings));
    }

    #[test]
    fn builtins_validate() {
        let profiles = builtin_profiles();
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            builtin_names(),
            vec!["Gamepad".to_string(), "Keyboard & Mouse".to_string()]
        );
        for profile in profiles {
            assert_eq!(profile.validate(), Ok(()));
        }
    }

    #[test]
    fn json_roundtrip() {
        let profile = Profile::parse(SAMPLE).unwrap();
        let text = profile.to_json().unwrap();
        assert_eq!(Profile::parse(&text).unwrap(), profile);
    }
}
