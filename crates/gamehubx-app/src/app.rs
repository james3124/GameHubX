use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use eframe::egui::*;
use gamehubx_core::{Game, GameId, GameSettings};
use gamehubx_download::{DownloadSpec, DownloadStatus, DownloadTask};
use gamehubx_input::{builtin_profiles, Engine, Profile};
use gamehubx_platform::{
    battery_level, big_core_mask, data_dir, files_access_granted, files_root, launch_winlator_main,
    storage_stats, StorageStats,
};
use gamehubx_runtime::{
    apply_dxvk, install_component, AudioBackend, ContainerManager, ContainerMetadata, LocalSource,
    RuntimeLayout, RuntimeManifest, SessionConfig, SessionExtras, SessionPlan,
};
use gamehubx_storage::LibraryStore;
use gamehubx_supervisor::{PssSample, RunningSession, Supervisor};

use crate::download_queue::{remove_files, DownloadQueue};
use crate::overlays;
use crate::screens;
use crate::theme::*;
use crate::widgets::*;

const DESIGN_W: f32 = 1612.0;
const DESIGN_H: f32 = 720.0;

static UI_CONTEXT: OnceLock<Context> = OnceLock::new();

pub(crate) fn set_ui_context(ctx: &Context) {
    let _ = UI_CONTEXT.set(ctx.clone());
}

#[cfg(target_os = "android")]
pub(crate) fn wake_ui() {
    if let Some(ctx) = UI_CONTEXT.get() {
        ctx.request_repaint();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum S {
    Dash,
    Search,
    Pc,
    Dl,
    Mgmt,
    Files,
    Detail,
    Set,
    App,
    Session,
    Runtime,
    Touch,
    Diag,
}

pub(crate) type Ent = (String, bool, usize);

pub(crate) struct Tile;

/// Selector for an extended string setting cycled from the Game Settings UI.
#[derive(Clone, Copy)]
pub(crate) enum ExtendedField {
    ContainerLanguage,
    DirectxHud,
    CompatLayer,
    TranslationParams,
    DinputLibrary,
    GpuDriver,
    SurfaceFormat,
    Vkd3d,
    ConfigPreset,
}

/// Selector for an extended boolean setting toggled from the Game Settings UI.
#[derive(Clone, Copy)]
pub(crate) enum BoolField {
    Mangohud,
    DisableWm,
    AutoKeyboard,
    DisableGfxPlugin,
    LogServer,
    RtsGestures,
}

/// Which free-text setting the editor modal is editing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextEditorKind {
    CommandLine,
    WineDebug,
    EnvVars,
}

/// Why the file browser was opened.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilePurpose {
    Import,
    StartupExe,
    BackupArchive,
    CoverImage,
}

pub struct App {
    pub(crate) s: S,
    pub(crate) stack: Vec<S>,
    pub(crate) drawer: bool,
    pub(crate) modal: bool,
    pub(crate) popup: bool,
    pub(crate) path: PathBuf,
    pub(crate) ents: Vec<Ent>,
    pub(crate) pend: PathBuf,
    pub(crate) name: String,
    pub(crate) intro: String,
    pub(crate) query: String,
    pub(crate) games: Vec<Game>,
    pub(crate) tiles: Vec<Tile>,
    pub(crate) store: Option<LibraryStore>,
    pub(crate) store_error: Option<String>,
    pub(crate) files_access: bool,
    pub(crate) files_access_error: Option<String>,
    pub(crate) files_access_at: Instant,
    pub(crate) storage: Option<StorageStats>,
    pub(crate) storage_at: Instant,
    pub(crate) session: Option<RunningSession>,
    pub(crate) audio_session: Option<RunningSession>,
    pub(crate) session_game: String,
    pub(crate) session_status: String,
    pub(crate) session_missing: Vec<PathBuf>,
    pub(crate) session_error: Option<String>,
    pub(crate) session_pss: Option<PssSample>,
    pub(crate) runtime_message: Option<String>,
    pub(crate) settings_notice: Option<String>,
    pub(crate) downloads: Vec<DownloadTask>,
    pub(crate) downloads_at: Instant,
    pub(crate) download_queue: Option<DownloadQueue>,
    pub(crate) download_message: Option<String>,
    pub(crate) download_speeds: HashMap<i64, u64>,
    download_samples: HashMap<i64, (Instant, u64)>,
    pub(crate) runtime_rx: Option<Receiver<Result<String, String>>>,
    pub(crate) runtime_installing: bool,
    pub(crate) game_settings: GameSettings,
    pub(crate) settings_for: Option<GameId>,
    pub(crate) language_choice: String,
    font_lang: crate::i18n::Lang,
    pub(crate) touch_profiles: Vec<String>,
    pub(crate) touch_profiles_data: Vec<Profile>,
    pub(crate) touch_edit: Option<Profile>,
    pub(crate) touch_engine: Option<Engine>,
    pub(crate) touch_test_mode: bool,
    pub(crate) touch_events: Vec<String>,
    pub(crate) touch_message: Option<String>,
    pub(crate) touch_drag: Option<usize>,
    pub(crate) cur: usize,
    pub(crate) sel: usize,
    pub(crate) pane: usize,
    /// Active free-text settings editor (command line / debug params / env).
    pub(crate) text_editor: Option<TextEditorKind>,
    pub(crate) text_buffer: String,
    /// Cached cover textures keyed by game local id.
    pub(crate) cover_textures: HashMap<String, TextureHandle>,
    /// Purpose of the current file-browser visit.
    pub(crate) file_purpose: FilePurpose,
}

pub(crate) fn list(p: &Path) -> Vec<Ent> {
    let mut entries: Vec<Ent> = fs::read_dir(p)
        .map(|dir| {
            dir.flatten()
                .map(|entry| {
                    let meta = entry.metadata().ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    let count = if is_dir {
                        fs::read_dir(entry.path()).map(|d| d.count()).unwrap_or(0)
                    } else {
                        meta.map(|m| m.len() as usize).unwrap_or(0)
                    };
                    (
                        entry.file_name().to_string_lossy().to_string(),
                        is_dir,
                        count,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    entries
}

fn clock() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

fn paint_background(ctx: &Context) {
    let rect = ctx.screen_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Background, Id::new("app-bg")));

    let mut base = Mesh::default();
    base.colored_vertex(rect.left_top(), Color32::from_rgb(53, 59, 73));
    base.colored_vertex(rect.right_top(), Color32::from_rgb(53, 59, 73));
    base.colored_vertex(rect.right_bottom(), BG);
    base.colored_vertex(rect.left_bottom(), BG);
    base.add_triangle(0, 1, 2);
    base.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(base));

    let fade_top = rect.bottom() - 280.0;
    if fade_top > rect.top() {
        let mut fade = Mesh::default();
        let transparent = Color32::from_rgba_unmultiplied(26, 26, 26, 0);
        let solid = Color32::from_rgb(26, 26, 26);
        fade.colored_vertex(pos2(rect.left(), fade_top), transparent);
        fade.colored_vertex(pos2(rect.right(), fade_top), transparent);
        fade.colored_vertex(pos2(rect.right(), rect.bottom()), solid);
        fade.colored_vertex(pos2(rect.left(), rect.bottom()), solid);
        fade.add_triangle(0, 1, 2);
        fade.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(fade));
    }
}

fn install_fonts(ctx: &Context, chinese: bool) {
    if !chinese {
        ctx.set_fonts(FontDefinitions::default());
        return;
    }
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "gamehubx-cjk".to_owned(),
        std::sync::Arc::new(FontData::from_static(include_bytes!(
            "../../../assets/ui/fonts/GameHubX-CJK.ttf"
        ))),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("gamehubx-cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}

fn battery_text() -> String {
    battery_level()
        .map(|level| format!("{level}%"))
        .unwrap_or_else(|| "--%".to_owned())
}

/// Scale the fixed 1612x720 design to fit the current viewport.
///
/// `screen_rect` is in points, so physical size = size * pixels_per_point.
/// Setting the scale to `physical / design` makes the design fit both axes;
/// the change takes effect next pass and is stable once converged.
fn fit_design(ctx: &Context) {
    let screen = ctx.screen_rect().size();
    let ppp = ctx.pixels_per_point();
    if screen.x <= 1.0 || screen.y <= 1.0 || !ppp.is_finite() || ppp <= 0.0 {
        return;
    }
    let target = (screen.x * ppp / DESIGN_W)
        .min(screen.y * ppp / DESIGN_H)
        .clamp(0.25, 4.0);
    if target.is_finite() && (target - ppp).abs() > 0.001 {
        ctx.set_pixels_per_point(target);
    }
}

impl App {
    pub fn new(ctx: &Context) -> Self {
        set_ui_context(ctx);
        let path = files_root();
        let ents = list(&path);
        let data = data_dir();
        let _ = fs::create_dir_all(&data);
        let (store, games, store_error) = match LibraryStore::open(data.join("library.db")) {
            Ok(store) => match store.list() {
                Ok(games) => (Some(store), games, None),
                Err(error) => (
                    None,
                    Vec::new(),
                    Some(format!("library read failed: {error}")),
                ),
            },
            Err(error) => (
                None,
                Vec::new(),
                Some(format!("library open failed: {error}")),
            ),
        };
        let storage = storage_stats(&path);
        let language_choice = store
            .as_ref()
            .and_then(|store| store.setting("language").ok().flatten())
            .unwrap_or_else(|| "system".to_string());
        crate::i18n::set_choice(&language_choice);
        let font_lang = crate::i18n::current();
        install_fonts(ctx, font_lang == crate::i18n::Lang::ZhHans);
        let downloads = store
            .as_ref()
            .and_then(|store| store.downloads().ok())
            .unwrap_or_default();
        let has_pending = downloads.iter().any(|task| {
            matches!(
                task.status,
                DownloadStatus::Queued | DownloadStatus::Downloading
            )
        });
        let download_queue = if store.is_some() && has_pending {
            Some(DownloadQueue::start(
                data.join("library.db"),
                data.join("downloads"),
            ))
        } else {
            None
        };
        if let Some(queue) = &download_queue {
            queue.wake();
        }
        let mut touch_profiles_data = builtin_profiles();
        for profile in Profile::load_dir(&data.join("inputcontrols")) {
            if !touch_profiles_data
                .iter()
                .any(|known| known.name == profile.name)
            {
                touch_profiles_data.push(profile);
            }
        }
        let touch_profiles: Vec<String> = touch_profiles_data
            .iter()
            .map(|profile| profile.name.clone())
            .collect();
        let mut app = Self {
            s: S::Dash,
            stack: Vec::new(),
            drawer: false,
            modal: false,
            popup: false,
            path,
            ents,
            pend: PathBuf::new(),
            name: String::new(),
            intro: String::new(),
            query: String::new(),
            games,
            tiles: Vec::new(),
            store,
            store_error,
            files_access: files_access_granted(),
            files_access_error: None,
            files_access_at: Instant::now(),
            storage,
            storage_at: Instant::now(),
            session: None,
            audio_session: None,
            session_game: String::new(),
            session_status: String::new(),
            session_missing: Vec::new(),
            session_error: None,
            session_pss: None,
            runtime_message: None,
            settings_notice: None,
            downloads,
            downloads_at: Instant::now()
                .checked_sub(Duration::from_secs(1))
                .unwrap_or_else(Instant::now),
            download_queue,
            download_message: None,
            download_speeds: HashMap::new(),
            download_samples: HashMap::new(),
            runtime_rx: None,
            runtime_installing: false,
            game_settings: GameSettings::default(),
            settings_for: None,
            language_choice,
            font_lang,
            touch_profiles,
            touch_profiles_data,
            touch_edit: None,
            touch_engine: None,
            touch_test_mode: false,
            touch_events: Vec::new(),
            touch_message: None,
            touch_drag: None,
            cur: 0,
            sel: 0,
            pane: 0,
            text_editor: None,
            text_buffer: String::new(),
            cover_textures: HashMap::new(),
            file_purpose: FilePurpose::Import,
        };
        app.rebuild_tiles();
        // A pinned home-screen shortcut carries the exe path; open that game.
        if let Some(shortcut_exe) = gamehubx_platform::consume_shortcut_exe() {
            if let Some(position) = app.games.iter().position(|game| {
                game.exe
                    .as_ref()
                    .map(|p| p == &std::path::PathBuf::from(&shortcut_exe))
                    .unwrap_or(false)
            }) {
                app.cur = position;
                app.sel = position + 2;
                app.launch_game();
            }
        }
        app
    }

    pub(crate) fn rebuild_tiles(&mut self) {
        let mut tiles = Vec::with_capacity(self.games.len() + 2);
        tiles.push(Tile);
        tiles.push(Tile);
        for _ in &self.games {
            tiles.push(Tile);
        }
        self.tiles = tiles;
    }

    pub(crate) fn current_id(&self) -> Option<GameId> {
        self.games.get(self.cur).map(|game| game.id)
    }

    pub(crate) fn current_exe(&self) -> String {
        self.games
            .get(self.cur)
            .and_then(|game| game.exe.as_ref())
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    }

    pub(crate) fn remove_game(&mut self, id: GameId) {
        if let Some(store) = &self.store {
            if let Err(error) = store.delete(id) {
                self.store_error = Some(format!("library delete failed: {error}"));
            }
        }
        self.games.retain(|game| game.id != id);
        self.rebuild_tiles();
        self.cur = self.cur.min(self.games.len().saturating_sub(1));
        self.sel = self.sel.min(self.tiles.len().saturating_sub(1));
    }

    pub(crate) fn add_game(&mut self, game: Game) {
        if let Some(store) = &self.store {
            if let Err(error) = store.upsert(&game) {
                self.store_error = Some(format!("library write failed: {error}"));
            }
        }
        self.games.push(game);
        self.rebuild_tiles();
        self.sel = self.tiles.len().saturating_sub(1);
    }

    pub(crate) fn set_startup_exe(&mut self, id: GameId, path: PathBuf) {
        if let Some(game) = self.games.iter_mut().find(|game| game.id == id) {
            game.exe = Some(path.clone());
        }
        if let Some(store) = &self.store {
            if let Err(error) = store.update_exe(id, Some(&path)) {
                self.store_error = Some(format!("library update failed: {error}"));
            }
        }
    }

    pub(crate) fn refresh_storage(&mut self) {
        if self.storage.is_none() || self.storage_at.elapsed() >= Duration::from_secs(30) {
            self.storage = storage_stats(&files_root());
            self.storage_at = Instant::now();
        }
    }

    pub(crate) fn refresh_files_access(&mut self) {
        let was_granted = self.files_access;
        self.files_access = files_access_granted();
        self.files_access_at = Instant::now();
        if !was_granted && self.files_access && self.s == S::Files {
            self.nav(files_root());
        }
    }

    pub(crate) fn load_current_settings(&mut self) {
        let id = self.current_id();
        if self.settings_for == id && id.is_some() {
            return;
        }
        let settings = match (id, &self.store) {
            (Some(id), Some(store)) => store.settings(id).unwrap_or_default(),
            _ => GameSettings::default(),
        };
        if !self
            .touch_profiles
            .iter()
            .any(|name| name == &settings.touch_profile)
        {
            self.touch_profiles.push(settings.touch_profile.clone());
        }
        self.game_settings = settings;
        self.settings_for = id;
    }

    pub(crate) fn save_game_settings(&mut self) {
        let result = if let (Some(id), Some(store)) = (self.current_id(), &self.store) {
            Some(store.save_settings(id, &self.game_settings))
        } else {
            None
        };
        if let Some(Err(error)) = result {
            self.store_error = Some(format!("settings save failed: {error}"));
        }
    }

    pub(crate) fn audio_backend(&self) -> AudioBackend {
        AudioBackend::from_label(&self.game_settings.audio_backend)
    }

    pub(crate) fn cycle_resolution(&mut self) {
        const OPTIONS: [(u32, u32); 3] = [(1280, 720), (1920, 1080), (960, 540)];
        let position = OPTIONS
            .iter()
            .position(|(width, height)| {
                *width == self.game_settings.width && *height == self.game_settings.height
            })
            .unwrap_or(0);
        let (width, height) = OPTIONS[(position + 1) % OPTIONS.len()];
        self.game_settings.width = width;
        self.game_settings.height = height;
        self.save_game_settings();
    }

    pub(crate) fn cycle_fps(&mut self) {
        const OPTIONS: [u32; 3] = [30, 60, 120];
        let position = OPTIONS
            .iter()
            .position(|cap| *cap == self.game_settings.fps_cap)
            .unwrap_or(1);
        self.game_settings.fps_cap = OPTIONS[(position + 1) % OPTIONS.len()];
        self.save_game_settings();
    }

    pub(crate) fn cycle_dxvk(&mut self) {
        const OPTIONS: [&str; 2] = ["Default", "DXVK"];
        let position = OPTIONS
            .iter()
            .position(|value| *value == self.game_settings.dxvk_version)
            .unwrap_or(0);
        self.game_settings.dxvk_version = OPTIONS[(position + 1) % OPTIONS.len()].to_string();
        self.save_game_settings();
    }

    pub(crate) fn cycle_audio(&mut self) {
        const OPTIONS: [&str; 2] = ["Pulse", "Alsa"];
        let position = OPTIONS
            .iter()
            .position(|value| *value == self.game_settings.audio_backend)
            .unwrap_or(0);
        self.game_settings.audio_backend = OPTIONS[(position + 1) % OPTIONS.len()].to_string();
        self.save_game_settings();
    }

    pub(crate) fn cycle_touch_profile(&mut self) {
        if self.touch_profiles.is_empty() {
            return;
        }
        let position = self
            .touch_profiles
            .iter()
            .position(|name| name == &self.game_settings.touch_profile)
            .unwrap_or(0);
        self.game_settings.touch_profile =
            self.touch_profiles[(position + 1) % self.touch_profiles.len()].clone();
        self.save_game_settings();
    }

    pub(crate) fn cycle_memory_limit(&mut self) {
        const OPTIONS: [u32; 4] = [0, 2048, 3072, 4096];
        let position = OPTIONS
            .iter()
            .position(|value| *value == self.game_settings.memory_limit_mb)
            .unwrap_or(0);
        self.game_settings.memory_limit_mb = OPTIONS[(position + 1) % OPTIONS.len()];
        self.save_game_settings();
    }

    pub(crate) fn cycle_cpu_affinity(&mut self) {
        self.game_settings.cpu_affinity_big = !self.game_settings.cpu_affinity_big;
        self.save_game_settings();
    }

    /// Cycle one of the extended string settings backed by a core option list.
    pub(crate) fn cycle_extended(&mut self, options: &[&str], field: ExtendedField) {
        use gamehubx_core::cycle_option;
        let current = match field {
            ExtendedField::ContainerLanguage => &self.game_settings.container_language,
            ExtendedField::DirectxHud => &self.game_settings.directx_hud,
            ExtendedField::CompatLayer => &self.game_settings.compat_layer,
            ExtendedField::TranslationParams => &self.game_settings.translation_params,
            ExtendedField::DinputLibrary => &self.game_settings.dinput_library,
            ExtendedField::GpuDriver => &self.game_settings.gpu_driver,
            ExtendedField::SurfaceFormat => &self.game_settings.surface_format,
            ExtendedField::Vkd3d => &self.game_settings.vkd3d_version,
            ExtendedField::ConfigPreset => &self.game_settings.config_preset,
        }
        .clone();
        let next = cycle_option(options, &current);
        match field {
            ExtendedField::ContainerLanguage => self.game_settings.container_language = next,
            ExtendedField::DirectxHud => self.game_settings.directx_hud = next,
            ExtendedField::CompatLayer => self.game_settings.compat_layer = next,
            ExtendedField::TranslationParams => self.game_settings.translation_params = next,
            ExtendedField::DinputLibrary => self.game_settings.dinput_library = next,
            ExtendedField::GpuDriver => self.game_settings.gpu_driver = next,
            ExtendedField::SurfaceFormat => self.game_settings.surface_format = next,
            ExtendedField::Vkd3d => self.game_settings.vkd3d_version = next,
            ExtendedField::ConfigPreset => {
                self.game_settings.config_preset = next.clone();
                self.game_settings.apply_preset(&next);
            }
        }
        self.save_game_settings();
    }

    /// Toggle one of the extended boolean settings and persist it.
    pub(crate) fn toggle_extended(&mut self, field: BoolField) {
        match field {
            BoolField::Mangohud => self.game_settings.mangohud = !self.game_settings.mangohud,
            BoolField::DisableWm => self.game_settings.disable_wm = !self.game_settings.disable_wm,
            BoolField::AutoKeyboard => {
                self.game_settings.auto_keyboard = !self.game_settings.auto_keyboard
            }
            BoolField::DisableGfxPlugin => {
                self.game_settings.disable_gfx_plugin = !self.game_settings.disable_gfx_plugin
            }
            BoolField::LogServer => self.game_settings.log_server = !self.game_settings.log_server,
            BoolField::RtsGestures => {
                self.game_settings.rts_gestures = !self.game_settings.rts_gestures
            }
        }
        self.save_game_settings();
    }

    pub(crate) fn cycle_cpu_core_limit(&mut self) {
        const OPTIONS: [u32; 5] = [0, 2, 4, 6, 8];
        let position = OPTIONS
            .iter()
            .position(|value| *value == self.game_settings.cpu_core_limit)
            .unwrap_or(0);
        self.game_settings.cpu_core_limit = OPTIONS[(position + 1) % OPTIONS.len()];
        self.save_game_settings();
    }

    /// Back up the current game's container into the app data dir.
    pub(crate) fn backup_current_container(&mut self) {
        use gamehubx_runtime::backup_container;
        let Some(id) = self.current_id() else {
            self.settings_notice = Some("No game selected".into());
            return;
        };
        let container = data_dir().join("containers").join(format!("{:016x}", id.0));
        let backups = data_dir().join("backups");
        let label = self
            .games
            .get(self.cur)
            .map(|game| game.label.clone())
            .unwrap_or_else(|| id.0.to_string());
        let safe: String = label
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let archive = backups.join(format!("{safe}-{:016x}.tar.gz", id.0));
        match backup_container(&container, &archive) {
            Ok(bytes) => {
                self.settings_notice = Some(format!(
                    "Backed up container ({} bytes) to {}",
                    bytes,
                    archive.display()
                ));
            }
            Err(error) => {
                self.settings_notice = Some(format!("Backup failed: {error}"));
            }
        }
    }

    /// Restore the current game's container from a chosen archive path stored
    /// in `pend`. The path is validated to be an existing file.
    pub(crate) fn restore_current_container(&mut self) {
        use gamehubx_runtime::restore_container;
        let Some(id) = self.current_id() else {
            self.settings_notice = Some("No game selected".into());
            return;
        };
        if self.pend.as_os_str().is_empty() || !self.pend.is_file() {
            self.settings_notice = Some("Choose a backup archive first (Files → .tar.gz)".into());
            return;
        }
        let container = data_dir().join("containers").join(format!("{:016x}", id.0));
        match restore_container(&self.pend, &container) {
            Ok(count) => {
                self.settings_notice = Some(format!("Restored {count} file(s) into container"));
                self.pend = PathBuf::new();
            }
            Err(error) => {
                self.settings_notice = Some(format!("Restore failed: {error}"));
            }
        }
    }

    /// Reset the current game's container data by deleting and recreating it.
    pub(crate) fn reset_current_container(&mut self) {
        use gamehubx_runtime::{ContainerManager, ContainerMetadata};
        let Some(id) = self.current_id() else {
            self.settings_notice = Some("No game selected".into());
            return;
        };
        let manager = match ContainerManager::new(data_dir().join("containers")) {
            Ok(manager) => manager,
            Err(error) => {
                self.settings_notice = Some(format!("Container reset failed: {error}"));
                return;
            }
        };
        let label = self
            .games
            .get(self.cur)
            .map(|game| game.label.clone())
            .unwrap_or_else(|| "Game".into());
        let metadata = ContainerMetadata::new(label, "/runtime/wine/bin/wine64", "open");
        let result = manager
            .delete(id)
            .ok()
            .and_then(|_| manager.create(id, metadata).ok());
        self.settings_notice = Some(if result.is_some() {
            "Container data reset".into()
        } else {
            "Container reset failed".into()
        });
    }

    /// Load (and cache) a game's cover texture. Prefers an explicit cover file;
    /// otherwise extracts the embedded icon from the game's .exe, mirroring the
    /// reference launcher's automatic cover handling.
    pub(crate) fn cover_texture(&mut self, ctx: &Context, game: &Game) -> Option<TextureHandle> {
        let key = game.local_id();
        if let Some(texture) = self.cover_textures.get(&key) {
            return Some(texture.clone());
        }
        let image = game
            .cover
            .as_ref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| image::load_from_memory(&bytes).ok())
            .map(|dynamic| dynamic.to_rgba8())
            .and_then(|rgba| {
                let (w, h) = rgba.dimensions();
                if w == 0 || h == 0 {
                    None
                } else {
                    Some(ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        rgba.as_raw(),
                    ))
                }
            })
            .or_else(|| {
                let exe = game.exe.as_ref()?;
                let bytes = fs::read(exe).ok()?;
                let icon = crate::pe_icon::extract_first_icon(&bytes)?;
                Some(ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ))
            })?;
        let texture = ctx.load_texture(format!("cover-{key}"), image, TextureOptions::LINEAR);
        self.cover_textures.insert(key, texture.clone());
        Some(texture)
    }

    pub(crate) fn open_text_editor(&mut self, kind: TextEditorKind) {
        self.load_current_settings();
        self.text_buffer = match kind {
            TextEditorKind::CommandLine => self.game_settings.command_line.clone(),
            TextEditorKind::WineDebug => self.game_settings.wine_debug_params.clone(),
            TextEditorKind::EnvVars => String::new(),
        };
        self.text_editor = Some(kind);
    }

    pub(crate) fn open_env_editor(&mut self) {
        self.load_current_settings();
        // One "KEY=VALUE" per line.
        self.text_buffer = self
            .game_settings
            .env_vars
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.text_editor = Some(TextEditorKind::EnvVars);
    }

    /// Commit the active text editor buffer into GameSettings and persist.
    pub(crate) fn commit_text_editor(&mut self) {
        let Some(kind) = self.text_editor else {
            return;
        };
        match kind {
            TextEditorKind::CommandLine => {
                self.game_settings.command_line = self.text_buffer.trim().to_string();
            }
            TextEditorKind::WineDebug => {
                let value = self.text_buffer.trim();
                self.game_settings.wine_debug_params = if value.is_empty() {
                    "-all".to_string()
                } else {
                    value.to_string()
                };
            }
            TextEditorKind::EnvVars => {
                let mut vars = Vec::new();
                for line in self.text_buffer.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        if !key.is_empty()
                            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        {
                            vars.push((key.to_string(), value.trim().to_string()));
                        }
                    }
                }
                self.game_settings.env_vars = vars;
            }
        }
        self.save_game_settings();
        self.text_editor = None;
        self.text_buffer.clear();
    }

    /// Names of runtime components that have a verified install marker.
    pub(crate) fn runtime_component_names(&self) -> Vec<String> {
        let root = self.runtime_root();
        let names = [
            "proot",
            "wine",
            "rootfs",
            "xserver",
            "box64",
            "dxvk",
            "vkd3d",
            "pulseaudio",
        ];
        names
            .iter()
            .filter(|name| {
                let dir = root.join(name);
                let marker = root.join(format!(".{name}.installed"));
                dir.exists() || marker.exists()
            })
            .map(|name| name.to_string())
            .collect()
    }

    /// Launch a Wine container desktop (explorer.exe) through the Rust session
    /// path. On Android the Winlator session handles game launches instead.
    pub(crate) fn launch_container_desktop(&mut self) {
        #[cfg(target_os = "android")]
        {
            self.settings_notice = Some("Use the Winlator Session desktop from Settings".into());
            return;
        }
        #[cfg(not(target_os = "android"))]
        {
            let missing = RuntimeLayout::new(self.runtime_root()).missing_components();
            self.settings_notice = Some(if missing.is_empty() {
                "Container desktop requires an interactive session host with explorer.exe".into()
            } else {
                format!(
                    "Runtime pack incomplete ({} component(s) missing); install it before opening a desktop",
                    missing.len()
                )
            });
        }
    }

    /// Pin the current game to the Android home screen (no-op on desktop).
    pub(crate) fn pin_current_game(&mut self) {
        #[cfg(target_os = "android")]
        {
            let exe = self.current_exe();
            let label = self
                .games
                .get(self.cur)
                .map(|game| game.label.clone())
                .unwrap_or_default();
            let ok = gamehubx_platform::add_home_shortcut(&exe, &label);
            self.settings_notice = Some(if ok {
                "Shortcut pinned to Home".into()
            } else {
                "Shortcut pinning is unavailable or was cancelled".into()
            });
        }
        #[cfg(not(target_os = "android"))]
        {
            self.settings_notice = Some("Home-screen shortcuts are available on Android".into());
        }
    }

    /// Import the pending .exe, extracting its embedded PE icon as a PNG cover
    /// (mirroring the reference launcher's automatic cover behaviour).
    pub(crate) fn import_pending_with_cover(&mut self, label: String, intro: String) {
        let cover_path = self
            .pend
            .file_stem()
            .and_then(|stem| {
                std::fs::read(&self.pend)
                    .ok()
                    .and_then(|bytes| crate::pe_icon::extract_first_icon(&bytes))
                    .map(|icon| (stem.to_string_lossy().to_string(), icon))
            })
            .and_then(|(stem, icon)| {
                let dir = data_dir().join("covers");
                if std::fs::create_dir_all(&dir).is_err() {
                    return None;
                }
                let path = dir.join(format!("{stem}.png"));
                let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.rgba)?;
                buffer.save(&path).ok()?;
                Some(path)
            });

        let mut game = Game::new_pc_exe(label, intro, self.pend.clone());
        game.cover = cover_path;
        self.add_game(game);
        self.reset_import();
        self.modal = false;
    }

    /// Set the cover image path from the pending file selection.
    pub(crate) fn set_pending_cover(&mut self) {
        if self.pend.as_os_str().is_empty() {
            return;
        }
        let path = self.pend.clone();
        if let Some(id) = self.current_id() {
            if let Some(game) = self.games.iter_mut().find(|game| game.id == id) {
                game.cover = Some(path.clone());
            }
            if let Some(store) = &self.store {
                let _ = store.set_cover(id, Some(&path));
            }
            self.settings_notice = Some(format!("Cover set to {}", path.display()));
        }
        self.pend = PathBuf::new();
    }

    pub(crate) fn open_touch_editor(&mut self) {
        self.load_current_settings();
        let selected = self
            .touch_profiles_data
            .iter()
            .find(|profile| profile.name == self.game_settings.touch_profile)
            .cloned()
            .or_else(|| self.touch_profiles_data.first().cloned());
        if let Some(profile) = selected {
            self.touch_edit = Some(profile.clone());
            self.touch_engine = Some(Engine::new(profile));
            self.touch_test_mode = false;
            self.touch_events.clear();
            self.touch_message = None;
            self.touch_drag = None;
        }
        self.go(S::Touch);
    }

    pub(crate) fn cycle_touch_editor_profile(&mut self) {
        if self.touch_profiles_data.is_empty() {
            return;
        }
        let position = self
            .touch_edit
            .as_ref()
            .and_then(|profile| {
                self.touch_profiles_data
                    .iter()
                    .position(|candidate| candidate.name == profile.name)
            })
            .unwrap_or(0);
        let next =
            self.touch_profiles_data[(position + 1) % self.touch_profiles_data.len()].clone();
        self.game_settings.touch_profile = next.name.clone();
        if !self.touch_profiles.contains(&next.name) {
            self.touch_profiles.push(next.name.clone());
        }
        self.touch_edit = Some(next.clone());
        self.touch_engine = Some(Engine::new(next));
        self.save_game_settings();
        self.touch_events.clear();
        self.touch_message = None;
    }

    pub(crate) fn save_touch_profile(&mut self) {
        let Some(profile) = self.touch_edit.clone() else {
            return;
        };
        let id = self
            .touch_profiles_data
            .iter()
            .map(|profile| profile.id)
            .max()
            .unwrap_or(0)
            + 1;
        let name = format!("{} Custom", profile.name);
        let mut saved = profile;
        saved.id = id;
        saved.name = name.clone();

        let dir = data_dir().join("inputcontrols");
        if let Err(error) = fs::create_dir_all(&dir) {
            self.touch_message = Some(format!("Cannot create {}: {error}", dir.display()));
            return;
        }
        let json = match saved.to_json() {
            Ok(json) => json,
            Err(error) => {
                self.touch_message = Some(format!("Cannot encode profile: {error:?}"));
                return;
            }
        };
        let path = dir.join(format!("{id}.icp"));
        if let Err(error) = fs::write(&path, json) {
            self.touch_message = Some(format!("Cannot write {}: {error}", path.display()));
            return;
        }
        if !self
            .touch_profiles_data
            .iter()
            .any(|profile| profile.name == name)
        {
            self.touch_profiles_data.push(saved.clone());
        }
        if !self.touch_profiles.contains(&name) {
            self.touch_profiles.push(name.clone());
        }
        self.game_settings.touch_profile = name;
        self.save_game_settings();
        self.touch_edit = Some(saved.clone());
        self.touch_engine = Some(Engine::new(saved));
        self.touch_message = Some(format!("Saved {}", path.display()));
    }

    pub(crate) fn launch_game(&mut self) {
        if self.session.is_some() {
            self.stop_session();
        }
        self.session_error = None;
        self.session_missing.clear();
        self.session_pss = None;
        self.session_status = "Not started".into();
        self.load_current_settings();

        let Some(game) = self.games.get(self.cur) else {
            self.session_game.clear();
            self.session_error = Some("No game selected".into());
            self.go(S::Session);
            return;
        };
        let label = game.label.clone();
        let exe = game.exe.clone();
        self.session_game = label.clone();

        let Some(exe) = exe else {
            self.session_error = Some("Startup file path is not set".into());
            self.go(S::Session);
            return;
        };

        #[cfg(target_os = "android")]
        {
            let exe_text = exe.to_string_lossy();
            let settings_json = gamehubx_runtime::session_settings_json(&self.game_settings);
            match gamehubx_platform::launch_winlator_game(exe_text.as_ref(), &label, &settings_json)
            {
                Ok(()) => {
                    self.session_status = "Launching in Winlator".into();
                    return;
                }
                Err(error) => {
                    self.session_error = Some(error);
                    self.go(S::Session);
                    return;
                }
            }
        }

        let Some(game_dir) = exe.parent().map(Path::to_path_buf) else {
            self.session_error = Some("Startup path has no parent folder".into());
            self.go(S::Session);
            return;
        };

        let data = data_dir();
        let settings = self.game_settings.clone();
        let cpu_affinity = if settings.cpu_affinity_big {
            match big_core_mask() {
                Some(mask) => Some(mask),
                None => {
                    self.session_error =
                        Some("Big-core affinity is unavailable on this device".into());
                    self.go(S::Session);
                    return;
                }
            }
        } else {
            None
        };
        let extras = SessionExtras {
            container_language: settings.container_language.clone(),
            mangohud: settings.mangohud,
            directx_hud: settings.directx_hud.clone(),
            disable_wm: settings.disable_wm,
            env_vars: settings.env_vars.clone(),
            command_line: settings.command_line.clone(),
            auto_keyboard: settings.auto_keyboard,
            disable_gfx_plugin: settings.disable_gfx_plugin,
            compat_layer: settings.compat_layer.clone(),
            translation_params: settings.translation_params.clone(),
            dinput_library: settings.dinput_library.clone(),
            gpu_driver: settings.gpu_driver.clone(),
            surface_format: settings.surface_format.clone(),
            vkd3d_version: settings.vkd3d_version.clone(),
            log_server: settings.log_server,
            wine_debug_params: settings.wine_debug_params.clone(),
            cpu_core_limit: settings.cpu_core_limit,
        };
        let config = SessionConfig {
            layout: RuntimeLayout::new(data.join("runtime")),
            container: data.join("containers").join(format!("{:016x}", game.id.0)),
            game_dir,
            exe,
            fps_cap: settings.fps_cap,
            width: settings.width,
            height: settings.height,
            audio: self.audio_backend(),
            dxvk: settings.dxvk_version != "Default",
            memory_limit_mb: settings.memory_limit_mb,
            cpu_affinity,
            extras,
        };
        let plan = match SessionPlan::new(config) {
            Ok(plan) => plan,
            Err(error) => {
                self.session_error = Some(format!("Session plan failed: {error}"));
                self.go(S::Session);
                return;
            }
        };

        let missing = plan.missing_runtime();
        if !missing.is_empty() {
            self.session_missing = missing;
            self.session_error = Some("Runtime pack is incomplete".into());
            self.go(S::Session);
            return;
        }

        if let Ok(manager) = ContainerManager::new(data.join("containers")) {
            let metadata = ContainerMetadata::new(
                label,
                plan.config().layout.wine().display().to_string(),
                "open",
            );
            let _ = manager.repair(game.id, metadata);
        }

        let layout = plan.config().layout.clone();
        let container = plan.config().container.clone();
        if let Err(error) = apply_dxvk(&layout, &container, plan.config().dxvk) {
            self.session_error = Some(format!("DXVK apply failed: {error:?}"));
            self.go(S::Session);
            return;
        }
        if plan.config().extras.vkd3d_version == "VKD3D" {
            if let Err(error) = apply_dxvk(&layout, &container, true) {
                self.session_error = Some(format!("VKD3D apply failed: {error:?}"));
                self.go(S::Session);
                return;
            }
        }
        if let Err(error) = plan.prepare_audio() {
            self.session_error = Some(format!("Audio setup failed: {error:?}"));
            self.go(S::Session);
            return;
        }
        if let Some(command) = plan.audio_command() {
            match Supervisor::spawn(&command) {
                Ok(session) => self.audio_session = Some(session),
                Err(error) => {
                    self.session_error = Some(format!("Audio start failed: {error}"));
                    self.go(S::Session);
                    return;
                }
            }
        }

        match Supervisor::spawn(&plan.command) {
            Ok(session) => {
                self.session = Some(session);
                self.session_status = "Running".into();
                self.go(S::Session);
            }
            Err(error) => {
                self.stop_audio();
                self.session_error = Some(format!("Failed to start session: {error}"));
                self.go(S::Session);
            }
        }
    }

    pub(crate) fn stop_audio(&mut self) {
        if let Some(mut audio) = self.audio_session.take() {
            let _ = audio.terminate();
        }
    }

    pub(crate) fn stop_session(&mut self) {
        self.stop_audio();
        if let Some(mut session) = self.session.take() {
            let _ = session.terminate();
            self.session_status = "Stopped".into();
            self.session_pss = None;
        }
    }

    pub(crate) fn runtime_root(&self) -> PathBuf {
        data_dir().join("runtime")
    }

    pub(crate) fn runtime_pack_dir(&self) -> PathBuf {
        let shared = files_root().join("GameHubX/runtime-pack");
        if shared.is_dir() {
            shared
        } else {
            data_dir().join("runtime-pack")
        }
    }

    pub(crate) fn start_runtime_install(&mut self) {
        if self.runtime_installing {
            return;
        }
        self.runtime_message = None;
        let (pack_dir, manifest) = match self.runtime_manifest() {
            Ok(value) => value,
            Err(error) => {
                self.runtime_message = Some(error);
                return;
            }
        };

        let layout = RuntimeLayout::new(self.runtime_root());
        let downloads_dir = data_dir().join("downloads");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut installed = 0usize;
            for component in &manifest.components {
                let archive = format!("{}.tar.gz", component.name);
                let source_dir = if pack_dir.join(&archive).is_file() {
                    pack_dir.clone()
                } else if downloads_dir.join(&archive).is_file() {
                    downloads_dir.clone()
                } else {
                    let _ = tx.send(Err(format!(
                        "{}: archive not found; queue the component download first",
                        component.name
                    )));
                    return;
                };
                let source = LocalSource::new(source_dir);
                match install_component(&layout, &source, component) {
                    Ok(()) => installed += 1,
                    Err(error) => {
                        let _ = tx.send(Err(format!("{}: {error:?}", component.name)));
                        return;
                    }
                }
            }
            let _ = tx.send(Ok(format!("Installed {installed} component(s)")));
        });
        self.runtime_rx = Some(rx);
        self.runtime_installing = true;
    }

    pub(crate) fn poll_runtime_install(&mut self) {
        let mut result = None;
        if let Some(rx) = &self.runtime_rx {
            match rx.try_recv() {
                Ok(value) => result = Some(value),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    result = Some(Err("installer thread stopped".to_string()));
                }
            }
        }
        if let Some(value) = result {
            self.runtime_rx = None;
            self.runtime_installing = false;
            self.runtime_message = Some(match value {
                Ok(message) => message,
                Err(error) => format!("Install failed: {error}"),
            });
        }
    }

    pub(crate) fn reset_import(&mut self) {
        self.name.clear();
        self.intro.clear();
        self.pend = PathBuf::new();
    }

    pub(crate) fn go(&mut self, s: S) {
        self.stack.push(self.s);
        self.s = s;
        self.drawer = false;
    }

    pub(crate) fn nav(&mut self, p: PathBuf) {
        self.ents = list(&p);
        self.path = p;
    }

    pub(crate) fn open_winlator(&mut self) {
        self.settings_notice = launch_winlator_main().err();
    }

    pub(crate) fn open_winlator_setup(&mut self) {
        if let Err(error) = launch_winlator_main() {
            self.session_error = Some(error);
        }
    }

    #[cfg(target_os = "android")]
    fn handle_gamepad(&mut self, action: i32) {
        match action {
            5 => {
                if self.s == S::Dash {
                    match self.sel {
                        0 => self.go(S::Pc),
                        1 => {}
                        n => {
                            if n >= 2 {
                                self.cur = n - 2;
                                self.go(S::Detail);
                            }
                        }
                    }
                }
            }
            6 => self.back(),
            7 => self.drawer = true,
            8 => {
                if self.s == S::Dash {
                    self.go(S::Search);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn cycle_language(&mut self) {
        self.language_choice = crate::i18n::next_choice(&self.language_choice).to_string();
        crate::i18n::set_choice(&self.language_choice);
        if let Some(store) = &self.store {
            let _ = store.save_setting("language", &self.language_choice);
        }
    }

    pub(crate) fn language_value(&self) -> String {
        crate::i18n::choice_label(&self.language_choice)
    }

    fn load_downloads(&mut self) {
        self.downloads_at = Instant::now();
        let downloads = match &self.store {
            Some(store) => match store.downloads() {
                Ok(downloads) => downloads,
                Err(error) => {
                    self.download_message = Some(format!("Cannot read downloads: {error}"));
                    Vec::new()
                }
            },
            None => Vec::new(),
        };
        let now = Instant::now();
        let mut samples = HashMap::new();
        let mut speeds = HashMap::new();
        for task in &downloads {
            if task.status == DownloadStatus::Downloading {
                if let Some((when, bytes)) = self.download_samples.get(&task.id) {
                    let seconds = now.duration_since(*when).as_secs_f64();
                    if seconds >= 0.2 {
                        let speed =
                            (task.bytes_done.saturating_sub(*bytes) as f64 / seconds) as u64;
                        speeds.insert(task.id, speed);
                    }
                }
                samples.insert(task.id, (now, task.bytes_done));
            }
        }
        self.downloads = downloads;
        self.download_samples = samples;
        self.download_speeds = speeds;
    }

    pub(crate) fn refresh_downloads(&mut self) {
        if self.downloads_at.elapsed() < Duration::from_millis(400) {
            return;
        }
        self.load_downloads();
    }

    pub(crate) fn downloads_active(&self) -> bool {
        self.downloads.iter().any(|task| {
            matches!(
                task.status,
                DownloadStatus::Queued | DownloadStatus::Downloading
            )
        })
    }

    fn ensure_download_queue(&mut self) -> Result<(), String> {
        if self.download_queue.is_some() {
            return Ok(());
        }
        if self.store.is_none() {
            return Err("Library database is unavailable".into());
        }
        let data = data_dir();
        let queue = DownloadQueue::start(data.join("library.db"), data.join("downloads"));
        queue.wake();
        self.download_queue = Some(queue);
        Ok(())
    }

    pub(crate) fn queue_download(&mut self, spec: DownloadSpec) -> Result<i64, String> {
        let id = {
            let store = self
                .store
                .as_ref()
                .ok_or_else(|| "Library database is unavailable".to_string())?;
            store
                .insert_download(&spec, chrono::Utc::now().timestamp())
                .map_err(|error| format!("Cannot queue download: {error}"))?
        };
        self.ensure_download_queue()?;
        if let Some(queue) = &self.download_queue {
            queue.wake();
        }
        self.load_downloads();
        Ok(id)
    }

    pub(crate) fn pause_download(&mut self, id: i64) {
        let paused = self
            .store
            .as_ref()
            .and_then(|store| store.pause_active_download(id).ok())
            .unwrap_or(false);
        if paused {
            if let Some(queue) = &self.download_queue {
                queue.cancel_current();
            }
        }
        self.load_downloads();
    }

    pub(crate) fn resume_download(&mut self, id: i64) {
        if let Some(store) = &self.store {
            let _ = store.requeue_download(id);
        }
        if let Err(error) = self.ensure_download_queue() {
            self.download_message = Some(error);
        }
        if let Some(queue) = &self.download_queue {
            queue.wake();
        }
        self.load_downloads();
    }

    pub(crate) fn remove_download(&mut self, id: i64) {
        let task = self.downloads.iter().find(|task| task.id == id).cloned();
        if let Some(task) = &task {
            if task.status == DownloadStatus::Downloading {
                if let Some(queue) = &self.download_queue {
                    queue.cancel_current();
                }
            }
        }
        if let Some(store) = &self.store {
            let _ = store.delete_download(id);
        }
        if let Some(task) = task {
            remove_files(&task.dest);
        }
        self.load_downloads();
    }

    pub(crate) fn clear_completed_downloads(&mut self) {
        if let Some(store) = &self.store {
            let _ = store.clear_completed_downloads();
        }
        self.load_downloads();
    }

    fn runtime_manifest(&self) -> Result<(PathBuf, RuntimeManifest), String> {
        let pack_dir = self.runtime_pack_dir();
        let manifest_path = pack_dir.join("manifest.json");
        let text = fs::read_to_string(&manifest_path)
            .map_err(|error| format!("Cannot read {}: {error}", manifest_path.display()))?;
        let manifest = RuntimeManifest::from_json(&text)
            .map_err(|error| format!("Invalid manifest: {error:?}"))?;
        manifest
            .validate()
            .map_err(|error| format!("Manifest rejected: {error:?}"))?;
        Ok((pack_dir, manifest))
    }

    pub(crate) fn queue_runtime_downloads(&mut self) {
        self.runtime_message = None;
        self.download_message = None;
        let (pack_dir, manifest) = match self.runtime_manifest() {
            Ok(value) => value,
            Err(error) => {
                self.runtime_message = Some(error);
                return;
            }
        };
        let downloads_dir = data_dir().join("downloads");
        if let Err(error) = fs::create_dir_all(&downloads_dir) {
            self.runtime_message = Some(format!(
                "Cannot create {}: {error}",
                downloads_dir.display()
            ));
            return;
        }
        self.load_downloads();
        let existing = self.downloads.clone();
        let mut queued = 0usize;
        let mut retried = 0usize;
        let mut skipped = 0usize;
        let mut failures = Vec::new();

        for component in &manifest.components {
            if !component.url.starts_with("https://") {
                continue;
            }
            let file_name = format!("{}.tar.gz", component.name);
            let dest = downloads_dir.join(&file_name);
            if pack_dir.join(&file_name).is_file() {
                skipped += 1;
                continue;
            }
            if dest.is_file() {
                let size_ok = fs::metadata(&dest)
                    .map(|meta| meta.len() == component.size_bytes)
                    .unwrap_or(false);
                if size_ok {
                    skipped += 1;
                    continue;
                }
                let _ = fs::remove_file(&dest);
            }
            if let Some(task) = existing.iter().find(|task| task.dest == dest) {
                match task.status {
                    DownloadStatus::Failed => {
                        self.resume_download(task.id);
                        retried += 1;
                        skipped += 1;
                        continue;
                    }
                    DownloadStatus::Completed => {
                        self.remove_download(task.id);
                    }
                    _ => {
                        skipped += 1;
                        continue;
                    }
                }
            }
            let spec = DownloadSpec {
                title: format!("Runtime · {}", component.name),
                url: component.url.clone(),
                dest,
                bytes_total: component.size_bytes,
                sha256: Some(component.sha256.clone()),
            };
            match self.queue_download(spec) {
                Ok(_) => queued += 1,
                Err(error) => {
                    failures.push(error);
                    break;
                }
            }
        }

        self.runtime_message = Some(if failures.is_empty() {
            format!(
                "Queued {queued}, retried {retried}, skipped {skipped}. Install runtime pack when downloads finish."
            )
        } else {
            format!("Download queue failed: {}", failures.join("; "))
        });
    }

    pub(crate) fn open_files(&mut self, imp: bool) {
        self.file_purpose = if imp {
            FilePurpose::Import
        } else {
            FilePurpose::StartupExe
        };
        self.refresh_files_access();
        self.nav(files_root());
        self.go(S::Files);
    }

    /// Clear transient cache files (download .part files and stale bridge
    /// shortcuts) without touching the library or installed runtime.
    pub(crate) fn clear_cache(&mut self) {
        use gamehubx_download::part_path;
        let mut removed = 0usize;
        if let Some(store) = &self.store {
            if let Ok(tasks) = store.downloads() {
                for task in tasks {
                    let part = part_path(&task.dest);
                    if part.is_file() && fs::remove_file(&part).is_ok() {
                        removed += 1;
                    }
                }
            }
        }
        let cache = data_dir().join("cache");
        if cache.is_dir() {
            for entry in fs::read_dir(&cache).into_iter().flatten().flatten() {
                if entry.path().is_file() && fs::remove_file(entry.path()).is_ok() {
                    removed += 1;
                }
            }
        }
        self.settings_notice = Some(format!("Cleared {removed} cached file(s)"));
    }

    pub(crate) fn open_cover_picker(&mut self) {
        self.file_purpose = FilePurpose::CoverImage;
        self.refresh_files_access();
        self.nav(files_root());
        self.go(S::Files);
    }

    pub(crate) fn open_backup_picker(&mut self) {
        self.file_purpose = FilePurpose::BackupArchive;
        self.refresh_files_access();
        self.nav(files_root());
        self.go(S::Files);
    }

    pub(crate) fn back(&mut self) {
        if self.s == S::Session {
            self.stop_session();
            if let Some(previous) = self.stack.pop() {
                self.s = previous;
            } else {
                self.s = S::Detail;
            }
        } else if self.modal {
            self.modal = false;
        } else if self.popup {
            self.popup = false;
        } else if self.drawer {
            self.drawer = false;
        } else if self.s == S::Files && self.path != files_root() {
            let mut p = self.path.clone();
            p.pop();
            self.nav(p);
        } else if let Some(p) = self.stack.pop() {
            self.s = p;
        }
    }

    pub(crate) fn bar(&mut self, ui: &mut Ui, title: &str) {
        let title = crate::i18n::tr(title);
        crate::focus::set_suppress(true);
        let dash = self.s == S::Dash;
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space(40.0);
            if dash {
                if btn(ui, "≡", vec2(56.0, 56.0), CLEAR, W, 36.0).clicked() {
                    self.drawer = true;
                }
                chip(ui, "LB");
                ui.label(RichText::new(title).size(28.0).strong());
                chip(ui, "RB");
            } else {
                if btn(ui, "←", vec2(56.0, 56.0), CLEAR, W, 36.0).clicked() {
                    self.back();
                }
                ui.label(RichText::new(title).size(28.0).strong());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(40.0);
                ui.label(RichText::new(clock()).size(24.0).strong());
                ui.label(RichText::new(battery_text()).size(24.0).strong());
                ui.label("📶");
                if dash && btn(ui, "🔍", vec2(56.0, 56.0), CLEAR, W, 30.0).clicked() {
                    self.go(S::Search);
                }
            });
        });
        crate::focus::set_suppress(false);
    }

    pub(crate) fn tabs(&mut self, ui: &mut Ui, mgmt: bool) {
        ui.horizontal(|ui| {
            ui.add_space(46.0);
            if btn(
                ui,
                "Download Task",
                vec2(250.0, 62.0),
                if mgmt { CLEAR } else { CARD },
                if mgmt { DIM } else { W },
                24.0,
            )
            .clicked()
                && mgmt
            {
                self.s = S::Dl;
            }
            if btn(
                ui,
                "Game Management",
                vec2(300.0, 62.0),
                if mgmt { CARD } else { CLEAR },
                if mgmt { W } else { DIM },
                24.0,
            )
            .clicked()
                && !mgmt
            {
                self.s = S::Mgmt;
            }
        });
    }

    fn screen(&mut self, ui: &mut Ui) {
        match self.s {
            S::Dash => screens::dashboard::show(self, ui),
            S::Search => screens::search::show(self, ui),
            S::Pc => screens::pc_library::show(self, ui),
            S::Dl => screens::downloads::show(self, ui),
            S::Mgmt => screens::management::show(self, ui),
            S::Files => screens::files::show(self, ui),
            S::Detail => screens::detail::show(self, ui),
            S::Set => screens::game_settings::show(self, ui),
            S::App => screens::app_settings::show(self, ui),
            S::Session => screens::session::show(self, ui),
            S::Runtime => screens::runtime::show(self, ui),
            S::Touch => screens::touch::show(self, ui),
            S::Diag => screens::diag::show(self, ui),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        fit_design(ctx);
        paint_background(ctx);
        let first_focus = crate::focus::take_first();
        if cfg!(target_os = "android") && ctx.memory(|memory| memory.focused().is_none()) {
            if let Some(id) = first_focus {
                ctx.memory_mut(|memory| memory.request_focus(id));
            }
        }
        let language = crate::i18n::current();
        if language != self.font_lang {
            install_fonts(ctx, language == crate::i18n::Lang::ZhHans);
            self.font_lang = language;
        }
        if !self.files_access && self.files_access_at.elapsed() >= Duration::from_millis(500) {
            self.refresh_files_access();
        }
        #[cfg(target_os = "android")]
        {
            let action = gamehubx_platform::poll_gamepad_action();
            if action != 0 {
                self.handle_gamepad(action);
            }
        }
        self.poll_runtime_install();
        if self.s == S::Dl {
            self.refresh_downloads();
        }
        let downloads_active = self.s == S::Dl && self.downloads_active();
        let repaint = if self.runtime_installing {
            Duration::from_millis(250)
        } else if downloads_active {
            Duration::from_millis(500)
        } else if self.s == S::Diag {
            Duration::from_secs(1)
        } else if self.s == S::Session && self.session.is_some() {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(15)
        };
        ctx.request_repaint_after(repaint);
        ctx.style_mut(|style| {
            style.visuals.override_text_color = Some(W);
            style.spacing.item_spacing = vec2(20.0, 14.0);
        });
        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(Key::Escape) || i.key_pressed(Key::B)) {
                self.back();
            }
            if ctx.input(|i| i.key_pressed(Key::M)) {
                self.drawer = true;
            }
            if ctx.input(|i| i.key_pressed(Key::Y)) && self.s == S::Dash {
                self.go(S::Search);
            }
        }
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| self.screen(ui));
        overlays::layers(self, ctx);
        let hints_for: Vec<(&str, &str)> = if cfg!(target_os = "android") {
            match self.s {
                S::Dash => vec![
                    ("↕", "Navigate"),
                    ("OK", "Select"),
                    ("B", "Back"),
                    ("Y", "Search"),
                ],
                _ => vec![("↕", "Navigate"), ("OK", "Select"), ("B", "Back")],
            }
        } else {
            match self.s {
                S::Dash => vec![("Y", "Search"), ("M", "Menu")],
                S::Files => vec![("A", "Select"), ("B", "Back")],
                _ => vec![("B", "Back")],
            }
        };
        hints(ctx, &hints_for);
    }
}
