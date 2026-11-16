use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
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
    RuntimeLayout, RuntimeManifest, SessionConfig, SessionPlan,
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

pub(crate) struct Tile {
    pub(crate) ini: String,
}

pub struct App {
    pub(crate) s: S,
    pub(crate) stack: Vec<S>,
    pub(crate) drawer: bool,
    pub(crate) modal: bool,
    pub(crate) popup: bool,
    pub(crate) imp: bool,
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
    pub(crate) tg: [bool; 4],
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
            imp: true,
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
            tg: [false, false, true, false],
        };
        app.rebuild_tiles();
        app
    }

    pub(crate) fn rebuild_tiles(&mut self) {
        let mut tiles = Vec::with_capacity(self.games.len() + 2);
        tiles.push(Tile { ini: "P".into() });
        tiles.push(Tile { ini: "S".into() });
        for game in &self.games {
            tiles.push(Tile {
                ini: game.label.chars().next().unwrap_or('?').to_string(),
            });
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
        self.files_access = files_access_granted();
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
        let id = game.id;
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
            match gamehubx_platform::launch_winlator_game(exe_text.as_ref(), &label) {
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
        let config = SessionConfig {
            layout: RuntimeLayout::new(data.join("runtime")),
            container: data.join("containers").join(format!("{:016x}", id.0)),
            game_dir,
            exe,
            fps_cap: settings.fps_cap,
            width: settings.width,
            height: settings.height,
            audio: self.audio_backend(),
            dxvk: settings.dxvk_version != "Default",
            memory_limit_mb: settings.memory_limit_mb,
            cpu_affinity,
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
            let _ = manager.repair(id, metadata);
        }

        let layout = plan.config().layout.clone();
        let container = plan.config().container.clone();
        if let Err(error) = apply_dxvk(&layout, &container, plan.config().dxvk) {
            self.session_error = Some(format!("DXVK apply failed: {error:?}"));
            self.go(S::Session);
            return;
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
        self.imp = imp;
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
        if !ctx.input(|input| input.focused) {
            ctx.request_repaint_after(Duration::from_secs(60));
            return;
        }
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
            .frame(Frame::none().fill(BG))
            .show(ctx, |ui| self.screen(ui));
        overlays::layers(self, ctx);
        let hints_for: Vec<(&str, &str)> = if cfg!(target_os = "android") {
            match self.s {
                S::Dash => vec![
                    ("↕", "Navigate"),
                    ("A", "Select"),
                    ("B", "Back"),
                    ("Y", "Search"),
                ],
                _ => vec![("↕", "Navigate"), ("A", "Select"), ("B", "Back")],
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
