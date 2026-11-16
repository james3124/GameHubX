use eframe::egui::*;
use gamehubx_input::{element_at, Engine, InputEvent, Shape, TouchPhase};

use crate::app::App;
use crate::theme::{CARD, DIM, W};
use crate::widgets::btn;

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "Touch Controls");
    let Some(profile) = app.touch_edit.clone() else {
        ui.horizontal(|ui| {
            ui.add_space(70.0);
            ui.label(
                RichText::new(crate::i18n::tr("No touch profile loaded"))
                    .size(24.0)
                    .color(DIM),
            );
        });
        return;
    };

    ui.horizontal(|ui| {
        ui.add_space(60.0);
        ui.label(RichText::new(&profile.name).size(26.0).strong());
        if btn(ui, "Switch profile", vec2(260.0, 56.0), CARD, W, 20.0).clicked() {
            app.cycle_touch_editor_profile();
        }
        if btn(
            ui,
            if app.touch_test_mode {
                "Edit layout"
            } else {
                "Test touch"
            },
            vec2(220.0, 56.0),
            CARD,
            W,
            20.0,
        )
        .clicked()
        {
            app.touch_test_mode = !app.touch_test_mode;
            app.touch_events.clear();
            app.touch_message = None;
            if app.touch_test_mode {
                if let Some(edit) = &app.touch_edit {
                    app.touch_engine = Some(Engine::new(edit.clone()));
                }
            }
        }
        if btn(ui, "Save as custom", vec2(280.0, 56.0), CARD, W, 20.0).clicked() {
            app.save_touch_profile();
        }
    });
    if let Some(message) = &app.touch_message {
        ui.horizontal(|ui| {
            ui.add_space(60.0);
            ui.label(RichText::new(message).size(18.0).color(DIM));
        });
    }

    let (response, painter) = ui.allocate_painter(vec2(1100.0, 340.0), Sense::click_and_drag());
    let rect = response.rect;
    painter.rect_filled(rect, 12.0, Color32::from_rgb(14, 20, 36));

    let profile = app.touch_edit.clone().unwrap_or(profile);
    for (index, element) in profile.elements.iter().enumerate() {
        let center = rect.min + vec2(element.x * rect.width(), element.y * rect.height());
        let radius = 34.0 * element.scale;
        let selected = app.touch_drag == Some(index);
        let stroke = Stroke::new(
            if selected { 3.0_f32 } else { 1.5_f32 },
            if selected {
                Color32::from_rgb(59, 130, 246)
            } else {
                Color32::from_white_alpha(120)
            },
        );
        match element.shape {
            Shape::Circle => {
                painter.circle_stroke(center, radius, stroke);
            }
            Shape::Square | Shape::Rect => {
                painter.rect_stroke(
                    Rect::from_center_size(center, vec2(radius * 2.0, radius * 2.0)),
                    8.0,
                    stroke,
                );
            }
        }
        let label: String = element
            .bindings
            .first()
            .cloned()
            .unwrap_or_default()
            .chars()
            .take(14)
            .collect();
        painter.text(
            center,
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.0),
            W,
        );
    }

    if app.touch_test_mode {
        let (pressed, released, down, delta, pointer) = ui.input(|input| {
            (
                input.pointer.any_pressed(),
                input.pointer.any_released(),
                input.pointer.primary_down(),
                input.pointer.delta(),
                input.pointer.interact_pos(),
            )
        });
        let mut events: Vec<InputEvent> = Vec::new();
        if let Some(pos) = pointer {
            if rect.contains(pos) {
                let x = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                let y = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
                if let Some(engine) = app.touch_engine.as_mut() {
                    if pressed {
                        events.extend(engine.on_touch(0, x, y, TouchPhase::Start));
                    } else if down && delta.length_sq() > 0.0 {
                        events.extend(engine.on_touch(0, x, y, TouchPhase::Move));
                    }
                    if released {
                        events.extend(engine.on_touch(0, x, y, TouchPhase::End));
                    }
                }
                painter.circle_filled(
                    pos,
                    16.0,
                    Color32::from_rgba_unmultiplied(59, 130, 246, 120),
                );
            }
        }
        if released && pointer.map(|pos| !rect.contains(pos)).unwrap_or(true) {
            if let Some(engine) = app.touch_engine.as_mut() {
                events.extend(engine.on_touch(0, 0.0, 0.0, TouchPhase::End));
            }
        }
        for event in events {
            app.touch_events.push(format!("{event:?}"));
            if app.touch_events.len() > 5 {
                app.touch_events.remove(0);
            }
        }
    } else {
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    let x = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let y = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
                    app.touch_drag = element_at(&profile, x, y);
                }
            }
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                if let Some(index) = app.touch_drag {
                    let x = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
                    let y = ((pos.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
                    if let Some(edit) = app.touch_edit.as_mut() {
                        if let Some(element) = edit.elements.get_mut(index) {
                            element.x = x;
                            element.y = y;
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            app.touch_drag = None;
            if let Some(edit) = &app.touch_edit {
                app.touch_engine = Some(Engine::new(edit.clone()));
            }
        }
    }

    ui.horizontal(|ui| {
        ui.add_space(60.0);
        ui.label(
            RichText::new(if app.touch_test_mode {
                "Test mode: touch the preview to emit mapped inputs."
            } else {
                "Edit mode: drag elements to move them."
            })
            .size(18.0)
            .color(DIM),
        );
    });
    for event in app.touch_events.iter().rev() {
        ui.horizontal(|ui| {
            ui.add_space(60.0);
            ui.label(RichText::new(event).size(16.0).monospace().color(DIM));
        });
    }
}
