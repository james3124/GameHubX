use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::DIM;
use crate::widgets::{chip, row};

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");
    ui.horizontal(|ui| {
        ui.add_space(86.0);
        ui.add(
            TextEdit::singleline(&mut app.query)
                .hint_text(crate::i18n::tr("Search for Game Name"))
                .desired_width(1100.0)
                .font(FontId::proportional(26.0))
                .margin(vec2(20.0, 18.0)),
        );
    });
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.add_space(90.0);
        chip(ui, "LB");
        ui.label(RichText::new(crate::i18n::tr("All")).size(25.0).strong());
        chip(ui, "RB");
    });
    ui.add_space(20.0);

    let query = app.query.trim().to_lowercase();
    if query.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(90.0);
            ui.label(
                RichText::new(crate::i18n::tr("Popular Recommendations"))
                    .size(26.0)
                    .strong(),
            );
        });
        ui.horizontal(|ui| {
            ui.add_space(90.0);
            ui.label(
                RichText::new(crate::i18n::tr("Type a game name to search your library"))
                    .size(22.0)
                    .color(DIM),
            );
        });
        return;
    }

    let matches: Vec<usize> = app
        .games
        .iter()
        .enumerate()
        .filter(|(_, game)| {
            game.label.to_lowercase().contains(&query) || game.intro.to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .collect();

    ui.horizontal(|ui| {
        ui.add_space(90.0);
        ui.label(
            RichText::new(format!("Results ({})", matches.len()))
                .size(26.0)
                .strong(),
        );
    });

    if matches.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(90.0);
            ui.label(
                RichText::new(crate::i18n::tr("No games found"))
                    .size(24.0)
                    .color(DIM),
            );
        });
        return;
    }

    let mut act = None;
    ScrollArea::vertical()
        .id_salt("search_results")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(70.0);
                ui.vertical(|ui| {
                    for index in &matches {
                        let game = &app.games[*index];
                        let desc = if game.intro.is_empty() {
                            "Local game"
                        } else {
                            game.intro.as_str()
                        };
                        if row(ui, &game.label, desc, "›").clicked() {
                            act = Some(*index);
                        }
                    }
                });
            });
        });

    if let Some(index) = act {
        app.cur = index;
        app.go(S::Detail);
    }
}
