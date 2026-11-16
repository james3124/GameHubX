use eframe::egui::*;

use crate::theme::{ACC, CARD, DIM, W};

pub(crate) fn btn(ui: &mut Ui, t: &str, sz: Vec2, fill: Color32, fg: Color32, fs: f32) -> Response {
    let t = crate::i18n::tr(t);
    let (r, p) = ui.allocate_painter(sz, Sense::click());
    crate::focus::register(r.id);
    let rr = Rounding::same(16.0);
    p.rect_filled(r.rect, rr, fill);
    if r.has_focus() || r.hovered() {
        p.rect_stroke(
            r.rect,
            rr,
            Stroke::new(2.0_f32, Color32::from_white_alpha(200)),
        );
    }
    p.text(
        r.rect.center(),
        Align2::CENTER_CENTER,
        t,
        FontId::proportional(fs),
        fg,
    );
    r
}

pub(crate) fn chip(ui: &mut Ui, t: &str) {
    ui.label(
        RichText::new(crate::i18n::tr(t))
            .size(17.0)
            .strong()
            .color(Color32::BLACK)
            .background_color(Color32::from_rgb(216, 219, 226)),
    );
}

pub(crate) fn row(ui: &mut Ui, t: &str, d: &str, v: &str) -> Response {
    let t = crate::i18n::tr(t);
    let d = crate::i18n::tr(d);
    let v = crate::i18n::tr(v);
    let (r, p) = ui.allocate_painter(vec2(1000.0, 110.0), Sense::click());
    crate::focus::register(r.id);
    p.rect_filled(r.rect, Rounding::same(14.0), CARD);
    if r.has_focus() || r.hovered() {
        p.rect_stroke(
            r.rect,
            Rounding::same(14.0),
            Stroke::new(1.5_f32, Color32::from_white_alpha(120)),
        );
    }
    p.text(
        r.rect.left_top() + vec2(14.0, 24.0),
        Align2::LEFT_CENTER,
        t,
        FontId::proportional(26.0),
        W,
    );
    p.text(
        r.rect.left_top() + vec2(14.0, 66.0),
        Align2::LEFT_CENTER,
        d,
        FontId::proportional(18.0),
        DIM,
    );
    p.text(
        r.rect.right_center() - vec2(14.0, 0.0),
        Align2::RIGHT_CENTER,
        v,
        FontId::proportional(24.0),
        W,
    );
    r
}

pub(crate) fn tog(ui: &mut Ui, t: &str, d: &str, on: &mut bool) {
    let r = row(ui, t, d, "");
    let c = r.rect.right_center() - vec2(50.0, 0.0);
    ui.painter().rect_filled(
        Rect::from_center_size(c, vec2(68.0, 34.0)),
        17.0,
        if *on { ACC } else { Color32::from_gray(60) },
    );
    ui.painter()
        .circle_filled(c + vec2(if *on { 17.0 } else { -17.0 }, 0.0), 13.0, W);
    if r.clicked() {
        *on = !*on;
    }
}

pub(crate) fn short(s: &str) -> String {
    let n = s.chars().count();
    if s.is_empty() {
        crate::i18n::tr("Not set ›").into()
    } else if n > 30 {
        format!("…{} ›", s.chars().skip(n - 30).collect::<String>())
    } else {
        format!("{s} ›")
    }
}

pub(crate) fn hints(ctx: &Context, items: &[(&str, &str)]) {
    let p = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("hints")));
    let mut x = ctx.screen_rect().right() - 46.0;
    let y = ctx.screen_rect().bottom() - 40.0;
    for (k, t) in items.iter().rev() {
        let g = p.layout_no_wrap(
            crate::i18n::tr(t).to_string(),
            FontId::proportional(22.0),
            W,
        );
        x -= g.size().x;
        p.galley(pos2(x, y - g.size().y / 2.0), g, W);
        x -= 14.0;
        p.circle_filled(pos2(x - 18.0, y), 18.0, W);
        p.text(
            pos2(x - 18.0, y),
            Align2::CENTER_CENTER,
            *k,
            FontId::proportional(20.0),
            Color32::BLACK,
        );
        x -= 76.0;
    }
}

pub(crate) fn dim(ctx: &Context) {
    ctx.layer_painter(LayerId::new(Order::Middle, Id::new("dim")))
        .rect_filled(ctx.screen_rect(), 0.0, Color32::from_black_alpha(150));
}
