//! Lightweight in-game HUD overlay.
//!
//! `draw` paints a compact, non-intrusive panel in the top-left corner using a
//! dedicated foreground layer. It is intentionally *not* wired into `app.rs`;
//! the host integration decides when to call it. All state (frame-rate and PSS
//! history) is thread-local, so no new `App` fields are needed.

use std::cell::RefCell;
use std::time::Instant;

use eframe::egui::*;

use crate::app::App;
use crate::theme::GREEN;

const FPS_SAMPLES: usize = 48;
const PSS_SAMPLES: usize = 48;

/// Fixed-capacity ring that preserves insertion order when read back.
struct FpsRing {
    values: [f32; FPS_SAMPLES],
    len: usize,
    next: usize,
    last: Option<Instant>,
}

impl FpsRing {
    const fn new() -> Self {
        Self {
            values: [0.0; FPS_SAMPLES],
            len: 0,
            next: 0,
            last: None,
        }
    }

    fn push(&mut self, value: f32) {
        self.values[self.next] = value;
        self.next = (self.next + 1) % FPS_SAMPLES;
        self.len = (self.len + 1).min(FPS_SAMPLES);
    }

    /// Measure the instantaneous frame rate from consecutive calls.
    fn push_now(&mut self) {
        let now = Instant::now();
        let fps = self
            .last
            .map(|previous| {
                let seconds = now.duration_since(previous).as_secs_f32();
                if seconds > 0.0 && seconds < 1.0 {
                    (1.0 / seconds).clamp(0.0, 240.0)
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        self.last = Some(now);
        self.push(fps);
    }

    fn ordered(&self) -> Vec<f32> {
        let start = (self.next + FPS_SAMPLES - self.len) % FPS_SAMPLES;
        (0..self.len)
            .map(|index| self.values[(start + index) % FPS_SAMPLES])
            .collect()
    }

    fn reset_timing(&mut self) {
        self.last = None;
    }
}

struct ValueRing<const N: usize> {
    values: [u64; N],
    len: usize,
    next: usize,
}

impl<const N: usize> ValueRing<N> {
    const fn new() -> Self {
        Self {
            values: [0; N],
            len: 0,
            next: 0,
        }
    }

    fn push(&mut self, value: u64) {
        self.values[self.next] = value;
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
    }

    fn ordered(&self) -> Vec<u64> {
        let start = (self.next + N - self.len) % N;
        (0..self.len)
            .map(|index| self.values[(start + index) % N])
            .collect()
    }

    fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
    }
}

thread_local! {
    static FPS: RefCell<FpsRing> = RefCell::new(FpsRing::new());
    static PSS: RefCell<ValueRing<PSS_SAMPLES>> = RefCell::new(ValueRing::new());
}

/// Record one PSS sample (KB). The session screen calls this as it polls.
pub(crate) fn push_pss(total_kb: u64) {
    PSS.with(|ring| ring.borrow_mut().push(total_kb));
}

/// PSS history, oldest first.
pub(crate) fn pss_samples() -> Vec<u64> {
    PSS.with(|ring| ring.borrow().ordered())
}

pub(crate) fn clear_pss() {
    PSS.with(|ring| ring.borrow_mut().clear());
}

pub(crate) fn fps_samples() -> Vec<f32> {
    FPS.with(|ring| ring.borrow().ordered())
}

/// Map a series of values to points inside `rect`, oldest on the left.
/// `max` is the value that maps to the top of the graph.
pub(crate) fn line_points(values: &[f32], rect: Rect, max: f32) -> Vec<Pos2> {
    if values.len() < 2 || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Vec::new();
    }
    let span = (values.len() - 1) as f32;
    let ceiling = if max.is_finite() && max > 0.0 {
        max
    } else {
        1.0
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = rect.left() + (index as f32 / span) * rect.width();
            let height = (*value / ceiling).clamp(0.0, 1.0);
            pos2(x, rect.bottom() - height * rect.height())
        })
        .collect()
}

/// Paint the HUD. Visible only while a session is active and the configured
/// overlay opacity is non-zero. `app` is borrowed immutably.
pub(crate) fn draw(app: &App, ctx: &Context) {
    let opacity = app.game_settings.overlay_opacity;
    if opacity == 0 || (app.session.is_none() && app.session_pss.is_none()) {
        FPS.with(|ring| ring.borrow_mut().reset_timing());
        return;
    }

    let fps = FPS.with(|ring| {
        let mut ring = ring.borrow_mut();
        ring.push_now();
        ring.ordered()
    });
    let current_fps = fps.last().copied().unwrap_or(0.0);
    let alpha = (opacity as f32 / 100.0).clamp(0.2, 1.0);

    let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("gamehubx-hud")));
    let rect = Rect::from_min_size(ctx.screen_rect().min + vec2(18.0, 18.0), vec2(268.0, 124.0));
    painter.rect_filled(
        rect,
        Rounding::same(10.0),
        Color32::from_black_alpha((170.0 * alpha) as u8),
    );
    painter.rect_stroke(
        rect,
        Rounding::same(10.0),
        Stroke::new(1.0_f32, Color32::from_white_alpha((70.0 * alpha) as u8)),
    );

    let text = Color32::from_white_alpha((255.0 * alpha) as u8);
    let dim = Color32::from_white_alpha((170.0 * alpha) as u8);
    let x = rect.left() + 12.0;
    let mut y = rect.top() + 9.0;

    painter.text(
        pos2(x, y),
        Align2::LEFT_TOP,
        crate::i18n::tr("GameHubX HUD"),
        FontId::proportional(14.0),
        GREEN,
    );
    y += 19.0;
    painter.text(
        pos2(x, y),
        Align2::LEFT_TOP,
        format!(
            "{} {} · {:.0} FPS",
            crate::i18n::tr("FPS cap"),
            app.game_settings.fps_cap,
            current_fps
        ),
        FontId::proportional(14.0),
        text,
    );
    y += 18.0;
    let pss_text = match app.session_pss {
        Some(sample) => format!("PSS {} KB", sample.total_kb),
        None => crate::i18n::tr("PSS --").to_string(),
    };
    painter.text(
        pos2(x, y),
        Align2::LEFT_TOP,
        pss_text,
        FontId::proportional(14.0),
        text,
    );
    y += 18.0;
    let battery = gamehubx_platform::battery_level()
        .map(|level| format!("{level}%"))
        .unwrap_or_else(|| "--".to_string());
    painter.text(
        pos2(x, y),
        Align2::LEFT_TOP,
        format!("{} {battery}", crate::i18n::tr("Battery")),
        FontId::proportional(14.0),
        dim,
    );
    y += 18.0;

    let graph = Rect::from_min_max(
        pos2(rect.left() + 12.0, y),
        pos2(rect.right() - 12.0, rect.bottom() - 8.0),
    );
    painter.line_segment(
        [
            pos2(graph.left(), graph.bottom()),
            pos2(graph.right(), graph.bottom()),
        ],
        Stroke::new(1.0_f32, dim),
    );
    let ceiling = (app.game_settings.fps_cap.max(30) as f32) * 1.2;
    let points = line_points(&fps, graph, ceiling);
    if points.len() >= 2 {
        painter.add(Shape::line(points, Stroke::new(1.5_f32, GREEN)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_newest_entries_in_order() {
        let mut ring = ValueRing::<4>::new();
        for value in 1..=6u64 {
            ring.push(value);
        }
        assert_eq!(ring.ordered(), vec![3, 4, 5, 6]);
        ring.clear();
        assert!(ring.ordered().is_empty());
    }

    #[test]
    fn fps_ring_wraps_and_orders() {
        let mut ring = FpsRing::new();
        for value in [10.0, 20.0, 30.0] {
            ring.push(value);
        }
        assert_eq!(ring.ordered(), vec![10.0, 20.0, 30.0]);
    }

    #[test]
    fn line_points_maps_to_rect() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        let points = line_points(&[0.0, 5.0, 10.0], rect, 10.0);
        assert_eq!(points.len(), 3);
        assert_eq!(points[0], pos2(0.0, 50.0));
        assert_eq!(points[1], pos2(50.0, 25.0));
        assert_eq!(points[2], pos2(100.0, 0.0));
    }

    #[test]
    fn line_points_needs_two_samples() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 50.0));
        assert!(line_points(&[], rect, 1.0).is_empty());
        assert!(line_points(&[1.0], rect, 1.0).is_empty());
    }

    #[test]
    fn pss_history_round_trip() {
        clear_pss();
        push_pss(1024);
        push_pss(2048);
        assert_eq!(pss_samples(), vec![1024, 2048]);
        clear_pss();
    }
}
