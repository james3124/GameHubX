//! Bridge between the reference-style input engine and the session router.
//!
//! [`gamehubx_input::Engine`] turns normalized touch coordinates into abstract
//! [`gamehubx_input::InputEvent`]s by consulting a profile's element bindings.
//! [`InputBridge`] translates those abstract events into the concrete,
//! copyable [`Event`](crate::input::Event)s understood by the session host and
//! routes every one of them through an owned [`InputRouter`]. Routing through
//! the router (instead of handing events back untracked) is what bounds the
//! pressed-state lifetime: a key, button or axis that goes down is remembered
//! by the router and is released automatically on focus loss or stop, even if
//! the surface never delivers a matching up event.

use gamehubx_input::{map_binding, Action, Engine, InputEvent, Profile, TouchPhase};

use crate::input::{Event, InputRouter, PushOutcome};

/// Reserved pointer id used for pointer events that come from mouse-button
/// bindings and trackpad (relative mouse) movement.
///
/// Touch ids supplied to [`InputBridge::on_touch`] are owned by the caller. A
/// dedicated, reserved value keeps synthetic mouse pointer state from aliasing
/// those touches inside the router's fixed track table.
pub const MOUSE_POINTER_ID: u64 = u64::MAX;

/// Owns exactly one input [`Engine`] and the [`InputRouter`] that receives its
/// translated events.
///
/// The bridge is intentionally not `Clone`: the engine's active-touch table and
/// the router's pressed-state table must have a single owner per session.
pub struct InputBridge {
    engine: Engine,
    router: InputRouter,
}

impl InputBridge {
    /// Build a bridge around a fresh engine using `profile`.
    pub fn new(profile: Profile) -> Self {
        Self::from_engine(Engine::new(profile))
    }

    /// Build a bridge around an already-configured engine.
    pub fn from_engine(engine: Engine) -> Self {
        Self {
            engine,
            router: InputRouter::new(),
        }
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn router(&self) -> &InputRouter {
        &self.router
    }

    pub fn router_mut(&mut self) -> &mut InputRouter {
        &mut self.router
    }

    /// Feed a touch to the engine, translate every produced [`InputEvent`] and
    /// route the result through the owned router.
    ///
    /// Returns the events from this touch that the router accepted. Events the
    /// router refused (duplicate press, full track table, overflow or a stopped
    /// router) are omitted, so the caller never injects an event that the
    /// router does not consider active. Pending events from earlier touches
    /// remain available through [`InputBridge::drain`].
    pub fn on_touch(&mut self, id: u64, x: f32, y: f32, phase: TouchPhase) -> Vec<Event> {
        let input_events = self.engine.on_touch(id, x, y, phase);
        let mut routed = Vec::with_capacity(input_events.len());
        for input in &input_events {
            let Some(event) = map_input_event(input) else {
                continue;
            };
            if self.router.push(event) == PushOutcome::Enqueued {
                routed.push(event);
            }
        }
        routed
    }

    /// Drain up to `limit` pending router events in FIFO order. `limit` is
    /// capped by [`InputRouter::drain`] at `EVENT_CAPACITY`.
    pub fn drain(&mut self, limit: usize) -> Vec<Event> {
        self.router.drain(limit).collect()
    }

    /// Forget all active touches and all routed state. A fresh router is
    /// installed, so the bridge is immediately reusable.
    pub fn reset(&mut self) {
        self.engine.reset();
        self.router = InputRouter::new();
    }

    /// Release every tracked press and clear active touches on focus loss.
    ///
    /// Returns `false` if the router was already stopped.
    pub fn focus_lost(&mut self) -> bool {
        self.engine.reset();
        self.router.focus_lost()
    }

    /// Release every tracked press and permanently stop routing.
    ///
    /// Returns `false` on later, idempotent calls.
    pub fn stop(&mut self) -> bool {
        self.engine.reset();
        self.router.stop()
    }
}

/// Translate one neutral input event into the router event it represents.
fn map_input_event(input: &InputEvent) -> Option<Event> {
    match input {
        InputEvent::Press(binding) => press_event(map_binding(binding)),
        InputEvent::Release(binding) => release_event(map_binding(binding)),
        InputEvent::Axis(binding, value) => axis_event(map_binding(binding), *value, binding),
        InputEvent::MouseMove { dx, dy, .. } => Some(Event::PointerMove {
            id: MOUSE_POINTER_ID,
            x: *dx,
            y: *dy,
        }),
    }
}

fn press_event(action: Action) -> Option<Event> {
    match action {
        Action::KeyCode(code) => Some(Event::KeyDown { code }),
        Action::GamepadButton(button) => Some(Event::GamepadButton {
            button,
            pressed: true,
        }),
        Action::MouseButton(_) => Some(Event::PointerDown {
            id: MOUSE_POINTER_ID,
            x: 0.0,
            y: 0.0,
        }),
        Action::GamepadAxis(_, _) | Action::MouseMove | Action::None => None,
    }
}

fn release_event(action: Action) -> Option<Event> {
    match action {
        Action::KeyCode(code) => Some(Event::KeyUp { code }),
        Action::GamepadButton(button) => Some(Event::GamepadButton {
            button,
            pressed: false,
        }),
        Action::MouseButton(_) => Some(Event::PointerUp {
            id: MOUSE_POINTER_ID,
            x: 0.0,
            y: 0.0,
        }),
        Action::GamepadAxis(_, _) | Action::MouseMove | Action::None => None,
    }
}

fn axis_event(action: Action, value: f32, binding: &str) -> Option<Event> {
    let value = if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let (axis, sign) = match action {
        Action::GamepadAxis(axis, sign) => (axis, sign),
        // An analog element bound to a digital action (for example a range
        // button bound to a key) still carries a magnitude. Surface it on a
        // stable synthetic axis instead of silently dropping the value.
        Action::KeyCode(_)
        | Action::GamepadButton(_)
        | Action::MouseButton(_)
        | Action::MouseMove
        | Action::None => (synthetic_axis(binding), 1.0),
    };
    Some(Event::GamepadAxis {
        axis,
        value: value * sign,
    })
}

/// Stable synthetic axis index in `0..8` for bindings that do not name a
/// gamepad axis. FNV-1a keeps unrelated bindings from collapsing onto the same
/// axis while staying inside the router's `MAX_AXES` track table.
fn synthetic_axis(binding: &str) -> u16 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in binding.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % 8) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamehubx_input::{ButtonMode, Element, ElementType, Shape};

    fn element(kind: ElementType, x: f32, y: f32, bindings: &[&str]) -> Element {
        Element {
            kind,
            shape: Shape::Circle,
            mode: ButtonMode::Click,
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

    fn profile(elements: Vec<Element>) -> Profile {
        Profile {
            id: 1,
            name: "bridge-test".into(),
            cursor_speed: 1.0,
            game_id: String::new(),
            elements,
        }
    }

    fn bridge() -> InputBridge {
        InputBridge::new(profile(vec![
            element(ElementType::Button, 0.50, 0.50, &["KEY_A"]),
            element(ElementType::Button, 0.80, 0.70, &["GAMEPAD_BUTTON_A"]),
            element(ElementType::Button, 0.10, 0.90, &["MOUSE_LEFT"]),
            element(
                ElementType::RangeButton,
                0.30,
                0.10,
                &["GAMEPAD_AXIS_LEFT_X"],
            ),
            element(ElementType::Trackpad, 0.10, 0.10, &["MOUSE_MOVE"]),
        ]))
    }

    #[test]
    fn key_button_routes_key_down_and_up() {
        let mut bridge = bridge();
        assert_eq!(
            bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start),
            vec![Event::KeyDown { code: 38 }]
        );
        assert_eq!(
            bridge.on_touch(1, 0.50, 0.50, TouchPhase::End),
            vec![Event::KeyUp { code: 38 }]
        );
        // Routed events remain queued until the consumer drains them.
        let drained = bridge.drain(16);
        assert_eq!(
            drained,
            vec![Event::KeyDown { code: 38 }, Event::KeyUp { code: 38 }]
        );
        assert!(bridge.router().is_empty());
    }

    #[test]
    fn gamepad_button_and_mouse_button_map_to_their_event_kinds() {
        let mut bridge = bridge();
        assert_eq!(
            bridge.on_touch(2, 0.80, 0.70, TouchPhase::Start),
            vec![Event::GamepadButton {
                button: 0,
                pressed: true
            }]
        );
        assert_eq!(
            bridge.on_touch(3, 0.10, 0.90, TouchPhase::Start),
            vec![Event::PointerDown {
                id: MOUSE_POINTER_ID,
                x: 0.0,
                y: 0.0
            }]
        );
        assert_eq!(
            bridge.on_touch(2, 0.80, 0.70, TouchPhase::End),
            vec![Event::GamepadButton {
                button: 0,
                pressed: false
            }]
        );
    }

    #[test]
    fn axis_binding_maps_to_gamepad_axis() {
        let mut bridge = bridge();
        assert_eq!(
            bridge.on_touch(4, 0.30, 0.10, TouchPhase::Start),
            vec![Event::GamepadAxis {
                axis: 0,
                value: 0.5
            }]
        );
        assert_eq!(
            bridge.on_touch(4, 0.30, 0.10, TouchPhase::End),
            vec![Event::GamepadAxis {
                axis: 0,
                value: 0.0
            }]
        );
    }

    #[test]
    fn signed_axis_binding_applies_sign() {
        let mut bridge = InputBridge::new(profile(vec![element(
            ElementType::RangeButton,
            0.30,
            0.10,
            &["GAMEPAD_LEFT_THUMB_LEFT"],
        )]));
        assert_eq!(
            bridge.on_touch(4, 0.30, 0.10, TouchPhase::Start),
            vec![Event::GamepadAxis {
                axis: 0,
                value: -0.5
            }]
        );
    }

    #[test]
    fn trackpad_move_maps_to_pointer_move() {
        let mut bridge = bridge();
        bridge.on_touch(5, 0.10, 0.10, TouchPhase::Start);
        assert_eq!(
            bridge.on_touch(5, 0.40, 0.25, TouchPhase::Move),
            vec![Event::PointerMove {
                id: MOUSE_POINTER_ID,
                x: 0.3,
                y: 0.15
            }]
        );
    }

    #[test]
    fn events_are_also_pending_in_the_router_queue() {
        let mut bridge = bridge();
        bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start);
        bridge.on_touch(2, 0.80, 0.70, TouchPhase::Start);
        let drained = bridge.drain(64);
        assert!(drained.contains(&Event::KeyDown { code: 38 }));
        assert!(drained.contains(&Event::GamepadButton {
            button: 0,
            pressed: true
        }));
        assert!(bridge.drain(64).is_empty());
    }

    #[test]
    fn focus_lost_releases_router_pressed_state() {
        let mut bridge = bridge();
        bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start);
        bridge.on_touch(2, 0.80, 0.70, TouchPhase::Start);
        bridge.drain(64);
        assert_eq!(bridge.router().active_count(), 2);

        assert!(bridge.focus_lost());
        assert!(!bridge.router().is_stopped());
        let events = bridge.drain(64);
        assert!(events.contains(&Event::KeyUp { code: 38 }));
        assert!(events.contains(&Event::GamepadButton {
            button: 0,
            pressed: false
        }));
        assert_eq!(events.last(), Some(&Event::FocusLost));
        assert_eq!(bridge.router().active_count(), 0);
    }

    #[test]
    fn reset_restarts_engine_and_router() {
        let mut bridge = bridge();
        bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start);
        assert_eq!(bridge.router().len(), 1);

        bridge.reset();
        assert!(bridge.router().is_empty());
        assert_eq!(bridge.router().active_count(), 0);
        // The engine forgot the active touch, so a second Start re-emits Press.
        assert_eq!(
            bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start),
            vec![Event::KeyDown { code: 38 }]
        );
    }

    #[test]
    fn stop_is_terminal() {
        let mut bridge = bridge();
        bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start);
        assert!(bridge.stop());
        assert!(!bridge.stop());
        assert!(bridge.router().is_stopped());
        assert!(bridge.on_touch(2, 0.80, 0.70, TouchPhase::Start).is_empty());
    }

    #[test]
    fn unbound_press_produces_no_event() {
        let mut bridge = InputBridge::new(profile(vec![element(
            ElementType::Button,
            0.50,
            0.50,
            &["NONE"],
        )]));
        assert!(bridge.on_touch(1, 0.50, 0.50, TouchPhase::Start).is_empty());
    }
}
