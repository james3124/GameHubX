use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{btn, row};

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Settings");
    let (detail, state) = match (&app.store, &app.store_error) {
        (Some(_), _) => ("SQLite library loaded".to_string(), "Ready ›".to_string()),
        (None, Some(error)) => (error.clone(), "Unavailable ›".to_string()),
        (None, None) => (
            "No library database".to_string(),
            "Unavailable ›".to_string(),
        ),
    };
    let access = if app.files_access {
        "Granted ›"
    } else {
        "Not granted ›"
    };
    ui.horizontal_top(|ui| {
        ui.add_space(46.0);
        ui.vertical(|ui| {
            if btn(ui, "Advanced", vec2(440.0, 88.0), CARD, W, 26.0).clicked() {
                app.go(S::Diag);
            }
            btn(ui, "About", vec2(440.0, 88.0), CLEAR, DIM, 26.0);
        });
        ui.vertical(|ui| {
            let language = app.language_value();
            if row(ui, "Language", "", &language).clicked() {
                app.cycle_language();
            }
            row(ui, "Library Database", &detail, &state);
            let access_response = row(ui, "All Files Access", "Required for import", access);
            if access_response.clicked() {
                app.files_access_error = None;
                if let Err(error) = gamehubx_platform::request_files_access() {
                    app.files_access_error = Some(error);
                }
            }
            if let Some(error) = &app.files_access_error {
                ui.label(RichText::new(error).size(16.0).color(ACC));
            }
            if row(
                ui,
                "Runtime Pack",
                "Install/verify Wine/Box64/PRoot/X-server",
                "›",
            )
            .clicked()
            {
                app.go(S::Runtime);
            }
            if row(
                ui,
                "Winlator Session",
                "Open the GPLv3 Winlator setup/container launcher",
                "›",
            )
            .clicked()
            {
                app.open_winlator();
            }
            row(ui, "Clear Cache", "", "");
            if let Some(notice) = &app.settings_notice {
                ui.label(RichText::new(notice).size(18.0).color(ACC));
            }
        });
    });
}
