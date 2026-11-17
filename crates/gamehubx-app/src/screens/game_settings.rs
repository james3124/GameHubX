use eframe::egui::*;
use gamehubx_core::{
    COMPAT_LAYERS, CONFIG_PRESETS, CONTAINER_LANGUAGES, DINPUT_OPTIONS, DIRECTX_HUD_OPTIONS,
    GPU_DRIVER_OPTIONS, SURFACE_FORMAT_OPTIONS, TRANSLATION_PRESETS, VKD3D_OPTIONS,
};

use crate::app::{App, BoolField, ExtendedField, TextEditorKind, S};
use crate::theme::*;
use crate::widgets::*;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.load_current_settings();
    app.bar(ui, "Game Settings");
    let exe = app.current_exe();
    let gs = &app.game_settings;
    let container_language = gs.container_language.clone();
    let directx_hud = gs.directx_hud.clone();
    let compat_layer = gs.compat_layer.clone();
    let translation_params = gs.translation_params.clone();
    let dinput_library = gs.dinput_library.clone();
    let gpu_driver = gs.gpu_driver.clone();
    let surface_format = gs.surface_format.clone();
    let vkd3d_version = gs.vkd3d_version.clone();
    let config_preset = gs.config_preset.clone();
    let mangohud = gs.mangohud;
    let disable_wm = gs.disable_wm;
    let auto_keyboard = gs.auto_keyboard;
    let disable_gfx_plugin = gs.disable_gfx_plugin;
    let log_server = gs.log_server;
    let rts_gestures = gs.rts_gestures;
    let core_limit = gs.cpu_core_limit;
    let memory = if gs.memory_limit_mb == 0 {
        "Unlimited ›".to_string()
    } else {
        format!("{} MB ›", gs.memory_limit_mb)
    };
    let affinity = if gs.cpu_affinity_big {
        "Big cores ›"
    } else {
        "All cores ›"
    };
    let core_label = if core_limit == 0 {
        "All cores ›".to_string()
    } else {
        format!("{core_limit} cores ›")
    };
    let resolution = gs.resolution_label();
    let fps = gs.fps_cap;
    let dxvk = gs.dxvk_version.clone();
    let audio = gs.audio_backend.clone();
    let touch_profile = gs.touch_profile.clone();
    let env_count = gs.env_vars.len();
    let command_line = gs.command_line.clone();
    let wine_debug = gs.wine_debug_params.clone();

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
                if response.clicked() {
                    app.pane = i;
                }
            }
        });
        ui.vertical(|ui| {
            ScrollArea::vertical()
                .id_salt("game_settings")
                .show(ui, |ui| match app.pane {
                    0 => {
                        if row(
                            ui,
                            "Enter Container Desktop",
                            "Launch Wine container desktop environment to operate files and programs",
                            "›",
                        )
                        .clicked()
                        {
                            app.launch_container_desktop();
                        }
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
                        if row(
                            ui,
                            "Language",
                            "Set container system language and region",
                            &format!("{container_language} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(CONTAINER_LANGUAGES, ExtendedField::ContainerLanguage);
                        }
                        if row(
                            ui,
                            "Game Resolution",
                            "Set screen resolution when programs are running",
                            &format!("{resolution} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_resolution();
                        }
                        if row(
                            ui,
                            "Frame Rate Limit",
                            "Cap the game's frame rate",
                            &format!("{fps} FPS ›"),
                        )
                        .clicked()
                        {
                            app.cycle_fps();
                        }
                        if tog(
                            ui,
                            "Enable MangoHUD",
                            "Enable MangoHUD overlay as a more professional performance dashboard",
                            mangohud,
                        ) {
                            app.toggle_extended(BoolField::Mangohud);
                        }
                        if row(
                            ui,
                            "DirectX HUD",
                            "Control performance monitoring HUD display, showing FPS and debug info",
                            &format!("{directx_hud} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(DIRECTX_HUD_OPTIONS, ExtendedField::DirectxHud);
                        }
                        if tog(
                            ui,
                            "Disable Window Manager",
                            "Windows will no longer be decorated and cannot be repositioned",
                            disable_wm,
                        ) {
                            app.toggle_extended(BoolField::DisableWm);
                        }
                        if row(
                            ui,
                            "Environment Variables",
                            "Configure container environment variables",
                            &format!("{env_count} set ›"),
                        )
                        .clicked()
                        {
                            app.open_env_editor();
                        }
                        if row(ui, "Command Line", "Add program launch parameters", &short(&command_line))
                            .clicked()
                        {
                            app.open_text_editor(TextEditorKind::CommandLine);
                        }
                        if tog(
                            ui,
                            "Auto-display keyboard when input needed",
                            "",
                            auto_keyboard,
                        ) {
                            app.toggle_extended(BoolField::AutoKeyboard);
                        }
                        if tog(
                            ui,
                            "Disable Graphics Enhancement Plugin",
                            "Disable if game has issues",
                            disable_gfx_plugin,
                        ) {
                            app.toggle_extended(BoolField::DisableGfxPlugin);
                        }
                        if row(
                            ui,
                            "Reset Game Data",
                            "Clear all container data and restore to initial state",
                            "›",
                        )
                        .clicked()
                        {
                            app.reset_current_container();
                        }
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
                        if row(
                            ui,
                            "Compatibility Layer",
                            "Select Wine version",
                            &format!("{compat_layer} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(COMPAT_LAYERS, ExtendedField::CompatLayer);
                        }
                        if row(
                            ui,
                            "DXVK Version",
                            "Select the Direct3D translation layer",
                            &format!("{dxvk} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_dxvk();
                        }
                        if row(
                            ui,
                            "VKD3D Version",
                            "Select the Direct3D 12 translation layer",
                            &format!("{vkd3d_version} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(VKD3D_OPTIONS, ExtendedField::Vkd3d);
                        }
                        if row(
                            ui,
                            "Translation Params",
                            "x86-to-ARM64 translation parameters",
                            &format!("{translation_params} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(TRANSLATION_PRESETS, ExtendedField::TranslationParams);
                        }
                        if row(
                            ui,
                            "DInput Library",
                            "dinput.dll and dinput8.dll priority",
                            &format!("{dinput_library} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(DINPUT_OPTIONS, ExtendedField::DinputLibrary);
                        }
                        if row(
                            ui,
                            "GPU Driver",
                            "Select graphics driver type",
                            &format!("{gpu_driver} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(GPU_DRIVER_OPTIONS, ExtendedField::GpuDriver);
                        }
                        if row(
                            ui,
                            "Surface Format",
                            "Select surface color format for rendering",
                            &format!("{surface_format} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(SURFACE_FORMAT_OPTIONS, ExtendedField::SurfaceFormat);
                        }
                        if row(
                            ui,
                            "Audio Driver",
                            "Select audio output driver",
                            &format!("{audio} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_audio();
                        }
                        if row(
                            ui,
                            "Recommended Config",
                            "Apply a known-good preset bundle",
                            &format!("{config_preset} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_extended(CONFIG_PRESETS, ExtendedField::ConfigPreset);
                        }
                    }
                    2 => {
                        if row(
                            ui,
                            "Backup container data",
                            "Write a .tar.gz archive of this game's container to the app data dir",
                            "›",
                        )
                        .clicked()
                        {
                            app.backup_current_container();
                        }
                        if row(
                            ui,
                            "Restore container data",
                            "Restore from a selected .tar.gz archive (pick the file in Files first)",
                            "›",
                        )
                        .clicked()
                        {
                            app.restore_current_container();
                        }
                        if row(
                            ui,
                            "Choose backup archive",
                            "Open the file browser to select a .tar.gz backup",
                            "›",
                        )
                        .clicked()
                        {
                            app.open_backup_picker();
                        }
                        if let Some(notice) = &app.settings_notice {
                            ui.add_space(10.0);
                            ui.label(RichText::new(notice).size(18.0).color(DIM));
                        }
                    }
                    3 => {
                        let names = app.runtime_component_names();
                        ui.label(
                            RichText::new("Installed runtime components (verified via manifest):")
                                .size(20.0)
                                .color(DIM),
                        );
                        for name in &names {
                            row(ui, name, "Component", "installed ›");
                        }
                        if names.is_empty() {
                            ui.label(RichText::new("No components installed yet").size(20.0).color(DIM));
                        }
                        if btn(ui, "Open Runtime Pack", vec2(420.0, 72.0), CARD, W, 24.0).clicked() {
                            app.go(S::Runtime);
                        }
                    }
                    4 => {
                        if row(
                            ui,
                            "Touch Profile",
                            "On-screen control layout (ICP profiles)",
                            &format!("{touch_profile} ›"),
                        )
                        .clicked()
                        {
                            app.cycle_touch_profile();
                        }
                        if tog(
                            ui,
                            "RTS gesture controls",
                            "Tap/drag/pinch gestures mapped to game actions",
                            rts_gestures,
                        ) {
                            app.toggle_extended(BoolField::RtsGestures);
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
                                "Preview, drag-edit, save custom ICP profiles, and test the mapping engine. Live injection awaits the display bridge.",
                            )
                            .size(18.0)
                            .color(DIM),
                        );
                    }
                    5 => {
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
                        if row(
                            ui,
                            "CPU Core Limit",
                            "Maximum number of CPU cores for the session",
                            &core_label,
                        )
                        .clicked()
                        {
                            app.cycle_cpu_core_limit();
                        }
                        if tog(
                            ui,
                            "Enable Log Server",
                            "Enable the in-session HTTP log server",
                            log_server,
                        ) {
                            app.toggle_extended(BoolField::LogServer);
                        }
                        if row(
                            ui,
                            "Wine Debug Params",
                            "WINEDEBUG value (e.g. -all, fixme-all)",
                            &short(&wine_debug),
                        )
                        .clicked()
                        {
                            app.open_text_editor(TextEditorKind::WineDebug);
                        }
                        ui.label(
                            RichText::new(
                                "Applied at session spawn; big cores use cpufreq max frequencies.",
                            )
                            .size(18.0)
                            .color(DIM),
                        );
                    }
                    _ => {}
                });
        });
    });
}
