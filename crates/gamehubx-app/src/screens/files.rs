use eframe::egui::*;
use gamehubx_platform::{files_root, format_bytes};

use crate::app::{App, S};
use crate::theme::{CARD, DIM, W};
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");

    if !app.files_access {
        ui.horizontal(|ui| {
            ui.add_space(70.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "All files access is required to browse and import games.",
                    ))
                    .size(24.0)
                    .color(DIM),
                );
                if let Some(error) = &app.files_access_error {
                    ui.label(
                        RichText::new(error)
                            .size(18.0)
                            .color(Color32::from_rgb(245, 120, 120)),
                    );
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if btn(ui, "Grant file access", vec2(320.0, 72.0), CARD, W, 24.0).clicked() {
                        app.files_access_error = None;
                        if let Err(error) = gamehubx_platform::request_files_access() {
                            app.files_access_error = Some(error);
                        }
                    }
                    if btn(ui, "Check again", vec2(260.0, 72.0), CARD, W, 24.0).clicked() {
                        app.refresh_files_access();
                        if app.files_access {
                            app.nav(files_root());
                        }
                    }
                });
            });
        });
        return;
    }

    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.label(
            RichText::new(app.path.display().to_string())
                .size(22.0)
                .color(DIM),
        );
    });

    let mut act = None;
    ScrollArea::vertical().id_salt("files").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(50.0);
            for (name, is_dir, count) in &app.ents {
                let sub = if *is_dir {
                    format!("{count} file")
                } else {
                    format_bytes(*count as u64)
                };
                let short_name: String = name.chars().take(20).collect();
                let icon = if *is_dir {
                    "📁"
                } else if name.to_lowercase().ends_with(".exe") {
                    "👾"
                } else {
                    "📄"
                };
                if btn(
                    ui,
                    &format!("{icon}\n{short_name}\n{sub}"),
                    vec2(260.0, 190.0),
                    Color32::TRANSPARENT,
                    Color32::WHITE,
                    22.0,
                )
                .clicked()
                {
                    act = Some((name.clone(), *is_dir));
                }
            }
        });
    });

    if let Some((name, is_dir)) = act {
        let path = app.path.join(&name);
        if is_dir {
            app.nav(path);
        } else if name.to_lowercase().ends_with(".exe") {
            if app.imp {
                app.name = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default();
                app.intro.clear();
                app.pend = path;
                app.stack.clear();
                app.s = S::Dash;
                app.modal = true;
            } else if let Some(id) = app.current_id() {
                app.set_startup_exe(id, path);
                app.path = files_root();
                app.back();
            }
        }
    }
}
