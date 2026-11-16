use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{btn, game_card, CardArt};

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Home");
    let mut sel = app.sel;
    let mut open = None;
    let mut import = false;

    ScrollArea::vertical()
        .id_salt("dashboard_scroll")
        .show(ui, |ui| {
            ui.add_space(6.0);
            section(ui, "Platforms");
            ScrollArea::horizontal()
                .id_salt("dashboard_platforms")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(70.0);
                        let pc = game_card(
                            ui,
                            "PC Emulator",
                            CardArt::Pc,
                            Color32::from_rgb(31, 44, 58),
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
                            Color32::from_rgb(29, 51, 71),
                            sel == 1,
                        );
                        if steam.hovered() || steam.has_focus() {
                            sel = 1;
                        }
                    });
                });

            ui.add_space(18.0);
            section(ui, "My Games");
            ScrollArea::horizontal()
                .id_salt("dashboard_games")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(70.0);
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
                        } else {
                            for (index, game) in app.games.iter().enumerate() {
                                let selected = sel == index + 2;
                                let card = game_card(
                                    ui,
                                    &game.label,
                                    CardArt::Initial,
                                    Color32::from_rgb(45, 50, 61),
                                    selected,
                                );
                                if card.hovered() || card.has_focus() {
                                    sel = index + 2;
                                }
                                if card.clicked() {
                                    open = Some(index + 2);
                                }
                            }
                        }
                    });
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
                    .unwrap_or(("", "")),
            };

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.add_space(74.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(crate::i18n::tr(title)).size(30.0).strong());
                    ui.label(RichText::new(crate::i18n::tr(desc)).size(20.0).color(DIM));
                });
            });
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(74.0);
                if btn(ui, "Import PC games", vec2(420.0, 74.0), CARD, W, 24.0).clicked() {
                    import = true;
                }
                if btn(ui, "⚙", vec2(74.0, 74.0), CARD, W, 28.0).clicked() {
                    app.go(S::App);
                }
            });
            ui.add_space(30.0);
        });

    app.sel = sel;

    if import {
        app.open_files(true);
    }
    if let Some(index) = open {
        match index {
            0 => app.go(S::Pc),
            1 => {}
            n => {
                app.cur = n - 2;
                app.go(S::Detail);
            }
        }
    }
}

fn section(ui: &mut Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.label(
            RichText::new(crate::i18n::tr(title))
                .size(23.0)
                .strong()
                .color(W),
        );
    });
}
