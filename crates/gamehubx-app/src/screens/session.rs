use std::path::Path;

use eframe::egui::*;
use gamehubx_supervisor::read_pss;

use crate::app::{App, S};
use crate::theme::{ACC, CARD, DIM, W};
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
    if let Some(sample) = pss {
        crate::screens::hud::push_pss(sample.total_kb);
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

            // Richer readouts: audio helper PID, container count and a small
            // PSS trend sparkline. All values come from existing app state or
            // bounded filesystem reads — no new App fields are required.
            if let Some(audio_pid) = app.audio_session.as_ref().map(|session| session.pid()) {
                ui.label(
                    RichText::new(format!("{} {audio_pid}", crate::i18n::tr("Audio PID")))
                        .size(20.0)
                        .color(DIM),
                );
            }
            ui.label(
                RichText::new(format!(
                    "{} {}",
                    crate::i18n::tr("Containers"),
                    container_count()
                ))
                .size(20.0)
                .color(DIM),
            );

            let history = crate::screens::hud::pss_samples();
            if history.len() >= 2 {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(crate::i18n::tr("PSS history"))
                        .size(18.0)
                        .color(DIM),
                );
                let (rect, _) = ui.allocate_exact_size(vec2(620.0, 72.0), Sense::hover());
                ui.painter().rect_filled(rect, Rounding::same(10.0), CARD);
                let peak = history.iter().copied().max().unwrap_or(1) as f32;
                let values: Vec<f32> = history.iter().map(|&value| value as f32).collect();
                let points = crate::screens::hud::line_points(&values, rect.shrink(6.0), peak * 1.1);
                if points.len() >= 2 {
                    ui.painter().add(Shape::line(points, Stroke::new(2.0_f32, ACC)));
                }
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

/// Count immediate sub-directories of `path` (used for the container readout).
fn count_dirs(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

/// Number of per-game containers currently present in the app data dir.
pub(crate) fn container_count() -> usize {
    count_dirs(&gamehubx_platform::data_dir().join("containers"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn counts_only_directories() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base =
            std::env::temp_dir().join(format!("ghx-containers-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a")).unwrap();
        std::fs::create_dir_all(base.join("b")).unwrap();
        std::fs::write(base.join("note.txt"), b"x").unwrap();
        assert_eq!(count_dirs(&base), 2);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
