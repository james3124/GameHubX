use std::path::Path;

use eframe::egui::*;
use gamehubx_platform::{files_root, format_bytes, scan_storage_categories};

use crate::app::App;
use crate::theme::{ACC, DIM};
use crate::widgets::{progress_bar, section};

const MAX_FOLDERS: usize = 24;
const MAX_ENTRIES_PER_FOLDER: u32 = 8_000;

/// One row of the per-folder breakdown table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FolderStat {
    pub(crate) name: String,
    pub(crate) bytes: u64,
    pub(crate) files: usize,
}

/// Bounded, symlink-safe recursive size/count for a single folder. Uses only
/// `std::fs` and caps the number of entries it visits.
fn walk_folder(path: &Path, visited: &mut u32) -> (u64, usize) {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        if *visited >= MAX_ENTRIES_PER_FOLDER {
            break;
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            if *visited >= MAX_ENTRIES_PER_FOLDER {
                break;
            }
            *visited += 1;
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    stack.push(entry.path());
                }
            } else if meta.is_file() {
                bytes = bytes.saturating_add(meta.len());
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// Summarise the immediate sub-folders of `root`, largest first. Hidden
/// folders and symlinks are ignored, and the total work is bounded.
pub(crate) fn folder_breakdown(root: &Path) -> Vec<FolderStat> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut result = Vec::new();
    let mut budget = MAX_ENTRIES_PER_FOLDER * 4;
    for entry in entries.flatten() {
        if result.len() >= MAX_FOLDERS || budget == 0 {
            break;
        }
        let meta = match std::fs::symlink_metadata(entry.path()) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let mut used = 0u32;
        let (bytes, files) = walk_folder(&entry.path(), &mut used);
        budget = budget.saturating_sub(used);
        result.push(FolderStat { name, bytes, files });
    }
    result.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    result
}

fn truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        format!(
            "…{}",
            text.chars().skip(count - max + 1).collect::<String>()
        )
    }
}

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    app.bar(ui, "");
    app.tabs(ui, true);
    ui.add_space(30.0);
    app.refresh_storage();

    let Some(stats) = app.storage else {
        ui.horizontal(|ui| {
            ui.add_space(100.0);
            ui.label(
                RichText::new(crate::i18n::tr("Storage statistics unavailable"))
                    .size(24.0)
                    .color(DIM),
            );
        });
        return;
    };

    let used = stats.used();
    let ratio = if stats.total == 0 {
        0.0
    } else {
        used as f32 / stats.total as f32
    };

    ui.horizontal(|ui| {
        ui.add_space(100.0);
        ui.vertical(|ui| {
            progress_bar(ui, ratio, 1080.0, 12.0, Color32::from_rgb(140, 147, 168));
            ui.horizontal(|ui| {
                for (color, text) in [
                    (ACC, format!("Used {}", format_bytes(used))),
                    (
                        Color32::from_rgb(140, 147, 168),
                        format!("Available {}", format_bytes(stats.available)),
                    ),
                    (
                        Color32::from_gray(70),
                        format!("Total {}", format_bytes(stats.total)),
                    ),
                ] {
                    ui.label(RichText::new("●").color(color));
                    ui.label(RichText::new(text).size(24.0));
                }
            });

            // Per-category breakdown mirroring the reference Game/Media/Other
            // legend (computed on demand with a bounded, symlink-safe scan).
            ui.add_space(18.0);
            let categories = scan_storage_categories(&files_root());
            section(ui, "Storage by category", |ui| {
                for (color, label, bytes) in [
                    (
                        Color32::from_rgb(59, 130, 246),
                        "Game",
                        categories.game_bytes,
                    ),
                    (
                        Color32::from_rgb(140, 147, 168),
                        "Media",
                        categories.media_bytes,
                    ),
                    (Color32::from_gray(70), "Other", categories.other_bytes),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").color(color));
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                crate::i18n::tr(label),
                                format_bytes(bytes)
                            ))
                            .size(22.0),
                        );
                    });
                }
            });

            // Per-folder table: each top-level folder under the files root with
            // its total size and file count.
            ui.add_space(18.0);
            section(ui, "Per-folder breakdown", |ui| {
                let folders = folder_breakdown(&files_root());
                if folders.is_empty() {
                    ui.label(
                        RichText::new(crate::i18n::tr("No folders found"))
                            .size(19.0)
                            .color(DIM),
                    );
                    return;
                }
                Grid::new("folder_breakdown")
                    .num_columns(3)
                    .spacing(vec2(18.0, 6.0))
                    .show(ui, |ui| {
                        for heading in ["Folder", "Size", "Files"] {
                            ui.label(RichText::new(crate::i18n::tr(heading)).strong());
                        }
                        ui.end_row();
                        for stat in &folders {
                            ui.label(RichText::new(truncate(&stat.name, 34)).size(18.0));
                            ui.label(RichText::new(format_bytes(stat.bytes)).size(18.0));
                            ui.label(RichText::new(stat.files.to_string()).size(18.0));
                            ui.end_row();
                        }
                    });
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("ghx-folders-{tag}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[test]
    fn breakdown_sums_sizes_and_counts_files() {
        let root = temp_dir("sums");
        std::fs::create_dir_all(root.join("games/sub")).unwrap();
        std::fs::write(root.join("games/a.exe"), vec![b'x'; 100]).unwrap();
        std::fs::write(root.join("games/sub/b.zip"), vec![b'x'; 50]).unwrap();
        std::fs::create_dir_all(root.join("media")).unwrap();
        std::fs::write(root.join("media/c.png"), vec![b'x'; 20]).unwrap();
        std::fs::write(root.join("loose.txt"), b"x").unwrap();
        // Hidden folder must be skipped entirely.
        std::fs::create_dir(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/secret.png"), vec![b'x'; 999]).unwrap();

        let rows = folder_breakdown(&root);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "games");
        assert_eq!(rows[0].bytes, 150);
        assert_eq!(rows[0].files, 2);
        assert_eq!(rows[1].name, "media");
        assert_eq!(rows[1].bytes, 20);
        assert!(rows.iter().all(|row| row.name != ".hidden"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn breakdown_handles_missing_root() {
        let root = temp_dir("missing").join("nope");
        assert!(folder_breakdown(&root).is_empty());
    }
}
