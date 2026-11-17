use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{btn, game_card, CardArt};

/// Reference-style dashboard (GameHub Lite `main.rs.orig`).
///
/// The original used a single direct-layout `Ui` with no nested `ScrollArea`s.
/// The redesigned version wrapped the tiles/controls in `ScrollArea::vertical`
/// + two `ScrollArea::horizontal`s; under Android's `fit_design` zoom those
/// nested clip rects can collapse, blanking the whole `CentralPanel` while the
/// background mesh and controller hints (drawn with `layer_painter`) still
/// render — exactly the reported symptom. Restoring direct layout removes the
/// clipping so the dashboard fills the available screen at any scale.
pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Dashboard");
    ui.add_space(8.0);

    let mut sel = app.sel;
    let mut open = None;
    let mut import = false;

    // Platform tiles + local games in one horizontal row, no scroll container.
    ui.horizontal(|ui| {
        ui.add_space(70.0);
        let pc = game_card(
            ui,
            "PC Emulator",
            CardArt::Pc,
            Color32::from_rgb(18, 18, 22),
            sel == 0,
        );
        if pc.hovered() || pc.has_focus() {
            sel = 0;
        }
        if pc.clicked() {
            open = Some(0);
        }

        let steam = game_card(
            ui,
            "Steam",
            CardArt::Steam,
            Color32::from_rgb(34, 64, 96),
            sel == 1,
        );
        if steam.hovered() || steam.has_focus() {
            sel = 1;
        }
        if steam.clicked() {
            open = Some(1);
        }

        // Clone the lightweight game list once so we can load cover textures
        // (which borrow `app` mutably) without aliasing `app.games`.
        let games = app.games.clone();
        for (index, game) in games.iter().enumerate() {
            let slot = index + 2;
            let selected = sel == slot;
            let image = app.cover_texture(ui.ctx(), game);
            let card = game_card_with(ui, &game.label, image.as_ref(), selected);
            if card.hovered() || card.has_focus() {
                sel = slot;
            }
            if card.clicked() {
                open = Some(slot);
            }
            if card.secondary_clicked() {
                app.cur = index;
                app.sel = slot;
                app.popup = true;
            }
        }

        if app.games.is_empty() {
            let add = game_card(
                ui,
                "Import PC games",
                CardArt::Initial,
                Color32::from_rgb(45, 50, 61),
                false,
            );
            if add.clicked() {
                import = true;
            }
        }
    });

    let (title, desc): (&str, &str) = match sel {
        0 => ("PC Emulator", "Import PC games, play AAA titles on mobile"),
        1 => ("Steam", "Log in to your Steam account"),
        n => app
            .games
            .get(n - 2)
            .map(|game| {
                (
                    game.label.as_str(),
                    if game.intro.is_empty() {
                        "Local game"
                    } else {
                        game.intro.as_str()
                    },
                )
            })
            .unwrap_or(("PC Emulator", "Import PC games, play AAA titles on mobile")),
    };

    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.add_space(74.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(crate::i18n::tr(title)).size(32.0).strong());
            ui.label(RichText::new(crate::i18n::tr(desc)).size(22.0).color(DIM));
        });
    });
    ui.add_space(30.0);
    ui.horizontal(|ui| {
        ui.add_space(74.0);
        if btn(ui, "Import PC games", vec2(490.0, 82.0), CARD, W, 26.0).clicked() {
            import = true;
        }
        if btn(ui, "⚙", vec2(82.0, 82.0), CARD, W, 30.0).clicked() {
            app.go(S::App);
        }
    });

    app.sel = sel;

    if import {
        app.open_files(true);
    }
    if let Some(index) = open {
        match index {
            0 => app.go(S::Pc),
            1 => {} // Steam is intentionally out of scope.
            n => {
                app.cur = n - 2;
                app.go(S::Detail);
            }
        }
    }
}

fn game_card_with(
    ui: &mut Ui,
    label: &str,
    texture: Option<&TextureHandle>,
    selected: bool,
) -> Response {
    let label = crate::i18n::tr(label);
    let size = vec2(210.0, 250.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    crate::focus::register(response.id);
    let card = Rect::from_min_size(rect.min, vec2(size.x, 200.0));
    if texture.is_some() {
        ui.painter()
            .rect_filled(card, Rounding::same(14.0), Color32::from_rgb(45, 50, 61));
        let tex = texture.unwrap();
        let sized = eframe::egui::load::SizedTexture::new(tex.id(), tex.size_vec2());
        ui.put(
            card,
            Image::new(sized).fit_to_exact_size(vec2(210.0, 200.0)),
        );
    }
    {
        let painter = ui.painter();
        if texture.is_none() {
            painter.rect_filled(card, Rounding::same(14.0), Color32::from_rgb(45, 50, 61));
            let initial = label
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            painter.text(
                card.center(),
                Align2::CENTER_CENTER,
                initial,
                FontId::proportional(64.0),
                W,
            );
        }
        let short: String = label.chars().take(22).collect();
        painter.text(
            pos2(rect.center().x, rect.bottom() - 22.0),
            Align2::CENTER_CENTER,
            short,
            FontId::proportional(19.0),
            if selected { W } else { DIM },
        );
        if selected {
            painter.rect_stroke(card, Rounding::same(14.0), Stroke::new(2.5_f32, ACC));
        } else if response.hovered() || response.has_focus() {
            painter.rect_stroke(
                card,
                Rounding::same(14.0),
                Stroke::new(2.0_f32, Color32::from_white_alpha(180)),
            );
        }
    }
    response
}
