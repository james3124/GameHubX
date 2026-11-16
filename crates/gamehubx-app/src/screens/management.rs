use eframe::egui::*;
use gamehubx_platform::format_bytes;

use crate::app::App;
use crate::theme::{ACC, DIM};

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");
    app.tabs(ui, true);
    ui.add_space(30.0);
    app.refresh_storage();

    let Some(stats) = app.storage else {
        ui.horizontal(|ui| {
            ui.add_space(100.0);
            ui.label(
                RichText::new(crate::i18n::tr("Storage statistics unavailable"))
                    .size(24.0)
                    .color(DIM),
            );
        });
        return;
    };

    let used = stats.used();
    let ratio = if stats.total == 0 {
        0.0
    } else {
        used as f32 / stats.total as f32
    };

    ui.horizontal(|ui| {
        ui.add_space(100.0);
        ui.vertical(|ui| {
            let (rect, painter) = ui.allocate_painter(vec2(1080.0, 12.0), Sense::hover());
            painter.rect_filled(rect.rect, 6.0, Color32::from_rgb(42, 48, 66));
            painter.rect_filled(
                Rect::from_min_size(rect.rect.min, vec2(1080.0 * ratio.clamp(0.0, 1.0), 12.0)),
                6.0,
                Color32::from_rgb(140, 147, 168),
            );
            ui.horizontal(|ui| {
                for (color, text) in [
                    (ACC, format!("Used {}", format_bytes(used))),
                    (
                        Color32::from_rgb(140, 147, 168),
                        format!("Available {}", format_bytes(stats.available)),
                    ),
                    (
                        Color32::from_gray(70),
                        format!("Total {}", format_bytes(stats.total)),
                    ),
                ] {
                    ui.label(RichText::new("●").color(color));
                    ui.label(RichText::new(text).size(24.0));
                }
            });
        });
    });
}
