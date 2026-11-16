use eframe::egui::*;

use crate::app::App;
use crate::theme::DIM;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "PC Games");
    ui.add_space(150.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("🎮?").size(80.0).color(DIM));
        ui.label(
            RichText::new(crate::i18n::tr("No Games Available"))
                .size(34.0)
                .strong()
                .color(DIM),
        );
        ui.label(
            RichText::new(crate::i18n::tr(
                "Head over to the Discovery Channel to find your favorite games",
            ))
            .size(24.0)
            .color(DIM),
        );
    });
}
