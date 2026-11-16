use eframe::egui::*;
use gamehubx_core::Game;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{btn, dim};

pub(crate) fn layers(app: &mut App, ctx: &Context) {
    if app.drawer || app.modal || app.popup {
        dim(ctx);
    }

    if app.drawer {
        let mut dest = None;
        Window::new("drawer")
            .title_bar(false)
            .frame(frame())
            .anchor(Align2::LEFT_TOP, [0.0, 0.0])
            .fixed_size([620.0, 660.0])
            .show(ctx, |ui| {
                ui.label(RichText::new("🎮  GameHubX").size(24.0));
                ui.add_space(20.0);
                for (name, screen) in [
                    ("Home", S::Dash),
                    ("Game", S::Pc),
                    ("Download", S::Dl),
                    ("Setting", S::App),
                ] {
                    let selected = app.s == screen;
                    if btn(
                        ui,
                        name,
                        vec2(520.0, 90.0),
                        if selected { CARD } else { CLEAR },
                        W,
                        28.0,
                    )
                    .clicked()
                    {
                        dest = Some(screen);
                    }
                }
            });
        if let Some(screen) = dest {
            app.drawer = false;
            if screen != app.s {
                app.go(screen);
            }
        }
    }

    if app.modal {
        Window::new("import")
            .title_bar(false)
            .frame(frame())
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([700.0, 400.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(crate::i18n::tr("Upload cover image")).size(30.0));
                    ui.add_space(16.0);
                    ui.add(
                        TextEdit::singleline(&mut app.name)
                            .desired_width(640.0)
                            .font(FontId::proportional(26.0)),
                    );
                    ui.add(
                        TextEdit::singleline(&mut app.intro)
                            .hint_text(crate::i18n::tr("Enter introduction"))
                            .desired_width(640.0)
                            .font(FontId::proportional(26.0)),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if btn(ui, "Cancel", vec2(310.0, 80.0), CARD, W, 26.0).clicked() {
                            app.reset_import();
                            app.modal = false;
                        }
                        if btn(ui, "Confirm", vec2(310.0, 80.0), W, Color32::BLACK, 26.0).clicked()
                        {
                            let fallback = app
                                .pend
                                .file_stem()
                                .map(|stem| stem.to_string_lossy().to_string())
                                .filter(|stem| !stem.is_empty())
                                .unwrap_or_else(|| "Untitled".to_string());
                            let label = if app.name.trim().is_empty() {
                                fallback
                            } else {
                                app.name.trim().to_string()
                            };
                            let game = Game::new_pc_exe(label, app.intro.trim(), app.pend.clone());
                            app.add_game(game);
                            app.reset_import();
                            app.modal = false;
                        }
                    });
                });
            });
    }

    if app.popup {
        Window::new("popup")
            .title_bar(false)
            .frame(frame())
            .fixed_pos([628.0, 300.0])
            .show(ctx, |ui| {
                if btn(ui, "PC Game Settings", vec2(340.0, 70.0), CARD, W, 26.0).clicked() {
                    app.popup = false;
                    app.go(S::Set);
                }
                if btn(
                    ui,
                    "Add to Home screen",
                    vec2(340.0, 70.0),
                    CLEAR,
                    DIM,
                    26.0,
                )
                .clicked()
                {
                    app.popup = false;
                }
                if btn(ui, "Uninstall game", vec2(340.0, 70.0), CLEAR, DIM, 26.0).clicked() {
                    if let Some(id) = app.current_id() {
                        app.remove_game(id);
                    }
                    app.popup = false;
                    app.sel = 0;
                }
            });
    }
}
