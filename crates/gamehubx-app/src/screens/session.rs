use eframe::egui::*;
use gamehubx_supervisor::read_pss;

use crate::app::{App, S};
use crate::theme::{CARD, DIM, W};
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Game Session");

    let mut finished = None;
    let mut pid = None;
    let mut pss = None;
    if let Some(session) = app.session.as_mut() {
        pid = Some(session.pid());
        match session.try_wait() {
            Ok(Some(status)) => finished = Some(status),
            Ok(None) => pss = read_pss(session.pid()),
            Err(error) => {
                app.session_error = Some(format!("Session poll failed: {error}"));
            }
        }
    }
    if let Some(status) = finished {
        app.session = None;
        app.stop_audio();
        app.session_status = if status.success() {
            "Exited normally".to_string()
        } else {
            format!("Exited ({status})")
        };
        app.session_pss = None;
        pid = None;
    } else if pss.is_some() {
        app.session_pss = pss;
    }

    ui.add_space(40.0);
    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(&app.session_game).size(40.0).strong());

            if let Some(error) = &app.session_error {
                ui.label(
                    RichText::new(error)
                        .size(24.0)
                        .color(Color32::from_rgb(245, 120, 120)),
                );
            }

            if !app.session_missing.is_empty() {
                ui.label(
                    RichText::new(crate::i18n::tr(
                        "Runtime pack is missing these files:",
                    ))
                        .size(24.0)
                        .color(DIM),
                );
                for path in &app.session_missing {
                    ui.label(
                        RichText::new(path.display().to_string())
                            .size(18.0)
                            .monospace()
                            .color(DIM),
                    );
                }
                ui.label(
                    RichText::new(
                        "Place an open runtime pack under the app data directory, then press Play Now again.",
                    )
                    .size(18.0)
                    .color(DIM),
                );
            }

            if let Some(pid) = pid {
                ui.label(
                    RichText::new(format!("PID {pid}  •  {}", app.session_status)).size(24.0),
                );
            } else {
                ui.label(RichText::new(&app.session_status).size(24.0));
            }

            if let Some(pss) = app.session_pss {
                ui.label(
                    RichText::new(format!(
                        "PSS {} KB  (anon {} / file {} / shmem {})",
                        pss.total_kb, pss.anon_kb, pss.file_kb, pss.shmem_kb
                    ))
                    .size(20.0)
                    .color(DIM),
                );
            }

            ui.add_space(24.0);
            ui.horizontal(|ui| {
                if app.session.is_some()
                    && btn(ui, "Stop session", vec2(300.0, 72.0), CARD, W, 24.0).clicked()
                {
                    app.stop_session();
                }
                if !app.session_missing.is_empty()
                    && btn(ui, "Open Runtime", vec2(300.0, 72.0), CARD, W, 24.0).clicked()
                {
                    app.go(S::Runtime);
                }
                if btn(ui, "Winlator Setup", vec2(300.0, 72.0), CARD, W, 24.0).clicked() {
                    app.open_winlator_setup();
                }
                if btn(ui, "Back to game", vec2(300.0, 72.0), CARD, W, 24.0).clicked() {
                    app.back();
                }
            });
        });
    });
}
