use std::collections::{HashMap, HashSet};

use crate::profile::{Element, ElementType, Profile, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Start,
    Move,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Press(String),
    Release(String),
    Axis(String, f32),
    MouseMove { dx: f32, dy: f32, speed: f32 },
}

struct Active {
    element: usize,
    binding: Option<String>,
    axes: Vec<(String, f32)>,
    range_binding: Option<String>,
    /// Last normalized trackpad position, for relative delta computation.
    trackpad: Option<(f32, f32)>,
}

pub struct Engine {
    profile: Profile,
    active: HashMap<u64, Active>,
    toggles: HashSet<usize>,
}

impl Engine {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            active: HashMap::new(),
            toggles: HashSet::new(),
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn reset(&mut self) {
        self.active.clear();
        self.toggles.clear();
    }

    pub fn on_touch(&mut self, id: u64, x: f32, y: f32, phase: TouchPhase) -> Vec<InputEvent> {
        match phase {
            TouchPhase::Start => self.start(id, x, y),
            TouchPhase::Move => self.moved(id, x, y),
            TouchPhase::End => self.end(id),
        }
    }

    fn start(&mut self, id: u64, x: f32, y: f32) -> Vec<InputEvent> {
        let mut events = self.end(id);
        let Some(index) = element_at(&self.profile, x, y) else {
            return events;
        };
        let element = self.profile.elements[index].clone();
        let mut active = Active {
            element: index,
            binding: None,
            axes: Vec::new(),
            range_binding: None,
            trackpad: None,
        };

        match element.kind {
            ElementType::Button => {
                let binding = element.bindings.first().cloned().unwrap_or_default();
                if binding != "NONE" && !binding.is_empty() {
                    if element.toggle_switch {
                        if self.toggles.remove(&index) {
                            events.push(InputEvent::Release(binding));
                        } else {
                            self.toggles.insert(index);
                            events.push(InputEvent::Press(binding));
                        }
                    } else {
                        active.binding = Some(binding.clone());
                        events.push(InputEvent::Press(binding));
                    }
                }
            }
            ElementType::DPad => {
                if let Some(binding) = dpad_binding(&element, x, y) {
                    active.binding = Some(binding.clone());
                    events.push(InputEvent::Press(binding));
                }
            }
            ElementType::Stick => {
                active.axes = stick_axes(&element, x, y);
                events.extend(
                    active
                        .axes
                        .iter()
                        .map(|(binding, value)| InputEvent::Axis(binding.clone(), *value)),
                );
            }
            ElementType::RangeButton => {
                if let Some(binding) = primary_binding(&element) {
                    let value = range_value(&element, x);
                    active.range_binding = Some(binding.clone());
                    events.push(InputEvent::Axis(binding, value));
                }
            }
            ElementType::Trackpad => {
                active.trackpad = Some((x, y));
            }
        }

        self.active.insert(id, active);
        events
    }

    fn moved(&mut self, id: u64, x: f32, y: f32) -> Vec<InputEvent> {
        let Some(active) = self.active.get(&id) else {
            return Vec::new();
        };
        let element = self.profile.elements[active.element].clone();
        let old_binding = active.binding.clone();
        let old_axes = active.axes.clone();
        let mut events = Vec::new();
        let mut new_binding = None;
        let mut new_axes = Vec::new();
        let mut new_range = None;
        let mut new_trackpad = None;

        match element.kind {
            ElementType::DPad => {
                let binding = dpad_binding(&element, x, y);
                if binding != old_binding {
                    if let Some(old) = &old_binding {
                        events.push(InputEvent::Release(old.clone()));
                    }
                    if let Some(new) = &binding {
                        events.push(InputEvent::Press(new.clone()));
                    }
                }
                new_binding = binding;
            }
            ElementType::Stick => {
                for (binding, _) in &old_axes {
                    events.push(InputEvent::Axis(binding.clone(), 0.0));
                }
                new_axes = stick_axes(&element, x, y);
                events.extend(
                    new_axes
                        .iter()
                        .map(|(binding, value)| InputEvent::Axis(binding.clone(), *value)),
                );
            }
            ElementType::RangeButton => {
                if let Some(binding) = primary_binding(&element) {
                    let value = range_value(&element, x);
                    events.push(InputEvent::Axis(binding.clone(), value));
                    new_range = Some(binding);
                }
            }
            ElementType::Trackpad => {
                if let Some((px, py)) = active.trackpad {
                    events.push(InputEvent::MouseMove {
                        dx: x - px,
                        dy: y - py,
                        speed: self.profile.cursor_speed,
                    });
                }
                new_trackpad = Some((x, y));
            }
            ElementType::Button => {}
        }

        if let Some(active) = self.active.get_mut(&id) {
            if new_binding.is_some() || matches!(element.kind, ElementType::DPad) {
                active.binding = new_binding;
            }
            if matches!(element.kind, ElementType::Stick) {
                active.axes = new_axes;
            }
            if new_range.is_some() {
                active.range_binding = new_range;
            }
            if let Some(position) = new_trackpad {
                active.trackpad = Some(position);
            }
        }
        events
    }

    fn end(&mut self, id: u64) -> Vec<InputEvent> {
        let Some(active) = self.active.remove(&id) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        if let Some(binding) = active.binding {
            events.push(InputEvent::Release(binding));
        }
        for (binding, _) in active.axes {
            events.push(InputEvent::Axis(binding, 0.0));
        }
        if let Some(binding) = active.range_binding {
            events.push(InputEvent::Axis(binding, 0.0));
        }
        events
    }
}

pub fn element_at(profile: &Profile, x: f32, y: f32) -> Option<usize> {
    profile
        .elements
        .iter()
        .rposition(|element| hit(element, x, y))
}

fn hit(element: &Element, x: f32, y: f32) -> bool {
    let scale = element.scale.max(0.1);
    let radius = match element.kind {
        ElementType::Button | ElementType::RangeButton => 0.075 * scale,
        ElementType::DPad | ElementType::Stick => 0.10 * scale,
        ElementType::Trackpad => 0.16 * scale,
    };
    let dx = (x - element.x).abs();
    let dy = (y - element.y).abs();
    match element.shape {
        Shape::Circle => dx * dx + dy * dy <= radius * radius,
        Shape::Square | Shape::Rect => dx <= radius && dy <= radius,
    }
}

fn dpad_binding(element: &Element, x: f32, y: f32) -> Option<String> {
    if element.bindings.len() < 4 {
        return None;
    }
    let dx = x - element.x;
    let dy = y - element.y;
    let index = if dx.abs() > dy.abs() {
        if dx >= 0.0 {
            1
        } else {
            3
        }
    } else if dy >= 0.0 {
        2
    } else {
        0
    };
    let binding = &element.bindings[index];
    (binding != "NONE" && !binding.is_empty()).then(|| binding.clone())
}

fn stick_axes(element: &Element, x: f32, y: f32) -> Vec<(String, f32)> {
    if element.bindings.len() < 4 {
        return Vec::new();
    }
    let radius = 0.10 * element.scale.max(0.1);
    let dx = ((x - element.x) / radius).clamp(-1.0, 1.0);
    let dy = ((y - element.y) / radius).clamp(-1.0, 1.0);
    let values = [(-dy).max(0.0), dx.max(0.0), dy.max(0.0), (-dx).max(0.0)];
    element
        .bindings
        .iter()
        .take(4)
        .zip(values)
        .filter(|(binding, _)| binding.as_str() != "NONE" && !binding.is_empty())
        .map(|(binding, value)| (binding.clone(), value))
        .collect()
}

fn primary_binding(element: &Element) -> Option<String> {
    element
        .bindings
        .iter()
        .find(|binding| binding.as_str() != "NONE" && !binding.is_empty())
        .cloned()
}

fn range_value(element: &Element, x: f32) -> f32 {
    let radius = 0.075 * element.scale.max(0.1);
    ((x - (element.x - radius)) / (radius * 2.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{builtin_profiles, Element, ElementType, Profile, Shape};

    fn element(kind: ElementType, x: f32, y: f32, bindings: &[&str]) -> Element {
        Element {
            kind,
            shape: Shape::Circle,
            bindings: bindings
                .iter()
                .map(|binding| (*binding).to_string())
                .collect(),
            scale: 1.0,
            x,
            y,
            toggle_switch: false,
            text: String::new(),
            icon_id: 0,
            range: None,
        }
    }

    fn profile() -> Profile {
        Profile {
            id: 99,
            name: "Test".into(),
            cursor_speed: 1.0,
            game_id: String::new(),
            elements: vec![
                element(ElementType::Button, 0.80, 0.70, &["GAMEPAD_BUTTON_A"]),
                element(
                    ElementType::DPad,
                    0.20,
                    0.70,
                    &[
                        "GAMEPAD_DPAD_UP",
                        "GAMEPAD_DPAD_RIGHT",
                        "GAMEPAD_DPAD_DOWN",
                        "GAMEPAD_DPAD_LEFT",
                    ],
                ),
                element(
                    ElementType::Stick,
                    0.50,
                    0.50,
                    &[
                        "GAMEPAD_LEFT_THUMB_UP",
                        "GAMEPAD_LEFT_THUMB_RIGHT",
                        "GAMEPAD_LEFT_THUMB_DOWN",
                        "GAMEPAD_LEFT_THUMB_LEFT",
                    ],
                ),
                element(ElementType::RangeButton, 0.30, 0.10, &["KEY_1"]),
            ],
        }
    }

    fn approx(left: f32, right: f32) {
        assert!((left - right).abs() < 1e-4, "{left} != {right}");
    }

    #[test]
    fn button_presses_and_releases() {
        let mut engine = Engine::new(profile());
        assert_eq!(
            engine.on_touch(1, 0.80, 0.70, TouchPhase::Start),
            vec![InputEvent::Press("GAMEPAD_BUTTON_A".into())]
        );
        assert_eq!(
            engine.on_touch(1, 0.80, 0.70, TouchPhase::End),
            vec![InputEvent::Release("GAMEPAD_BUTTON_A".into())]
        );
    }

    #[test]
    fn dpad_changes_direction() {
        let mut engine = Engine::new(profile());
        assert_eq!(
            engine.on_touch(1, 0.23, 0.70, TouchPhase::Start),
            vec![InputEvent::Press("GAMEPAD_DPAD_RIGHT".into())]
        );
        assert_eq!(
            engine.on_touch(1, 0.20, 0.67, TouchPhase::Move),
            vec![
                InputEvent::Release("GAMEPAD_DPAD_RIGHT".into()),
                InputEvent::Press("GAMEPAD_DPAD_UP".into())
            ]
        );
        assert_eq!(
            engine.on_touch(1, 0.20, 0.67, TouchPhase::End),
            vec![InputEvent::Release("GAMEPAD_DPAD_UP".into())]
        );
    }

    #[test]
    fn stick_emits_and_zeros_axes() {
        let mut engine = Engine::new(profile());
        let start = engine.on_touch(1, 0.53, 0.47, TouchPhase::Start);
        assert_eq!(start.len(), 4);
        let value = |events: &[InputEvent], binding: &str| -> f32 {
            events
                .iter()
                .find_map(|event| match event {
                    InputEvent::Axis(name, value) if name == binding => Some(*value),
                    _ => None,
                })
                .unwrap_or(0.0)
        };
        approx(value(&start, "GAMEPAD_LEFT_THUMB_RIGHT"), 0.3);
        approx(value(&start, "GAMEPAD_LEFT_THUMB_UP"), 0.3);

        let end = engine.on_touch(1, 0.53, 0.47, TouchPhase::End);
        for binding in [
            "GAMEPAD_LEFT_THUMB_UP",
            "GAMEPAD_LEFT_THUMB_RIGHT",
            "GAMEPAD_LEFT_THUMB_DOWN",
            "GAMEPAD_LEFT_THUMB_LEFT",
        ] {
            approx(value(&end, binding), 0.0);
        }
    }

    #[test]
    fn range_reports_value() {
        let mut engine = Engine::new(profile());
        let events = engine.on_touch(1, 0.30, 0.10, TouchPhase::Start);
        match events.as_slice() {
            [InputEvent::Axis(binding, value)] => {
                assert_eq!(binding, "KEY_1");
                approx(*value, 0.5);
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn misses_do_not_emit() {
        let mut engine = Engine::new(profile());
        assert!(engine.on_touch(1, 0.99, 0.01, TouchPhase::Start).is_empty());
    }

    #[test]
    fn builtin_profiles_create_engines() {
        for profile in builtin_profiles() {
            let engine = Engine::new(profile);
            assert!(!engine.profile().elements.is_empty());
        }
    }
}
