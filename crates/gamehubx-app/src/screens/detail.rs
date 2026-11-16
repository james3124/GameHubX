use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");
    let (id, label) = match app.games.get(app.cur) {
        Some(game) => (game.local_id(), game.label.clone()),
        None => ("local_unknown".to_string(), String::new()),
    };

    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.label(RichText::new(format!("Local Game ID: {id}")).size(22.0));
    });
    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.label(RichText::new(label).size(46.0).strong());
    });
    ui.add_space(210.0);
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
