use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");
    let ctx = ui.ctx().clone();
    let (id, label, intro, has_exe, has_cover) = match app.games.get(app.cur) {
        Some(game) => (
            game.local_id(),
            game.label.clone(),
            game.intro.clone(),
            game.exe.is_some(),
            game.cover.is_some(),
        ),
        None => (
            "local_unknown".to_string(),
            String::new(),
            String::new(),
            false,
            false,
        ),
    };

    ui.horizontal_top(|ui| {
        ui.add_space(70.0);
        ui.vertical(|ui| {
            ui.add_space(10.0);
            // Cover art: explicit image or PE icon extracted from the exe.
            let selected_game = app.games.get(app.cur).cloned();
            let cover = selected_game
                .as_ref()
                .and_then(|game| app.cover_texture(&ctx, game));
            let (rect, _) = ui.allocate_exact_size(vec2(220.0, 220.0), Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(16.0), CARD);
            if let Some(texture) = cover {
                let sized =
                    eframe::egui::load::SizedTexture::new(texture.id(), texture.size_vec2());
                ui.put(
                    rect,
                    Image::new(sized).fit_to_exact_size(vec2(220.0, 220.0)),
                );
            } else {
                let initial = label
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    initial,
                    FontId::proportional(96.0),
                    W,
                );
            }
        });
        ui.add_space(30.0);
        ui.vertical(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new(format!("Local Game ID: {id}"))
                    .size(20.0)
                    .color(DIM),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(&label).size(46.0).strong());
            ui.add_space(8.0);
            if !intro.is_empty() {
                ui.label(RichText::new(&intro).size(20.0).color(DIM));
            }
            ui.add_space(8.0);
            let status = if has_exe {
                "PC · .exe"
            } else {
                "Startup path not set"
            };
            ui.label(
                RichText::new(status)
                    .size(18.0)
                    .color(if has_exe { GREEN } else { DIM }),
            );
            ui.label(
                RichText::new(format!(
                    "Cover: {}  ·  EXE icon extraction on import",
                    if has_cover { "set" } else { "auto" }
                ))
                .size(16.0)
                .color(DIM),
            );
        });
    });

    ui.add_space(60.0);
    ui.horizontal(|ui| {
        ui.add_space(70.0);
        if btn(
            ui,
            "⊞  Play Now",
            vec2(440.0, 90.0),
            W,
            Color32::BLACK,
            26.0,
        )
        .clicked()
        {
            app.launch_game();
        }
        if btn(ui, "•••", vec2(90.0, 90.0), CARD, W, 26.0).clicked() {
            app.go(S::Set);
        }
    });
}
