//! Self-contained "Album" screen.
//!
//! Lists the images the launcher writes into `<data dir>/covers` and
//! `<data dir>/screenshots` and renders them as an egui grid. The screen is
//! deliberately stateless with respect to [`App`]: its signature only borrows
//! the app immutably, and decoded textures live in a thread-local cache so a
//! host integration can call `show` from any UI pass without new app state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::load::SizedTexture;
use eframe::egui::*;
use gamehubx_platform::data_dir;

use crate::theme::{CARD, DIM, W};
use crate::widgets::{btn, section};

const IMAGE_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
const THUMB: Vec2 = vec2(210.0, 158.0);
const COLUMNS: usize = 4;

thread_local! {
    /// Decoded textures keyed by absolute path. egui textures are cheap to keep
    /// and expensive to decode, so never re-decode an image on every frame.
    static TEXTURES: RefCell<HashMap<String, TextureHandle>> = RefCell::new(HashMap::new());
}

/// Whether a host integration (drawer / settings row) has opened the album.
/// Kept here so no new `S` variant or `App` field is required.
static OPEN: AtomicBool = AtomicBool::new(false);

pub(crate) fn open() {
    OPEN.store(true, Ordering::Relaxed);
}

pub(crate) fn close() {
    OPEN.store(false, Ordering::Relaxed);
}

pub(crate) fn is_open() -> bool {
    OPEN.load(Ordering::Relaxed)
}

/// Drop cached textures so a later pass re-reads files from disk.
pub(crate) fn clear_textures() {
    TEXTURES.with(|cache| cache.borrow_mut().clear());
}

/// True when `path` has an image extension we can decode with `image`.
pub(crate) fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
        .unwrap_or(false)
}

/// List image files directly inside `dir`, sorted by path. Missing or
/// unreadable directories simply yield an empty list.
pub(crate) fn list_images(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_image(path))
        .collect();
    files.sort();
    files
}

/// Load (and cache) a texture for `path` using the `image` crate, mirroring the
/// decoding path used for game covers in `app.rs`.
fn load_texture(ctx: &Context, path: &Path) -> Option<TextureHandle> {
    let key = path.to_string_lossy().to_string();
    if let Some(texture) = TEXTURES.with(|cache| cache.borrow().get(&key).cloned()) {
        return Some(texture);
    }
    let bytes = std::fs::read(path).ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let color =
        ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());
    let texture = ctx.load_texture(format!("gallery-{key}"), color, TextureOptions::LINEAR);
    TEXTURES.with(|cache| {
        cache.borrow_mut().insert(key, texture.clone());
    });
    Some(texture)
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

fn image_grid(ui: &mut Ui, files: &[PathBuf], id: &str) {
    if files.is_empty() {
        ui.label(
            RichText::new(crate::i18n::tr("No images yet"))
                .size(19.0)
                .color(DIM),
        );
        return;
    }
    let ctx = ui.ctx().clone();
    Grid::new(id)
        .num_columns(COLUMNS)
        .spacing(vec2(18.0, 18.0))
        .show(ui, |ui| {
            for path in files {
                ui.vertical(|ui| {
                    let (rect, _) = ui.allocate_exact_size(THUMB, Sense::hover());
                    ui.painter().rect_filled(rect, Rounding::same(10.0), CARD);
                    if let Some(texture) = load_texture(&ctx, path) {
                        let sized = SizedTexture::new(texture.id(), texture.size_vec2());
                        ui.put(rect, Image::new(sized).fit_to_exact_size(rect.size()));
                    } else {
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "?",
                            FontId::proportional(40.0),
                            DIM,
                        );
                    }
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ui.label(RichText::new(truncate(&name, 24)).size(15.0).color(DIM));
                });
            }
        });
}

/// Render the album. `app` is only read (game count in the header), so this
/// function can be called from any `&App` context without extra app state.
pub(crate) fn show(app: &crate::app::App, ui: &mut Ui) {
    ui.add_space(18.0);
    ui.horizontal(|ui| {
        ui.add_space(46.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(crate::i18n::tr("Album")).size(34.0).strong());
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    crate::i18n::tr("Local library"),
                    app.games.len()
                ))
                .size(16.0)
                .color(DIM),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(46.0);
            if btn(ui, "✕", vec2(44.0, 44.0), CARD, W, 22.0).clicked() {
                close();
            }
        });
    });
    ui.add_space(10.0);
    ScrollArea::vertical().id_salt("album").show(ui, |ui| {
        ui.add_space(46.0);
        section(ui, "Covers", |ui| {
            let files = list_images(&data_dir().join("covers"));
            image_grid(ui, &files, "album_covers");
        });
        ui.add_space(18.0);
        section(ui, "Screenshots", |ui| {
            let files = list_images(&data_dir().join("screenshots"));
            image_grid(ui, &files, "album_screenshots");
        });
        ui.add_space(40.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("ghx-album-{tag}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn image_extension_detection() {
        assert!(is_image(Path::new("cover.PNG")));
        assert!(is_image(Path::new("shot.jpeg")));
        assert!(is_image(Path::new("anim.webp")));
        assert!(!is_image(Path::new("notes.txt")));
        assert!(!is_image(Path::new("no-extension")));
    }

    #[test]
    fn lists_only_images_sorted() {
        let dir = temp_dir("list");
        std::fs::write(dir.join("b.png"), b"x").unwrap();
        std::fs::write(dir.join("a.jpg"), b"x").unwrap();
        std::fs::write(dir.join("readme.txt"), b"x").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();

        let files = list_images(&dir);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.jpg", "b.png"]);
        assert!(list_images(&dir.join("missing")).is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn truncate_keeps_tail() {
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert_eq!(truncate("abcdefgh", 5), "…efgh");
    }
}
