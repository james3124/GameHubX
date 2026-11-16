use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::*;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.load_current_settings();
    app.bar(ui, "Game Settings");
    let exe = app.current_exe();
    ui.horizontal_top(|ui| {
        ui.add_space(46.0);
        ui.vertical(|ui| {
            for (i, name) in [
                "General",
                "Compatibility",
                "Backup / Restore",
                "Component",
                "Touch Controls",
                "Developer Options",
            ]
            .iter()
            .enumerate()
            {
                let response = btn(
                    ui,
                    name,
                    vec2(440.0, 88.0),
                    if app.pane == i { CARD } else { CLEAR },
                    if app.pane == i { W } else { DIM },
                    26.0,
                );
                if response.clicked() || response.has_focus() {
                    app.pane = i;
                }
            }
        });
        ui.vertical(|ui| {
            ScrollArea::vertical()
                .id_salt("game_settings")
                .show(ui, |ui| match app.pane {
                    0 => {
                        row(
                            ui,
                            "Enter Container Desktop",
                            "Launch Wine container desktop environment to operate files and programs",
                            "›",
                        );
                        if row(
                            ui,
                            "Startup File Path",
                            "Select the .exe file path of the game or program to run",
                            &short(&exe),
                        )
                        .clicked()
                        {
                            app.open_files(false);
                        }
                        row(
                            ui,
                            "Language",
                            "Set container system language and region",
                            "en_US ›",
                        );
                        if row(
                            ui,
                            "Game Resolution",
                            "Set screen resolution when programs are running",
                            &format!("{} ›", app.game_settings.resolution_label()),
                        )
                        .clicked()
                        {
                            app.cycle_resolution();
                        }
                        if row(
                            ui,
                            "Frame Rate Limit",
                            "Cap the game's frame rate",
                            &format!("{} FPS ›", app.game_settings.fps_cap),
                        )
                        .clicked()
                        {
                            app.cycle_fps();
                        }
                        tog(
                            ui,
                            "Enable MangoHUD",
                            "Enable MangoHUD overlay as a more professional performance dashboard",
                            &mut app.tg[0],
                        );
                        row(
                            ui,
                            "DirectX HUD",
                            "Control performance monitoring HUD display, showing FPS and debug info",
                            "Disable ›",
                        );
                        tog(
                            ui,
                            "Disable Window Manager",
                            "Windows will no longer be decorated and cannot be repositioned",
                            &mut app.tg[1],
                        );
                        row(
                            ui,
                            "Environment Variables",
                            "Configure container environment variables",
                            "›",
                        );
                        row(ui, "Command Line", "Add program launch parameters", "");
                        tog(
                            ui,
                            "Auto-display keyboard when input needed",
                            "",
                            &mut app.tg[2],
                        );
                        tog(
                            ui,
                            "Disable Graphics Enhancement Plugin",
                            "Disable if game has issues",
                            &mut app.tg[3],
                        );
                        row(
                            ui,
                            "Reset Game Data",
                            "Clear all container data and restore to initial state",
                            "›",
                        );
                        if btn(ui, "Uninstall game", vec2(1000.0, 82.0), CARD, W, 26.0).clicked() {
                            if let Some(id) = app.current_id() {
                                app.remove_game(id);
                            }
                            app.stack.clear();
                            app.s = S::Dash;
                        }
                        ui.add_space(80.0);
                    }
                    1 => {
                        row(
                            ui,
                            "Compatibility Layer",
                            "Select Wine version",
                            "proton10.0-arm64x-2 ›",
                        );
                        if row(
                            ui,
                            "DXVK Version",
                            "Select the Direct3D translation layer",
                            &format!("{} ›", app.game_settings.dxvk_version),
                        )
                        .clicked()
                        {
                            app.cycle_dxvk();
                        }
                        row(
                            ui,
                            "Translation Params",
                            "x86-to-ARM64 translation parameters",
                            "Game Presets ›",
                        );
                        row(
                            ui,
                            "DInput Library",
                            "dinput.dll and dinput8.dll priority",
                            "Prefer Native ›",
                        );
                        row(ui, "GPU Driver", "Select graphics driver type", "System Driver ›");
                        row(
                            ui,
                            "Surface Format",
                            "Select surface color format for rendering",
                            "RGBA8 ›",
                        );
                        if row(
                            ui,
                            "Audio Driver",
                            "Select audio output driver",
                            &format!("{} ›", app.game_settings.audio_backend),
                        )
                        .clicked()
                        {
                            app.cycle_audio();
                        }
                    }
                    4 => {
                        if row(
                            ui,
                            "Touch Profile",
                            "On-screen control layout (ICP profiles)",
                            &format!("{} ›", app.game_settings.touch_profile),
                        )
                        .clicked()
                        {
                            app.cycle_touch_profile();
                        }
                        if btn(
                            ui,
                            "Open Touch Controls",
                            vec2(380.0, 72.0),
                            CARD,
                            W,
                            24.0,
                        )
                        .clicked()
                        {
                            app.open_touch_editor();
                        }
                        ui.label(
                            RichText::new(
                                "Preview, drag-edit, save custom ICP profiles, and test the mapping engine. Game injection awaits the P3 display bridge.",
                            )
                            .size(18.0)
                            .color(DIM),
                        );
                    }
                    5 => {
                        let memory = if app.game_settings.memory_limit_mb == 0 {
                            "Unlimited ›".to_string()
                        } else {
                            format!("{} MB ›", app.game_settings.memory_limit_mb)
                        };
                        if row(
                            ui,
                            "Memory Limit",
                            "Address-space limit for the Wine session (RLIMIT_AS)",
                            &memory,
                        )
                        .clicked()
                        {
                            app.cycle_memory_limit();
                        }
                        let affinity = if app.game_settings.cpu_affinity_big {
                            "Big cores ›"
                        } else {
                            "All cores ›"
                        };
                        if row(
                            ui,
                            "CPU Affinity",
                            "Restrict the session to the fastest cores",
                            affinity,
                        )
                        .clicked()
                        {
                            app.cycle_cpu_affinity();
                        }
                        ui.label(
                            RichText::new(
                                "Applied at session spawn; big cores use cpufreq max frequencies.",
                            )
                            .size(18.0)
                            .color(DIM),
                        );
                    }
                    _ => {
                        ui.label(RichText::new("—").color(DIM));
                    }
                });
        });
    });
}
