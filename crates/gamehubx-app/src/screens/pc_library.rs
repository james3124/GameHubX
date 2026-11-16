use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{game_card, CardArt};

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "PC Games");
    let mut open = None;
    let mut import = false;

    ScrollArea::vertical().id_salt("pc_library").show(ui, |ui| {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(70.0);
            ui.label(
                RichText::new(crate::i18n::tr("My Games"))
                    .size(24.0)
                    .strong(),
            );
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add_space(70.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(18.0, 18.0);
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
                    ui.label(
                        RichText::new(crate::i18n::tr("No Games Available"))
                            .size(24.0)
                            .color(DIM),
                    );
                    ui.label(
                        RichText::new(crate::i18n::tr(
                            "Head over to the Discovery Channel to find your favorite games",
                        ))
                        .size(18.0)
                        .color(DIM),
                    );
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for (index, game) in app.games.iter().enumerate() {
                            if game_card(
                                ui,
                                &game.label,
                                CardArt::Initial,
                                Color32::from_rgb(45, 50, 61),
                                false,
                            )
                            .clicked()
                            {
                                open = Some(index);
                            }
                        }
                    });
                }
            });
        });
        ui.add_space(30.0);
    });

    if import {
        app.open_files(true);
    }
    if let Some(index) = open {
        app.cur = index;
        app.go(S::Detail);
    }
}
