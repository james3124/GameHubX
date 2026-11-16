use eframe::egui::*;
use gamehubx_platform::{battery_level, format_bytes};
use gamehubx_runtime::RuntimeLayout;
use gamehubx_supervisor::read_pss;

use crate::app::App;
use crate::theme::{CARD, DIM, W};
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Diagnostics");

    let app_pss = read_pss(std::process::id());
    let session_pid = app.session.as_ref().map(|session| session.pid());
    let session_pss = session_pid.and_then(read_pss);
    let audio_pid = app.audio_session.as_ref().map(|audio| audio.pid());
    let missing = RuntimeLayout::new(app.runtime_root())
        .missing_components()
        .len();
    let focused = ui.ctx().input(|input| input.focused);
    let storage = app
        .storage
        .map(|stats| {
            format!(
                "{} used / {} total",
                format_bytes(stats.used()),
                format_bytes(stats.total)
            )
        })
        .unwrap_or_else(|| "n/a".to_string());

    ui.horizontal(|ui| {
        ui.add_space(70.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Memory")).size(30.0).strong());
            ui.label(
                RichText::new(
                    app_pss
                        .map(|sample| {
                            format!(
                                "App PSS {} KB (anon {} / file {} / shmem {})",
                                sample.total_kb,
                                sample.anon_kb,
                                sample.file_kb,
                                sample.shmem_kb
                            )
                        })
                        .unwrap_or_else(|| "App PSS unavailable".to_string()),
                )
                .size(20.0)
                .color(DIM),
            );
            ui.label(
                RichText::new(match session_pid {
                    Some(pid) => format!(
                        "Session PID {pid} — {}",
                        session_pss
                            .map(|sample| format!("{} KB PSS", sample.total_kb))
                            .unwrap_or_else(|| "PSS unavailable".to_string())
                    ),
                    None => "Session: not running".to_string(),
                })
                .size(20.0)
                .color(DIM),
            );
            ui.label(
                RichText::new(match audio_pid {
                    Some(pid) => format!("Audio PID {pid}"),
                    None => "Audio: not running".to_string(),
                })
                .size(20.0)
                .color(DIM),
            );

            ui.add_space(10.0);
            ui.label(
                RichText::new(format!(
                    "Battery: {}",
                    battery_level()
                        .map(|level| format!("{level}%"))
                        .unwrap_or_else(|| "--%".to_string())
                ))
                .size(20.0)
                .color(DIM),
            );
            ui.label(RichText::new(format!("Storage: {storage}")).size(20.0).color(DIM));
            ui.label(
                RichText::new(format!(
                    "Library: {} games  •  runtime missing: {missing}  •  files access: {}",
                    app.games.len(),
                    if app.files_access { "yes" } else { "no" }
                ))
                .size(20.0)
                .color(DIM),
            );
            ui.label(
                RichText::new(format!(
                    "Focused: {focused}  •  repaint: 15 s idle / 2 s session / 1 s diag"
                ))
                .size(20.0)
                .color(DIM),
            );

            ui.add_space(14.0);
            ui.label(
                RichText::new(
                    "Targets: ≤50% of GameHub Lite PSS; budgets and the device matrix are in docs/POWER_MEMORY.md.",
                )
                .size(18.0)
                .color(DIM),
            );
            ui.add_space(14.0);
            if btn(ui, "Back", vec2(220.0, 72.0), CARD, W, 24.0).clicked() {
                app.back();
            }
        });
    });
}
