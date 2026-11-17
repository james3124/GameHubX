use eframe::egui::*;

use crate::app::{App, S};
use crate::theme::*;
use crate::widgets::{btn, dim};

fn menu_row(ui: &mut Ui, glyph: &str, name: &str, detail: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(460.0, 72.0), Sense::click());
    crate::focus::register(response.id);
    ui.painter().rect_filled(
        rect,
        Rounding::same(10.0),
        if selected { CARD } else { CLEAR },
    );
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(pos2(rect.left(), rect.center().y - 14.0), vec2(4.0, 28.0)),
            2.0,
            ACC,
        );
    }
    ui.painter().text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(24.0),
        if selected { W } else { DIM },
    );
    ui.painter().text(
        pos2(rect.left() + 54.0, rect.center().y - 10.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr(name),
        FontId::proportional(21.0),
        W,
    );
    ui.painter().text(
        pos2(rect.left() + 54.0, rect.center().y + 15.0),
        Align2::LEFT_CENTER,
        crate::i18n::tr(detail),
        FontId::proportional(14.0),
        DIM,
    );
    if response.hovered() || response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            Rounding::same(10.0),
            Stroke::new(1.5_f32, Color32::from_white_alpha(180)),
        );
    }
    response
}

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
            .fixed_size([520.0, 660.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (avatar, painter) = ui.allocate_painter(vec2(46.0, 46.0), Sense::hover());
                    painter.circle_filled(avatar.rect.center(), 23.0, ACC);
                    painter.text(
                        avatar.rect.center(),
                        Align2::CENTER_CENTER,
                        "GX",
                        FontId::proportional(16.0),
                        W,
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new("GameHubX").size(22.0).strong());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("●").size(12.0).color(GREEN));
                            ui.label(
                                RichText::new(crate::i18n::tr("Local library"))
                                    .size(15.0)
                                    .color(DIM),
                            );
                        });
                    });
                });
                ui.add_space(22.0);

                for (glyph, name, detail, screen) in [
                    ("⌂", "Home", "Library overview", S::Dash),
                    ("▦", "Game", "PC game library", S::Pc),
                    ("↓", "Download", "Queue and history", S::Dl),
                    ("⚙", "Setting", "Launcher settings", S::App),
                ] {
                    let selected = app.s == screen;
                    let response = menu_row(ui, glyph, name, detail, selected);
                    if response.clicked() {
                        dest = Some(screen);
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(crate::i18n::tr("B  Back"))
                            .size(15.0)
                            .color(DIM),
                    );
                });
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
                            let intro = app.intro.trim().to_string();
                            app.import_pending_with_cover(label, intro);
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
                if btn(ui, "Add to Home screen", vec2(340.0, 70.0), CLEAR, W, 26.0).clicked() {
                    app.pin_current_game();
                    app.popup = false;
                }
                if btn(ui, "Set cover image", vec2(340.0, 70.0), CLEAR, W, 26.0).clicked() {
                    app.open_cover_picker();
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

    if let Some(kind) = app.text_editor {
        let title = match kind {
            crate::app::TextEditorKind::CommandLine => "Launch parameters",
            crate::app::TextEditorKind::WineDebug => "Wine debug params (WINEDEBUG)",
            crate::app::TextEditorKind::EnvVars => "Environment variables (KEY=VALUE per line)",
        };
        let multiline = kind == crate::app::TextEditorKind::EnvVars;
        Window::new("text_editor")
            .title_bar(false)
            .frame(frame())
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([760.0, 480.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(RichText::new(title).size(28.0).strong());
                    ui.add_space(16.0);
                    let editor = if multiline {
                        TextEdit::multiline(&mut app.text_buffer)
                            .desired_width(700.0)
                            .desired_rows(12)
                            .font(FontId::proportional(18.0))
                    } else {
                        TextEdit::singleline(&mut app.text_buffer)
                            .desired_width(700.0)
                            .font(FontId::proportional(22.0))
                    };
                    ui.add(editor);
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if btn(ui, "Cancel", vec2(320.0, 72.0), CARD, W, 24.0).clicked() {
                            app.text_editor = None;
                            app.text_buffer.clear();
                        }
                        if btn(ui, "Save", vec2(320.0, 72.0), W, Color32::BLACK, 24.0).clicked() {
                            app.commit_text_editor();
                        }
                    });
                });
            });
    }
}
