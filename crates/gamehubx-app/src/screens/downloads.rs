use eframe::egui::*;
use gamehubx_download::{DownloadStatus, DownloadTask};
use gamehubx_platform::format_bytes;

use crate::app::App;
use crate::theme::{ACC, CARD, DIM, W};
use crate::widgets::btn;

#[derive(Clone, Copy)]
enum Action {
    Start(i64),
    Pause(i64),
    Retry(i64),
    Remove(i64),
    ClearCompleted,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.refresh_downloads();
    app.bar(ui, "");
    app.tabs(ui, false);
    ui.add_space(20.0);

    let tasks = app.downloads.clone();
    let queued: Vec<&DownloadTask> = tasks
        .iter()
        .filter(|task| task.status == DownloadStatus::Queued)
        .collect();
    let active: Vec<&DownloadTask> = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                DownloadStatus::Downloading | DownloadStatus::Paused
            )
        })
        .collect();
    let failed: Vec<&DownloadTask> = tasks
        .iter()
        .filter(|task| task.status == DownloadStatus::Failed)
        .collect();
    let completed: Vec<&DownloadTask> = tasks
        .iter()
        .filter(|task| task.status == DownloadStatus::Completed)
        .collect();

    let mut actions = Vec::new();
    ScrollArea::vertical().id_salt("downloads").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(70.0);
            ui.vertical(|ui| {
                heading(ui, "Download Soon", queued.len());
                if queued.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::tr("No Queued Tasks"))
                            .size(24.0)
                            .color(DIM),
                    );
                } else {
                    for task in &queued {
                        render_task(ui, task, "Start", 0, &mut actions);
                    }
                }

                if !active.is_empty() {
                    ui.add_space(16.0);
                    heading(ui, "Downloading", active.len());
                    for task in &active {
                        let action = if task.status == DownloadStatus::Downloading {
                            "Pause"
                        } else {
                            "Resume"
                        };
                        let speed = app.download_speeds.get(&task.id).copied().unwrap_or(0);
                        render_task(ui, task, action, speed, &mut actions);
                    }
                }

                if !failed.is_empty() {
                    ui.add_space(16.0);
                    heading(ui, "Failed", failed.len());
                    for task in &failed {
                        render_task(ui, task, "Retry", 0, &mut actions);
                    }
                }

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    heading(ui, "Completed", completed.len());
                    if !completed.is_empty()
                        && btn(ui, "Clear completed", vec2(240.0, 56.0), CARD, DIM, 20.0).clicked()
                    {
                        actions.push(Action::ClearCompleted);
                    }
                });
                if completed.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::tr("No completed downloads"))
                            .size(24.0)
                            .color(DIM),
                    );
                } else {
                    for task in &completed {
                        render_task(ui, task, "", 0, &mut actions);
                    }
                }

                if let Some(message) = &app.download_message {
                    ui.add_space(12.0);
                    ui.label(RichText::new(message).size(18.0).color(DIM));
                }
                ui.add_space(30.0);
            });
        });
    });

    for action in actions {
        match action {
            Action::Start(id) | Action::Retry(id) => app.resume_download(id),
            Action::Pause(id) => app.pause_download(id),
            Action::Remove(id) => app.remove_download(id),
            Action::ClearCompleted => app.clear_completed_downloads(),
        }
    }
}

fn heading(ui: &mut Ui, title: &str, count: usize) {
    ui.label(
        RichText::new(format!("{}  ({count})", crate::i18n::tr(title)))
            .size(28.0)
            .strong(),
    );
}

fn render_task(
    ui: &mut Ui,
    task: &DownloadTask,
    action: &str,
    speed: u64,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        let subtitle = subtitle(task, speed);
        let (rect, response) = ui.allocate_exact_size(vec2(700.0, 96.0), Sense::click());
        {
            let painter = ui.painter();
            painter.rect_filled(rect, Rounding::same(14.0), CARD);
            if response.hovered() || response.has_focus() {
                painter.rect_stroke(
                    rect,
                    Rounding::same(14.0),
                    Stroke::new(1.5_f32, Color32::from_white_alpha(160)),
                );
            }
            painter.text(
                rect.left_top() + vec2(14.0, 24.0),
                Align2::LEFT_CENTER,
                &task.title,
                FontId::proportional(24.0),
                W,
            );
            painter.text(
                rect.left_top() + vec2(14.0, 58.0),
                Align2::LEFT_CENTER,
                &subtitle,
                FontId::proportional(18.0),
                DIM,
            );
            progress_bar(painter, rect, task);
        }

        let clicked =
            !action.is_empty() && btn(ui, action, vec2(150.0, 64.0), CARD, W, 20.0).clicked();
        if clicked {
            actions.push(match task.status {
                DownloadStatus::Downloading => Action::Pause(task.id),
                DownloadStatus::Failed => Action::Retry(task.id),
                _ => Action::Start(task.id),
            });
        }
        if btn(ui, "Remove", vec2(130.0, 64.0), CARD, DIM, 18.0).clicked() {
            actions.push(Action::Remove(task.id));
        }
    });
}

fn progress_bar(painter: &Painter, rect: Rect, task: &DownloadTask) {
    if task.bytes_total == 0 {
        return;
    }
    let done = task.bytes_done.min(task.bytes_total);
    let width = (done as f32 / task.bytes_total as f32).clamp(0.0, 1.0);
    let track = Rect::from_min_max(
        pos2(rect.left() + 14.0, rect.bottom() - 14.0),
        pos2(rect.right() - 14.0, rect.bottom() - 8.0),
    );
    painter.rect_filled(track, 3.0, Color32::from_rgb(34, 42, 62));
    if width > 0.0 {
        let fill = Rect::from_min_max(
            track.min,
            pos2(track.left() + track.width() * width, track.bottom()),
        );
        painter.rect_filled(fill, 3.0, ACC);
    }
}

fn subtitle(task: &DownloadTask, speed: u64) -> String {
    match task.status {
        DownloadStatus::Queued => {
            if task.bytes_total > 0 {
                format!(
                    "{} · {}",
                    crate::i18n::tr("Waiting"),
                    format_bytes(task.bytes_total)
                )
            } else {
                crate::i18n::tr("Waiting").to_string()
            }
        }
        DownloadStatus::Downloading => {
            let mut text = format!(
                "{} / {} · {}%",
                format_bytes(task.bytes_done),
                total_text(task),
                percent(task)
            );
            if speed > 0 {
                text.push_str(&format!(" · {}/s", format_bytes(speed)));
            }
            text
        }
        DownloadStatus::Paused => format!(
            "{} · {} / {} · {}%",
            crate::i18n::tr("Paused"),
            format_bytes(task.bytes_done),
            total_text(task),
            percent(task)
        ),
        DownloadStatus::Failed => {
            let error = task.error.as_deref().unwrap_or("network error");
            format!("{}: {error}", crate::i18n::tr("Failed"))
        }
        DownloadStatus::Completed => format!(
            "{}  {}:{}",
            format_bytes(task.bytes_total.max(task.bytes_done)),
            crate::i18n::tr("Completed At"),
            completed_text(task.completed_at)
        ),
    }
}

fn total_text(task: &DownloadTask) -> String {
    if task.bytes_total > 0 {
        format_bytes(task.bytes_total)
    } else {
        "?".to_string()
    }
}

fn percent(task: &DownloadTask) -> u32 {
    if task.bytes_total == 0 {
        return 0;
    }
    ((task.bytes_done.min(task.bytes_total) * 100) / task.bytes_total) as u32
}

fn completed_text(timestamp: Option<i64>) -> String {
    timestamp
        .and_then(|value| chrono::DateTime::from_timestamp(value, 0))
        .map(|value| {
            value
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}
