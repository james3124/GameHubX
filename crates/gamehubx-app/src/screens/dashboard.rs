use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::*;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Dashboard");
    ui.add_space(8.0);

    let mut sel = app.sel;
    let mut click = None;
    let mut secondary = None;

    ScrollArea::horizontal()
        .id_salt("dashboard_tiles")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(70.0);
                for (i, tile) in app.tiles.iter().enumerate() {
                    let fill = match i {
                        0 => Color32::from_rgb(18, 18, 22),
                        1 => Color32::from_rgb(34, 64, 96),
                        _ => Color32::from_rgb(48, 34, 52),
                    };
                    let label = match i {
                        0 => "",
                        1 => "STEAM",
                        _ => tile.ini.as_str(),
                    };
                    let response = btn(ui, label, vec2(250.0, 250.0), fill, W, 30.0);
                    if i == 0 {
                        let center = response.rect.center();
                        for (k, col) in [
                            (0, (243, 83, 37)),
                            (1, (129, 188, 6)),
                            (2, (5, 166, 240)),
                            (3, (255, 186, 8)),
                        ] {
                            let offset =
                                vec2((k % 2) as f32 * 38.0 - 38.0, (k / 2) as f32 * 38.0 - 38.0);
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    center + offset + vec2(2.0, 2.0),
                                    vec2(34.0, 34.0),
                                ),
                                0.0,
                                Color32::from_rgb(col.0, col.1, col.2),
                            );
                        }
                    }
                    if response.hovered() || response.has_focus() {
                        sel = i;
                    }
                    if response.clicked() {
                        click = Some(i);
                    }
                    if i > 1 && response.secondary_clicked() {
                        secondary = Some(i);
                    }
                }
            });
        });

    app.sel = sel;

    let (title, desc): (&str, &str) = match app.sel {
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
            .unwrap_or(("", "")),
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
            app.open_files(true);
        }
        if btn(ui, "⚙", vec2(82.0, 82.0), CARD, W, 30.0).clicked() {
            app.go(S::App);
        }
    });

    if let Some(i) = click {
        match i {
            0 => app.go(S::Pc),
            1 => {}
            _ => {
                app.cur = i - 2;
                app.go(S::Detail);
            }
        }
    }
    if let Some(i) = secondary {
        app.cur = i - 2;
        app.popup = true;
    }
}
