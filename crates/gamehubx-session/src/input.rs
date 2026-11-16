//! R5 owned input routing.
//!
//! The future Rust session host receives touch/key/gamepad events from a thin
//! Android surface and drains them from a single owner. This module keeps that
//! owner bounded and leak-safe:
//!
//! - one fixed-size event ring (`EVENT_CAPACITY`), never grows;
//! - a fixed track table for pressed touches/keys/buttons/axes;
//! - `focus_lost`/`stop` release every tracked press and clear the tracked state;
//! - no background threads, globals, `Clone` or heap-backed event storage.
//!
//! Input mapping via `gamehubx_input::Engine` is intentionally not wired in
//! here yet; this router only owns transport and pressed-state lifetime.

/// Fixed number of pending events. Pushes above this are dropped.
pub const EVENT_CAPACITY: usize = 64;
/// Maximum simultaneously tracked touches.
pub const MAX_TOUCHES: usize = 8;
/// Maximum simultaneously tracked key codes.
pub const MAX_KEYS: usize = 16;
/// Maximum simultaneously tracked gamepad buttons.
pub const MAX_BUTTONS: usize = 24;
/// Maximum simultaneously tracked gamepad axes.
pub const MAX_AXES: usize = 8;

// Every tracked press must fit in the queue together with its marker event.
const _: () = assert!(MAX_TOUCHES + MAX_KEYS + MAX_BUTTONS + MAX_AXES + 1 <= EVENT_CAPACITY);

/// One routed input event. Events are small, copyable values and never own
/// heap memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    PointerDown { id: u64, x: f32, y: f32 },
    PointerMove { id: u64, x: f32, y: f32 },
    PointerUp { id: u64, x: f32, y: f32 },
    KeyDown { code: u32 },
    KeyUp { code: u32 },
    GamepadButton { button: u16, pressed: bool },
    GamepadAxis { axis: u16, value: f32 },
    FocusLost,
    Stop,
}

/// Result of routing one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    /// The event is now pending in the bounded queue.
    Enqueued,
    /// The event was refused: queue overflow, full track table, duplicate press
    /// or the router is already stopped.
    Dropped,
}

impl PushOutcome {
    pub fn is_enqueued(self) -> bool {
        matches!(self, PushOutcome::Enqueued)
    }
}

#[derive(Clone, Copy)]
struct Touch {
    id: u64,
    x: f32,
    y: f32,
}

struct EventQueue {
    slots: [Option<Event>; EVENT_CAPACITY],
    head: usize,
    len: usize,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            slots: [None; EVENT_CAPACITY],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, event: Event) -> bool {
        if self.len == EVENT_CAPACITY {
            return false;
        }
        let index = (self.head + self.len) % EVENT_CAPACITY;
        self.slots[index] = Some(event);
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<Event> {
        if self.len == 0 {
            return None;
        }
        let event = self.slots[self.head].take();
        self.head = (self.head + 1) % EVENT_CAPACITY;
        self.len -= 1;
        if self.len == 0 {
            self.head = 0;
        }
        event
    }

    fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
        self.head = 0;
        self.len = 0;
    }
}

/// Owns one bounded input queue and the pressed-state table.
///
/// The router is intentionally not `Clone`. All methods take `&mut self`, so
/// ownership of the routed state stays with the session host. `Drop` releases
/// every queued event and tracked press; no state is retained.
pub struct InputRouter {
    queue: EventQueue,
    touches: [Option<Touch>; MAX_TOUCHES],
    keys: [Option<u32>; MAX_KEYS],
    buttons: [Option<u16>; MAX_BUTTONS],
    axes: [Option<(u16, f32)>; MAX_AXES],
    stopped: bool,
}

impl InputRouter {
    pub fn new() -> Self {
        Self {
            queue: EventQueue::new(),
            touches: [None; MAX_TOUCHES],
            keys: [None; MAX_KEYS],
            buttons: [None; MAX_BUTTONS],
            axes: [None; MAX_AXES],
            stopped: false,
        }
    }

    /// Route one event. `FocusLost` and `Stop` run their cleanup paths here so
    /// callers cannot bypass pressed-state release by pushing the raw event.
    pub fn push(&mut self, event: Event) -> PushOutcome {
        if self.stopped {
            return PushOutcome::Dropped;
        }
        match event {
            Event::PointerDown { id, x, y } => self.pointer_down(id, x, y),
            Event::PointerMove { id, x, y } => self.pointer_move(id, x, y),
            Event::PointerUp { id, x, y } => self.pointer_up(id, x, y),
            Event::KeyDown { code } => self.key_down(code),
            Event::KeyUp { code } => self.key_up(code),
            Event::GamepadButton { button, pressed } => self.gamepad_button(button, pressed),
            Event::GamepadAxis { axis, value } => self.gamepad_axis(axis, value),
            Event::FocusLost => {
                self.focus_lost();
                PushOutcome::Enqueued
            }
            Event::Stop => {
                self.stop();
                PushOutcome::Enqueued
            }
        }
    }

    /// Clear pressed state and enqueue matching releases followed by
    /// `FocusLost`. Returns `false` when the router is already stopped.
    ///
    /// If the queue cannot fit the releases, pending events are discarded first;
    /// the queue remains bounded and no stale press can survive.
    pub fn focus_lost(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        self.flush_state(Event::FocusLost);
        true
    }

    /// Clear pressed state, enqueue releases and one `Stop`, and stop routing.
    ///
    /// Idempotent: the second and later calls do nothing and return `false`.
    pub fn stop(&mut self) -> bool {
        if self.stopped {
            return false;
        }
        self.flush_state(Event::Stop);
        self.stopped = true;
        true
    }

    /// Drain up to `limit` events in FIFO order. The limit is capped at
    /// `EVENT_CAPACITY`, so a caller cannot ask for an unbounded drain.
    pub fn drain(&mut self, limit: usize) -> Drain<'_> {
        Drain {
            router: self,
            remaining: limit.min(EVENT_CAPACITY),
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len
    }

    pub fn is_empty(&self) -> bool {
        self.queue.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.queue.len == EVENT_CAPACITY
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Number of currently tracked pressed touches, keys, buttons and nonzero
    /// axes. A focus-loss or stop clears this to zero.
    pub fn active_count(&self) -> usize {
        self.touches.iter().flatten().count()
            + self.keys.iter().flatten().count()
            + self.buttons.iter().flatten().count()
            + self.axes.iter().flatten().count()
    }

    fn enqueue(&mut self, event: Event) -> PushOutcome {
        if self.queue.push(event) {
            PushOutcome::Enqueued
        } else {
            PushOutcome::Dropped
        }
    }

    fn pointer_down(&mut self, id: u64, x: f32, y: f32) -> PushOutcome {
        let slot = self
            .touches
            .iter()
            .position(|touch| touch.is_some_and(|touch| touch.id == id))
            .or_else(|| self.touches.iter().position(Option::is_none));
        let Some(index) = slot else {
            return PushOutcome::Dropped;
        };
        if self.queue.push(Event::PointerDown { id, x, y }) {
            self.touches[index] = Some(Touch { id, x, y });
            PushOutcome::Enqueued
        } else {
            PushOutcome::Dropped
        }
    }

    fn pointer_move(&mut self, id: u64, x: f32, y: f32) -> PushOutcome {
        if let Some(touch) = self
            .touches
            .iter_mut()
            .find(|touch| touch.is_some_and(|touch| touch.id == id))
            .and_then(Option::as_mut)
        {
            touch.x = x;
            touch.y = y;
        }
        self.enqueue(Event::PointerMove { id, x, y })
    }

    fn pointer_up(&mut self, id: u64, x: f32, y: f32) -> PushOutcome {
        if let Some(slot) = self
            .touches
            .iter_mut()
            .find(|touch| touch.is_some_and(|touch| touch.id == id))
        {
            *slot = None;
        }
        self.enqueue(Event::PointerUp { id, x, y })
    }

    fn key_down(&mut self, code: u32) -> PushOutcome {
        if self.keys.iter().flatten().any(|&pressed| pressed == code) {
            return PushOutcome::Dropped;
        }
        let Some(index) = self.keys.iter().position(Option::is_none) else {
            return PushOutcome::Dropped;
        };
        if self.queue.push(Event::KeyDown { code }) {
            self.keys[index] = Some(code);
            PushOutcome::Enqueued
        } else {
            PushOutcome::Dropped
        }
    }

    fn key_up(&mut self, code: u32) -> PushOutcome {
        if let Some(slot) = self
            .keys
            .iter_mut()
            .find(|slot| slot.is_some_and(|pressed| pressed == code))
        {
            *slot = None;
        }
        self.enqueue(Event::KeyUp { code })
    }

    fn gamepad_button(&mut self, button: u16, pressed: bool) -> PushOutcome {
        if pressed {
            if self.buttons.iter().flatten().any(|&down| down == button) {
                return PushOutcome::Dropped;
            }
            let Some(index) = self.buttons.iter().position(Option::is_none) else {
                return PushOutcome::Dropped;
            };
            if self.queue.push(Event::GamepadButton {
                button,
                pressed: true,
            }) {
                self.buttons[index] = Some(button);
                PushOutcome::Enqueued
            } else {
                PushOutcome::Dropped
            }
        } else {
            if let Some(slot) = self
                .buttons
                .iter_mut()
                .find(|slot| slot.is_some_and(|down| down == button))
            {
                *slot = None;
            }
            self.enqueue(Event::GamepadButton {
                button,
                pressed: false,
            })
        }
    }

    fn gamepad_axis(&mut self, axis: u16, value: f32) -> PushOutcome {
        if value == 0.0 {
            if let Some(slot) = self
                .axes
                .iter_mut()
                .find(|slot| slot.is_some_and(|(tracked, _)| tracked == axis))
            {
                *slot = None;
            }
            return self.enqueue(Event::GamepadAxis { axis, value: 0.0 });
        }
        let index = self
            .axes
            .iter()
            .position(|slot| slot.is_some_and(|(tracked, _)| tracked == axis))
            .or_else(|| self.axes.iter().position(Option::is_none));
        let Some(index) = index else {
            return PushOutcome::Dropped;
        };
        if self.queue.push(Event::GamepadAxis { axis, value }) {
            self.axes[index] = Some((axis, value));
            PushOutcome::Enqueued
        } else {
            PushOutcome::Dropped
        }
    }

    /// Emit releases for every tracked press, then the caller's marker.
    ///
    /// The track tables are cleared even if an event had to be discarded, so a
    /// stale pressed state can never be retained by the router itself.
    fn flush_state(&mut self, marker: Event) {
        let releases = self.active_count();
        if self.queue.len + releases + 1 > EVENT_CAPACITY {
            self.queue.clear();
        }

        for slot in &mut self.touches {
            if let Some(touch) = slot.take() {
                let _ = self.queue.push(Event::PointerUp {
                    id: touch.id,
                    x: touch.x,
                    y: touch.y,
                });
            }
        }
        for slot in &mut self.keys {
            if let Some(code) = slot.take() {
                let _ = self.queue.push(Event::KeyUp { code });
            }
        }
        for slot in &mut self.buttons {
            if let Some(button) = slot.take() {
                let _ = self.queue.push(Event::GamepadButton {
                    button,
                    pressed: false,
                });
            }
        }
        for slot in &mut self.axes {
            if let Some((axis, _)) = slot.take() {
                let _ = self.queue.push(Event::GamepadAxis { axis, value: 0.0 });
            }
        }
        let _ = self.queue.push(marker);
    }
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for InputRouter {
    fn drop(&mut self) {
        self.queue.clear();
        self.touches = [None; MAX_TOUCHES];
        self.keys = [None; MAX_KEYS];
        self.buttons = [None; MAX_BUTTONS];
        self.axes = [None; MAX_AXES];
        self.stopped = true;
    }
}

/// Bounded FIFO drain iterator. It borrows the router, so routing cannot race
/// with draining and only one consumer can hold the router at a time.
pub struct Drain<'a> {
    router: &'a mut InputRouter,
    remaining: usize,
}

impl Iterator for Drain<'_> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        if self.remaining == 0 {
            return None;
        }
        match self.router.queue.pop() {
            Some(event) => {
                self.remaining -= 1;
                Some(event)
            }
            None => {
                self.remaining = 0;
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining.min(self.router.queue.len);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Drain<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    fn enqueued(router: &mut InputRouter, event: Event) {
        assert_eq!(router.push(event), PushOutcome::Enqueued, "{event:?}");
    }

    #[test]
    fn rejects_overflow_without_growing() {
        let mut router = InputRouter::new();
        for index in 0..EVENT_CAPACITY {
            enqueued(
                &mut router,
                Event::PointerMove {
                    id: 0,
                    x: index as f32,
                    y: 0.0,
                },
            );
        }
        assert_eq!(router.len(), EVENT_CAPACITY);
        assert!(router.is_full());
        assert_eq!(
            router.push(Event::KeyDown { code: 1 }),
            PushOutcome::Dropped
        );
        assert_eq!(router.len(), EVENT_CAPACITY);

        let drained: Vec<Event> = router.drain(EVENT_CAPACITY).collect();
        assert_eq!(drained.len(), EVENT_CAPACITY);
        assert_eq!(
            drained.first(),
            Some(&Event::PointerMove {
                id: 0,
                x: 0.0,
                y: 0.0
            })
        );
        assert_eq!(
            drained.last(),
            Some(&Event::PointerMove {
                id: 0,
                x: (EVENT_CAPACITY - 1) as f32,
                y: 0.0
            })
        );
        assert!(router.is_empty());
    }

    #[test]
    fn drains_in_order_with_limit() {
        let mut router = InputRouter::new();
        enqueued(
            &mut router,
            Event::PointerDown {
                id: 7,
                x: 0.1,
                y: 0.2,
            },
        );
        enqueued(
            &mut router,
            Event::PointerMove {
                id: 7,
                x: 0.3,
                y: 0.4,
            },
        );
        enqueued(
            &mut router,
            Event::PointerUp {
                id: 7,
                x: 0.3,
                y: 0.4,
            },
        );
        enqueued(&mut router, Event::KeyDown { code: 65 });
        enqueued(
            &mut router,
            Event::GamepadAxis {
                axis: 1,
                value: -0.5,
            },
        );

        let first: Vec<Event> = router.drain(2).collect();
        assert_eq!(
            first,
            vec![
                Event::PointerDown {
                    id: 7,
                    x: 0.1,
                    y: 0.2
                },
                Event::PointerMove {
                    id: 7,
                    x: 0.3,
                    y: 0.4
                }
            ]
        );
        assert_eq!(router.len(), 3);

        let second: Vec<Event> = router.drain(usize::MAX).collect();
        assert_eq!(
            second,
            vec![
                Event::PointerUp {
                    id: 7,
                    x: 0.3,
                    y: 0.4
                },
                Event::KeyDown { code: 65 },
                Event::GamepadAxis {
                    axis: 1,
                    value: -0.5
                }
            ]
        );
        assert!(router.is_empty());
    }

    #[test]
    fn focus_lost_releases_pressed_state() {
        let mut router = InputRouter::new();
        enqueued(
            &mut router,
            Event::PointerDown {
                id: 3,
                x: 0.5,
                y: 0.6,
            },
        );
        enqueued(&mut router, Event::KeyDown { code: 4 });
        enqueued(
            &mut router,
            Event::GamepadButton {
                button: 2,
                pressed: true,
            },
        );
        enqueued(
            &mut router,
            Event::GamepadAxis {
                axis: 0,
                value: 0.75,
            },
        );
        let _ = router.drain(EVENT_CAPACITY).collect::<Vec<_>>();
        assert_eq!(router.active_count(), 4);

        enqueued(&mut router, Event::FocusLost);
        assert_eq!(router.active_count(), 0);
        assert!(!router.is_stopped());

        let events: Vec<Event> = router.drain(EVENT_CAPACITY).collect();
        assert!(events.contains(&Event::PointerUp {
            id: 3,
            x: 0.5,
            y: 0.6
        }));
        assert!(events.contains(&Event::KeyUp { code: 4 }));
        assert!(events.contains(&Event::GamepadButton {
            button: 2,
            pressed: false
        }));
        assert!(events.contains(&Event::GamepadAxis {
            axis: 0,
            value: 0.0
        }));
        assert_eq!(events.last(), Some(&Event::FocusLost));
    }

    #[test]
    fn stop_is_idempotent_and_terminal() {
        let mut router = InputRouter::new();
        enqueued(
            &mut router,
            Event::PointerDown {
                id: 1,
                x: 0.0,
                y: 0.0,
            },
        );
        enqueued(&mut router, Event::KeyDown { code: 9 });
        assert_eq!(router.active_count(), 2);

        assert!(router.stop());
        assert!(!router.stop());
        assert!(router.is_stopped());
        assert_eq!(router.active_count(), 0);
        assert_eq!(
            router.push(Event::KeyDown { code: 10 }),
            PushOutcome::Dropped
        );
        assert!(!router.focus_lost());

        let events: Vec<Event> = router.drain(EVENT_CAPACITY).collect();
        assert!(events.contains(&Event::PointerUp {
            id: 1,
            x: 0.0,
            y: 0.0
        }));
        assert!(events.contains(&Event::KeyUp { code: 9 }));
        assert_eq!(events.last(), Some(&Event::Stop));
        assert_eq!(
            events.iter().filter(|event| **event == Event::Stop).count(),
            1
        );
    }

    #[test]
    fn stop_cleanup_survives_a_full_queue() {
        let mut router = InputRouter::new();
        enqueued(&mut router, Event::KeyDown { code: 11 });
        for index in 0..(EVENT_CAPACITY - 1) {
            enqueued(
                &mut router,
                Event::PointerMove {
                    id: 9,
                    x: index as f32,
                    y: 0.0,
                },
            );
        }
        assert!(router.is_full());

        assert!(router.stop());
        let events: Vec<Event> = router.drain(EVENT_CAPACITY).collect();
        assert!(events.contains(&Event::KeyUp { code: 11 }));
        assert_eq!(events.last(), Some(&Event::Stop));
    }

    #[test]
    fn drop_clears_owned_state() {
        assert!(std::mem::needs_drop::<InputRouter>());
        {
            let mut router = InputRouter::new();
            enqueued(
                &mut router,
                Event::PointerDown {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                },
            );
            for index in 0..(EVENT_CAPACITY - 1) {
                enqueued(
                    &mut router,
                    Event::PointerMove {
                        id: 1,
                        x: index as f32,
                        y: 0.0,
                    },
                );
            }
            drop(router);
        }

        let mut fresh = InputRouter::new();
        assert!(fresh.is_empty());
        assert_eq!(fresh.active_count(), 0);
        assert!(!fresh.is_stopped());
        enqueued(&mut fresh, Event::KeyDown { code: 1 });
        assert_eq!(fresh.len(), 1);
    }
}
