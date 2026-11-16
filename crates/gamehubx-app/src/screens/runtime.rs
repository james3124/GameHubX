use eframe::egui::*;
use gamehubx_runtime::RuntimeLayout;

use crate::app::App;
use crate::theme::{CARD, DIM, W};
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Runtime");

    let root = app.runtime_root();
    let pack = app.runtime_pack_dir();
    let layout = RuntimeLayout::new(&root);
    let missing = layout.missing_components();

    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.vertical(|ui| {
            let title = if missing.is_empty() {
                "Runtime pack ready"
            } else {
                "Runtime pack incomplete"
            };
            ui.label(
                RichText::new(crate::i18n::tr(title))
                    .size(30.0)
                    .strong(),
            );
            ui.label(
                RichText::new(format!(
                    "{}: {}",
                    crate::i18n::tr("Runtime"),
                    root.display()
                ))
                .size(18.0)
                .color(DIM),
            );
            ui.label(
                RichText::new(format!("{}: {}", crate::i18n::tr("Pack"), pack.display()))
                    .size(18.0)
                    .color(DIM),
            );

            if missing.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::tr("All required components are present."))
                        .size(22.0)
                        .color(DIM),
                );
            } else {
                ui.label(
                    RichText::new(crate::i18n::tr("Missing:"))
                        .size(24.0)
                        .strong(),
                );
                for path in &missing {
                    ui.label(
                        RichText::new(path.display().to_string())
                            .size(18.0)
                            .monospace()
                            .color(DIM),
                    );
                }
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "Place manifest.json and <component>.tar.gz files in the pack folder, then install.",
                    ))
                    .size(18.0)
                    .color(DIM),
                );
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "HTTPS components can be queued from this manifest and installed after they finish.",
                    ))
                    .size(18.0)
                    .color(DIM),
                );
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "Pulse audio also needs rootfs/bin/pulseaudio; DXVK needs <runtime>/dxvk.",
                    ))
                    .size(18.0)
                    .color(DIM),
                );
            }

            if let Some(message) = &app.runtime_message {
                ui.label(RichText::new(message).size(20.0));
            }
            if app.runtime_installing {
                ui.label(
                    RichText::new(crate::i18n::tr("Installing..."))
                        .size(22.0)
                        .color(DIM),
                );
            }

            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if !app.runtime_installing
                    && btn(ui, "Download components", vec2(360.0, 72.0), CARD, W, 24.0)
                        .clicked()
                {
                    app.queue_runtime_downloads();
                }
                if !app.runtime_installing
                    && btn(ui, "Install runtime pack", vec2(380.0, 72.0), CARD, W, 24.0)
                        .clicked()
                {
                    app.start_runtime_install();
                }
                if btn(ui, "Downloads", vec2(240.0, 72.0), CARD, W, 24.0).clicked() {
                    app.go(crate::app::S::Dl);
                }
                if btn(ui, "Back", vec2(220.0, 72.0), CARD, W, 24.0).clicked() {
                    app.back();
                }
            });
        });
    });
}
