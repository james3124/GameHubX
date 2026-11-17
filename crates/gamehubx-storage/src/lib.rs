use std::path::{Path, PathBuf};

use gamehubx_core::{Game, GameId, GameKind, GameSettings};
use gamehubx_download::{DownloadSpec, DownloadStatus, DownloadTask};
use rusqlite::{params, Connection, OptionalExtension};

const GAME_COLUMNS: &str =
    "id, kind, label, intro, exe_path, cover_path, container_id, favorite, created_at, last_played";

const DOWNLOAD_COLUMNS: &str =
    "id, title, url, dest, bytes_done, bytes_total, status, sha256, error, created_at, \
     completed_at, group_id, priority, auto_unzip, silent_install, sub_files_json";

pub struct LibraryStore {
    conn: Connection,
}

impl LibraryStore {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA busy_timeout=5000;
             PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA cache_size=-4096;
             PRAGMA temp_store=FILE;
             PRAGMA mmap_size=0;
             CREATE TABLE IF NOT EXISTS games (
                 id INTEGER PRIMARY KEY,
                 kind TEXT NOT NULL,
                 label TEXT NOT NULL,
                 intro TEXT NOT NULL DEFAULT '',
                 exe_path TEXT,
                 cover_path TEXT,
                 container_id TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0,
                 created_at INTEGER NOT NULL,
                 last_played INTEGER
             );
             CREATE INDEX IF NOT EXISTS games_label ON games(label COLLATE NOCASE);
             CREATE TABLE IF NOT EXISTS game_settings (
                 game_id INTEGER PRIMARY KEY,
                 width INTEGER NOT NULL,
                 height INTEGER NOT NULL,
                 fps_cap INTEGER NOT NULL,
                 dxvk_version TEXT NOT NULL,
                 audio_backend TEXT NOT NULL,
                 touch_profile TEXT NOT NULL,
                 memory_limit_mb INTEGER NOT NULL DEFAULT 0,
                 cpu_affinity_big INTEGER NOT NULL DEFAULT 0,
                 container_language TEXT NOT NULL DEFAULT 'en_US',
                 mangohud INTEGER NOT NULL DEFAULT 0,
                 directx_hud TEXT NOT NULL DEFAULT 'Disable',
                 disable_wm INTEGER NOT NULL DEFAULT 0,
                 env_vars_json TEXT NOT NULL DEFAULT '[]',
                 command_line TEXT NOT NULL DEFAULT '',
                 auto_keyboard INTEGER NOT NULL DEFAULT 1,
                 disable_gfx_plugin INTEGER NOT NULL DEFAULT 0,
                 compat_layer TEXT NOT NULL DEFAULT 'proton10.0-arm64x-2',
                 translation_params TEXT NOT NULL DEFAULT 'Game Presets',
                 dinput_library TEXT NOT NULL DEFAULT 'Prefer Native',
                 gpu_driver TEXT NOT NULL DEFAULT 'System Driver',
                 surface_format TEXT NOT NULL DEFAULT 'RGBA8',
                 vkd3d_version TEXT NOT NULL DEFAULT 'Default',
                 log_server INTEGER NOT NULL DEFAULT 0,
                 wine_debug_params TEXT NOT NULL DEFAULT '-all',
                 cpu_core_limit INTEGER NOT NULL DEFAULT 0,
                 overlay_opacity INTEGER NOT NULL DEFAULT 100,
                 rts_gestures INTEGER NOT NULL DEFAULT 0,
                 config_preset TEXT NOT NULL DEFAULT 'Balanced'
             );
             CREATE TABLE IF NOT EXISTS downloads (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 title TEXT NOT NULL,
                 url TEXT NOT NULL,
                 dest TEXT NOT NULL,
                 bytes_done INTEGER NOT NULL DEFAULT 0,
                 bytes_total INTEGER NOT NULL DEFAULT 0,
                 status TEXT NOT NULL,
                 sha256 TEXT,
                 error TEXT,
                 created_at INTEGER NOT NULL,
                 completed_at INTEGER,
                 group_id TEXT,
                 priority INTEGER NOT NULL DEFAULT 0,
                 auto_unzip INTEGER NOT NULL DEFAULT 0,
                 silent_install INTEGER NOT NULL DEFAULT 0,
                 sub_files_json TEXT NOT NULL DEFAULT '[]'
             );
             CREATE INDEX IF NOT EXISTS downloads_status ON downloads(status, created_at);
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version=6;",
        )?;
        if !has_column(&conn, "game_settings", "memory_limit_mb")? {
            conn.execute_batch(
                "ALTER TABLE game_settings ADD COLUMN memory_limit_mb INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !has_column(&conn, "game_settings", "cpu_affinity_big")? {
            conn.execute_batch(
                "ALTER TABLE game_settings ADD COLUMN cpu_affinity_big INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        // Extended per-game settings migration. Guard on a sentinel column so
        // an old database gains every new column exactly once.
        if !has_column(&conn, "game_settings", "container_language")? {
            conn.execute_batch(
                "ALTER TABLE game_settings ADD COLUMN container_language TEXT NOT NULL DEFAULT 'en_US';
                 ALTER TABLE game_settings ADD COLUMN mangohud INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN directx_hud TEXT NOT NULL DEFAULT 'Disable';
                 ALTER TABLE game_settings ADD COLUMN disable_wm INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN env_vars_json TEXT NOT NULL DEFAULT '[]';
                 ALTER TABLE game_settings ADD COLUMN command_line TEXT NOT NULL DEFAULT '';
                 ALTER TABLE game_settings ADD COLUMN auto_keyboard INTEGER NOT NULL DEFAULT 1;
                 ALTER TABLE game_settings ADD COLUMN disable_gfx_plugin INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN compat_layer TEXT NOT NULL DEFAULT 'proton10.0-arm64x-2';
                 ALTER TABLE game_settings ADD COLUMN translation_params TEXT NOT NULL DEFAULT 'Game Presets';
                 ALTER TABLE game_settings ADD COLUMN dinput_library TEXT NOT NULL DEFAULT 'Prefer Native';
                 ALTER TABLE game_settings ADD COLUMN gpu_driver TEXT NOT NULL DEFAULT 'System Driver';
                 ALTER TABLE game_settings ADD COLUMN surface_format TEXT NOT NULL DEFAULT 'RGBA8';
                 ALTER TABLE game_settings ADD COLUMN vkd3d_version TEXT NOT NULL DEFAULT 'Default';
                 ALTER TABLE game_settings ADD COLUMN log_server INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN wine_debug_params TEXT NOT NULL DEFAULT '-all';
                 ALTER TABLE game_settings ADD COLUMN cpu_core_limit INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN overlay_opacity INTEGER NOT NULL DEFAULT 100;
                 ALTER TABLE game_settings ADD COLUMN rts_gestures INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE game_settings ADD COLUMN config_preset TEXT NOT NULL DEFAULT 'Balanced';",
            )?;
        }
        // Download grouping / priority migration (user_version 6). Each column
        // is guarded so an interrupted migration can safely resume.
        if !has_column(&conn, "downloads", "group_id")? {
            conn.execute_batch("ALTER TABLE downloads ADD COLUMN group_id TEXT;")?;
        }
        if !has_column(&conn, "downloads", "priority")? {
            conn.execute_batch(
                "ALTER TABLE downloads ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !has_column(&conn, "downloads", "auto_unzip")? {
            conn.execute_batch(
                "ALTER TABLE downloads ADD COLUMN auto_unzip INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !has_column(&conn, "downloads", "silent_install")? {
            conn.execute_batch(
                "ALTER TABLE downloads ADD COLUMN silent_install INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
        if !has_column(&conn, "downloads", "sub_files_json")? {
            conn.execute_batch(
                "ALTER TABLE downloads ADD COLUMN sub_files_json TEXT NOT NULL DEFAULT '[]';",
            )?;
        }
        Ok(Self { conn })
    }

    pub fn upsert(&self, game: &Game) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO games
                 (id, kind, label, intro, exe_path, cover_path, container_id, favorite, created_at, last_played)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                 kind = excluded.kind,
                 label = excluded.label,
                 intro = excluded.intro,
                 exe_path = excluded.exe_path,
                 cover_path = excluded.cover_path,
                 container_id = excluded.container_id,
                 favorite = excluded.favorite,
                 last_played = excluded.last_played",
            params![
                game.id.0 as i64,
                kind_to_str(game.kind),
                game.label,
                game.intro,
                path_to_db(game.exe.as_ref()),
                path_to_db(game.cover.as_ref()),
                game.container,
                game.favorite as i64,
                game.created_at,
                game.last_played,
            ],
        )?;
        Ok(())
    }

    pub fn list(&self) -> rusqlite::Result<Vec<Game>> {
        let sql = format!(
            "SELECT {GAME_COLUMNS} FROM games ORDER BY favorite DESC, label COLLATE NOCASE"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let games = statement.query_map([], row_to_game)?.collect();
        games
    }

    pub fn search(&self, query: &str) -> rusqlite::Result<Vec<Game>> {
        let sql = format!(
            "SELECT {GAME_COLUMNS} FROM games
             WHERE label LIKE ?1 ESCAPE '\\' OR intro LIKE ?1 ESCAPE '\\'
             ORDER BY favorite DESC, label COLLATE NOCASE"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let games = statement
            .query_map([like_pattern(query)], row_to_game)?
            .collect();
        games
    }

    pub fn delete(&self, id: GameId) -> rusqlite::Result<bool> {
        self.conn.execute(
            "DELETE FROM game_settings WHERE game_id = ?1",
            [id.0 as i64],
        )?;
        let changed = self
            .conn
            .execute("DELETE FROM games WHERE id = ?1", [id.0 as i64])?;
        Ok(changed > 0)
    }

    pub fn update_exe(&self, id: GameId, exe: Option<&Path>) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE games SET exe_path = ?1 WHERE id = ?2",
            params![
                exe.map(|path| path.to_string_lossy().to_string()),
                id.0 as i64
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn set_cover(&self, id: GameId, cover: Option<&Path>) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE games SET cover_path = ?1 WHERE id = ?2",
            params![
                cover.map(|path| path.to_string_lossy().to_string()),
                id.0 as i64
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn settings(&self, id: GameId) -> rusqlite::Result<GameSettings> {
        self.conn
            .query_row(
                "SELECT width, height, fps_cap, dxvk_version, audio_backend, touch_profile,
                        memory_limit_mb, cpu_affinity_big, container_language, mangohud,
                        directx_hud, disable_wm, env_vars_json, command_line, auto_keyboard,
                        disable_gfx_plugin, compat_layer, translation_params, dinput_library,
                        gpu_driver, surface_format, vkd3d_version, log_server, wine_debug_params,
                        cpu_core_limit, overlay_opacity, rts_gestures, config_preset
                 FROM game_settings WHERE game_id = ?1",
                [id.0 as i64],
                |row| {
                    let env_json: String = row.get(12)?;
                    let env_vars = serde_json::from_str::<Vec<(String, String)>>(&env_json)
                        .unwrap_or_default();
                    Ok(GameSettings {
                        width: row.get::<_, i64>(0)? as u32,
                        height: row.get::<_, i64>(1)? as u32,
                        fps_cap: row.get::<_, i64>(2)? as u32,
                        dxvk_version: row.get(3)?,
                        audio_backend: row.get(4)?,
                        touch_profile: row.get(5)?,
                        memory_limit_mb: row.get::<_, i64>(6)? as u32,
                        cpu_affinity_big: row.get::<_, i64>(7)? != 0,
                        container_language: row.get(8)?,
                        mangohud: row.get::<_, i64>(9)? != 0,
                        directx_hud: row.get(10)?,
                        disable_wm: row.get::<_, i64>(11)? != 0,
                        env_vars,
                        command_line: row.get(13)?,
                        auto_keyboard: row.get::<_, i64>(14)? != 0,
                        disable_gfx_plugin: row.get::<_, i64>(15)? != 0,
                        compat_layer: row.get(16)?,
                        translation_params: row.get(17)?,
                        dinput_library: row.get(18)?,
                        gpu_driver: row.get(19)?,
                        surface_format: row.get(20)?,
                        vkd3d_version: row.get(21)?,
                        log_server: row.get::<_, i64>(22)? != 0,
                        wine_debug_params: row.get(23)?,
                        cpu_core_limit: row.get::<_, i64>(24)? as u32,
                        overlay_opacity: row.get::<_, i64>(25)? as u8,
                        rts_gestures: row.get::<_, i64>(26)? != 0,
                        config_preset: row.get(27)?,
                    })
                },
            )
            .optional()
            .map(|value| value.unwrap_or_default())
    }

    pub fn save_settings(&self, id: GameId, settings: &GameSettings) -> rusqlite::Result<()> {
        let env_json = serde_json::to_string(&settings.env_vars).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO game_settings
                 (game_id, width, height, fps_cap, dxvk_version, audio_backend, touch_profile,
                  memory_limit_mb, cpu_affinity_big, container_language, mangohud, directx_hud,
                  disable_wm, env_vars_json, command_line, auto_keyboard, disable_gfx_plugin,
                  compat_layer, translation_params, dinput_library, gpu_driver, surface_format,
                  vkd3d_version, log_server, wine_debug_params, cpu_core_limit, overlay_opacity,
                  rts_gestures, config_preset)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                     ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)
             ON CONFLICT(game_id) DO UPDATE SET
                 width = excluded.width,
                 height = excluded.height,
                 fps_cap = excluded.fps_cap,
                 dxvk_version = excluded.dxvk_version,
                 audio_backend = excluded.audio_backend,
                 touch_profile = excluded.touch_profile,
                 memory_limit_mb = excluded.memory_limit_mb,
                 cpu_affinity_big = excluded.cpu_affinity_big,
                 container_language = excluded.container_language,
                 mangohud = excluded.mangohud,
                 directx_hud = excluded.directx_hud,
                 disable_wm = excluded.disable_wm,
                 env_vars_json = excluded.env_vars_json,
                 command_line = excluded.command_line,
                 auto_keyboard = excluded.auto_keyboard,
                 disable_gfx_plugin = excluded.disable_gfx_plugin,
                 compat_layer = excluded.compat_layer,
                 translation_params = excluded.translation_params,
                 dinput_library = excluded.dinput_library,
                 gpu_driver = excluded.gpu_driver,
                 surface_format = excluded.surface_format,
                 vkd3d_version = excluded.vkd3d_version,
                 log_server = excluded.log_server,
                 wine_debug_params = excluded.wine_debug_params,
                 cpu_core_limit = excluded.cpu_core_limit,
                 overlay_opacity = excluded.overlay_opacity,
                 rts_gestures = excluded.rts_gestures,
                 config_preset = excluded.config_preset",
            params![
                id.0 as i64,
                settings.width as i64,
                settings.height as i64,
                settings.fps_cap as i64,
                settings.dxvk_version,
                settings.audio_backend,
                settings.touch_profile,
                settings.memory_limit_mb as i64,
                settings.cpu_affinity_big as i64,
                settings.container_language,
                settings.mangohud as i64,
                settings.directx_hud,
                settings.disable_wm as i64,
                env_json,
                settings.command_line,
                settings.auto_keyboard as i64,
                settings.disable_gfx_plugin as i64,
                settings.compat_layer,
                settings.translation_params,
                settings.dinput_library,
                settings.gpu_driver,
                settings.surface_format,
                settings.vkd3d_version,
                settings.log_server as i64,
                settings.wine_debug_params,
                settings.cpu_core_limit as i64,
                settings.overlay_opacity as i64,
                settings.rts_gestures as i64,
                settings.config_preset,
            ],
        )?;
        Ok(())
    }

    pub fn insert_download(&self, spec: &DownloadSpec, created_at: i64) -> rusqlite::Result<i64> {
        let sub_files_json = serde_json::to_string(&spec.sub_files).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO downloads
                 (title, url, dest, bytes_done, bytes_total, status, sha256, error,
                  created_at, completed_at, group_id, priority, auto_unzip, silent_install,
                  sub_files_json)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, NULL, ?7, NULL, ?8, ?9, ?10, ?11, ?12)",
            params![
                spec.title,
                spec.url,
                spec.dest.to_string_lossy().to_string(),
                spec.bytes_total as i64,
                DownloadStatus::Queued.as_str(),
                spec.sha256,
                created_at,
                spec.group_id,
                spec.priority,
                spec.auto_unzip as i64,
                spec.silent_install as i64,
                sub_files_json,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn downloads(&self) -> rusqlite::Result<Vec<DownloadTask>> {
        let sql =
            format!("SELECT {DOWNLOAD_COLUMNS} FROM downloads ORDER BY created_at DESC, id DESC");
        let mut statement = self.conn.prepare(&sql)?;
        let tasks = statement.query_map([], row_to_download)?.collect();
        tasks
    }

    pub fn download_by_id(&self, id: i64) -> rusqlite::Result<Option<DownloadTask>> {
        let sql = format!("SELECT {DOWNLOAD_COLUMNS} FROM downloads WHERE id = ?1");
        self.conn.query_row(&sql, [id], row_to_download).optional()
    }

    pub fn next_queued_download(&self) -> rusqlite::Result<Option<DownloadTask>> {
        let sql = format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads
             WHERE status = ?1 ORDER BY created_at, id LIMIT 1"
        );
        self.conn
            .query_row(&sql, [DownloadStatus::Queued.as_str()], row_to_download)
            .optional()
    }

    /// Returns every task that belongs to `group_id`, ordered by descending
    /// priority and then creation time.
    pub fn downloads_in_group(&self, group_id: &str) -> rusqlite::Result<Vec<DownloadTask>> {
        let sql = format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads
             WHERE group_id = ?1 ORDER BY priority DESC, created_at, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let tasks = statement.query_map([group_id], row_to_download)?.collect();
        tasks
    }

    /// Returns the sibling tasks of `parent_id`: every task in the parent's
    /// group except the parent itself. A parent without an explicit group
    /// uses its own id (as text) as the group key.
    pub fn children_of(&self, parent_id: i64) -> rusqlite::Result<Vec<DownloadTask>> {
        let parent = match self.download_by_id(parent_id)? {
            Some(task) => task,
            None => return Ok(Vec::new()),
        };
        let group = parent
            .group_id
            .clone()
            .unwrap_or_else(|| parent.id.to_string());
        let sql = format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads
             WHERE group_id = ?1 AND id != ?2
             ORDER BY priority DESC, created_at, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let tasks = statement
            .query_map(params![group, parent_id], row_to_download)?
            .collect();
        tasks
    }

    /// Updates the scheduling priority of a task. Higher values are dequeued
    /// first by `next_queued_by_priority`.
    pub fn set_priority(&self, id: i64, priority: i32) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE downloads SET priority = ?1 WHERE id = ?2",
            params![priority, id],
        )?;
        Ok(())
    }

    /// Returns the next queued task ordered by descending priority, then by
    /// creation time (oldest first) as a stable tie-breaker.
    pub fn next_queued_by_priority(&self) -> rusqlite::Result<Option<DownloadTask>> {
        let sql = format!(
            "SELECT {DOWNLOAD_COLUMNS} FROM downloads
             WHERE status = ?1 ORDER BY priority DESC, created_at, id LIMIT 1"
        );
        self.conn
            .query_row(&sql, [DownloadStatus::Queued.as_str()], row_to_download)
            .optional()
    }

    pub fn update_download_progress(
        &self,
        id: i64,
        bytes_done: u64,
        bytes_total: u64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE downloads SET bytes_done = ?1, bytes_total = ?2 WHERE id = ?3",
            params![bytes_done as i64, bytes_total as i64, id],
        )?;
        Ok(())
    }

    pub fn set_download_status(
        &self,
        id: i64,
        status: DownloadStatus,
        error: Option<&str>,
        completed_at: Option<i64>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE downloads
             SET status = ?1, error = ?2, completed_at = ?3
             WHERE id = ?4",
            params![status.as_str(), error, completed_at, id],
        )?;
        Ok(())
    }

    pub fn start_queued_download(&self, id: i64) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE downloads SET status = ?1, error = NULL, completed_at = NULL
             WHERE id = ?2 AND status = ?3",
            params![
                DownloadStatus::Downloading.as_str(),
                id,
                DownloadStatus::Queued.as_str()
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn pause_active_download(&self, id: i64) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE downloads SET status = ?1, error = NULL, completed_at = NULL
             WHERE id = ?2 AND status = ?3",
            params![
                DownloadStatus::Paused.as_str(),
                id,
                DownloadStatus::Downloading.as_str()
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn requeue_download(&self, id: i64) -> rusqlite::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE downloads SET status = ?1, error = NULL, completed_at = NULL
             WHERE id = ?2 AND status IN (?3, ?4)",
            params![
                DownloadStatus::Queued.as_str(),
                id,
                DownloadStatus::Paused.as_str(),
                DownloadStatus::Failed.as_str()
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_download(&self, id: i64) -> rusqlite::Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(changed > 0)
    }

    pub fn clear_completed_downloads(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "DELETE FROM downloads WHERE status = ?1",
            [DownloadStatus::Completed.as_str()],
        )
    }

    pub fn reset_interrupted_downloads(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE downloads SET status = ?1 WHERE status = ?2",
            params![
                DownloadStatus::Queued.as_str(),
                DownloadStatus::Downloading.as_str()
            ],
        )
    }

    pub fn setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
    }

    pub fn save_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn row_to_download(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadTask> {
    let sub_files_json: String = row.get(15)?;
    let sub_files = serde_json::from_str(&sub_files_json).unwrap_or_default();
    Ok(DownloadTask {
        id: row.get(0)?,
        title: row.get(1)?,
        url: row.get(2)?,
        dest: PathBuf::from(row.get::<_, String>(3)?),
        bytes_done: row.get::<_, i64>(4)? as u64,
        bytes_total: row.get::<_, i64>(5)? as u64,
        status: DownloadStatus::from_str(&row.get::<_, String>(6)?),
        sha256: row.get(7)?,
        error: row.get(8)?,
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
        group_id: row.get(11)?,
        priority: row.get::<_, i64>(12)? as i32,
        auto_unzip: row.get::<_, i64>(13)? != 0,
        silent_install: row.get::<_, i64>(14)? != 0,
        sub_files,
    })
}

fn row_to_game(row: &rusqlite::Row<'_>) -> rusqlite::Result<Game> {
    Ok(Game {
        id: GameId(row.get::<_, i64>(0)? as u64),
        kind: kind_from_str(&row.get::<_, String>(1)?),
        label: row.get(2)?,
        intro: row.get(3)?,
        exe: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
        cover: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
        container: row.get(6)?,
        favorite: row.get::<_, i64>(7)? != 0,
        created_at: row.get(8)?,
        last_played: row.get(9)?,
    })
}

fn path_to_db(path: Option<&PathBuf>) -> Option<String> {
    path.map(|value| value.to_string_lossy().to_string())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn kind_to_str(kind: GameKind) -> &'static str {
    match kind {
        GameKind::PcExe => "pc_exe",
        GameKind::Steam => "steam",
        GameKind::Android => "android",
    }
}

fn kind_from_str(value: &str) -> GameKind {
    match value {
        "pc_exe" => GameKind::PcExe,
        "steam" => GameKind::Steam,
        _ => GameKind::Android,
    }
}

fn like_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for ch in query.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(ch);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(label: &str, exe: &str) -> Game {
        let mut game = Game::new_pc_exe(label, "", PathBuf::from(exe));
        game.intro = format!("{label} intro");
        game
    }

    #[test]
    fn roundtrip_and_update() {
        let store = LibraryStore::open_in_memory().unwrap();
        let mut entry = game("Demo", "/games/demo.exe");
        entry.favorite = true;
        store.upsert(&entry).unwrap();

        let loaded = store.list().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, entry.id);
        assert_eq!(loaded[0].label, "Demo");
        assert_eq!(loaded[0].intro, "Demo intro");
        assert_eq!(loaded[0].exe.as_deref(), Some(Path::new("/games/demo.exe")));
        assert!(loaded[0].favorite);

        entry.label = "Demo Renamed".into();
        store.upsert(&entry).unwrap();
        let loaded = store.list().unwrap();
        assert_eq!(loaded[0].label, "Demo Renamed");
        assert_eq!(loaded[0].created_at, entry.created_at);
    }

    #[test]
    fn search_matches_and_escapes() {
        let store = LibraryStore::open_in_memory().unwrap();
        store.upsert(&game("Alpha Quest", "/g/a.exe")).unwrap();
        store.upsert(&game("100% Game", "/g/b.exe")).unwrap();
        store.upsert(&game("Save_Me", "/g/c.exe")).unwrap();

        assert_eq!(store.search("alpha").unwrap().len(), 1);
        assert_eq!(store.search("100%").unwrap().len(), 1);
        assert_eq!(store.search("_").unwrap().len(), 1);
        assert!(store.search("SaveXMe").unwrap().is_empty());
    }

    #[test]
    fn delete_and_update_exe() {
        let store = LibraryStore::open_in_memory().unwrap();
        let entry = game("Demo", "/g/demo.exe");
        store.upsert(&entry).unwrap();
        store
            .save_settings(entry.id, &GameSettings::default())
            .unwrap();

        assert!(store
            .update_exe(entry.id, Some(Path::new("/g/other.exe")))
            .unwrap());
        let loaded = store.list().unwrap();
        assert_eq!(loaded[0].exe.as_deref(), Some(Path::new("/g/other.exe")));

        assert!(store.delete(entry.id).unwrap());
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.settings(entry.id).unwrap(), GameSettings::default());
    }

    #[test]
    fn settings_default_and_roundtrip() {
        let store = LibraryStore::open_in_memory().unwrap();
        let id = GameId(9);
        assert_eq!(store.settings(id).unwrap(), GameSettings::default());

        let settings = GameSettings {
            width: 1920,
            height: 1080,
            fps_cap: 120,
            dxvk_version: "DXVK".into(),
            audio_backend: "Alsa".into(),
            touch_profile: "Keyboard & Mouse".into(),
            memory_limit_mb: 4096,
            cpu_affinity_big: true,
            container_language: "ja_JP".into(),
            mangohud: true,
            directx_hud: "Full".into(),
            disable_wm: true,
            env_vars: vec![("LIBGL_ALWAYS_SOFTWARE".into(), "1".into())],
            command_line: "-windowed".into(),
            auto_keyboard: false,
            disable_gfx_plugin: true,
            compat_layer: "wine10.0-arm64ec".into(),
            translation_params: "Box64 Optimized".into(),
            dinput_library: "Prefer Wine".into(),
            gpu_driver: "Turnip".into(),
            surface_format: "BGRA8".into(),
            vkd3d_version: "VKD3D".into(),
            log_server: true,
            wine_debug_params: "fixme-all".into(),
            cpu_core_limit: 4,
            overlay_opacity: 80,
            rts_gestures: true,
            config_preset: "Performance".into(),
        };
        store.save_settings(id, &settings).unwrap();
        assert_eq!(store.settings(id).unwrap(), settings);
    }

    #[test]
    fn favorite_sorts_first() {
        let store = LibraryStore::open_in_memory().unwrap();
        let mut zed = game("Zed", "/g/z.exe");
        zed.favorite = true;
        store.upsert(&game("Alpha", "/g/a.exe")).unwrap();
        store.upsert(&zed).unwrap();
        let labels: Vec<String> = store.list().unwrap().into_iter().map(|g| g.label).collect();
        assert_eq!(labels, ["Zed", "Alpha"]);
    }

    #[test]
    fn download_queue_roundtrip() {
        let store = LibraryStore::open_in_memory().unwrap();
        let spec = DownloadSpec {
            title: "runtime".into(),
            url: "https://example.com/runtime.tar.gz".into(),
            dest: PathBuf::from("/tmp/runtime.tar.gz"),
            bytes_total: 1024,
            sha256: Some("a".repeat(64)),
            ..Default::default()
        };
        let first = store.insert_download(&spec, 100).unwrap();
        let mut other = spec.clone();
        other.title = "second".into();
        let second = store.insert_download(&other, 200).unwrap();

        let tasks = store.downloads().unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, second);
        assert_eq!(tasks[0].status, DownloadStatus::Queued);
        assert_eq!(tasks[1].id, first);

        let next = store.next_queued_download().unwrap().unwrap();
        assert_eq!(next.id, first);
        store.update_download_progress(first, 512, 1024).unwrap();
        store
            .set_download_status(first, DownloadStatus::Downloading, None, None)
            .unwrap();
        assert!(store.pause_active_download(first).unwrap());
        assert_eq!(
            store.download_by_id(first).unwrap().unwrap().status,
            DownloadStatus::Paused
        );
        assert!(store.requeue_download(first).unwrap());
        assert_eq!(
            store.download_by_id(first).unwrap().unwrap().status,
            DownloadStatus::Queued
        );
        assert!(store.start_queued_download(first).unwrap());
        assert_eq!(
            store.download_by_id(first).unwrap().unwrap().status,
            DownloadStatus::Downloading
        );
        assert_eq!(store.reset_interrupted_downloads().unwrap(), 1);
        let paused = store.download_by_id(first).unwrap().unwrap();
        assert_eq!(paused.status, DownloadStatus::Queued);
        assert_eq!(paused.bytes_done, 512);

        store
            .set_download_status(first, DownloadStatus::Completed, None, Some(300))
            .unwrap();
        assert_eq!(store.clear_completed_downloads().unwrap(), 1);
        assert_eq!(store.downloads().unwrap().len(), 1);
        assert!(store.delete_download(second).unwrap());
        assert!(store.downloads().unwrap().is_empty());
    }

    #[test]
    fn setting_roundtrip() {
        let store = LibraryStore::open_in_memory().unwrap();
        assert_eq!(store.setting("language").unwrap(), None);
        store.save_setting("language", "zh-Hans").unwrap();
        assert_eq!(
            store.setting("language").unwrap().as_deref(),
            Some("zh-Hans")
        );
        store.save_setting("language", "en").unwrap();
        assert_eq!(store.setting("language").unwrap().as_deref(), Some("en"));
    }

    #[test]
    fn persists_across_reopen() {
        let path = std::env::temp_dir().join(format!(
            "gamehubx-storage-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let entry = game("Persisted", "/g/persist.exe");
        let spec = DownloadSpec {
            title: "Persisted download".into(),
            url: "https://example.com/persist.tar.gz".into(),
            dest: PathBuf::from("/tmp/persist.tar.gz"),
            bytes_total: 4096,
            sha256: None,
            group_id: Some("persist-group".into()),
            priority: 4,
            auto_unzip: true,
            silent_install: true,
            sub_files: vec!["one.bin".into(), "two.bin".into()],
        };
        {
            let store = LibraryStore::open(&path).unwrap();
            store.upsert(&entry).unwrap();
            store.insert_download(&spec, 500).unwrap();
        }
        {
            let store = LibraryStore::open(&path).unwrap();
            let loaded = store.list().unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].id, entry.id);
            assert_eq!(loaded[0].label, "Persisted");
            let downloads = store.downloads().unwrap();
            assert_eq!(downloads.len(), 1);
            assert_eq!(downloads[0].title, "Persisted download");
            assert_eq!(downloads[0].status, DownloadStatus::Queued);
            assert_eq!(downloads[0].group_id.as_deref(), Some("persist-group"));
            assert_eq!(downloads[0].priority, 4);
            assert!(downloads[0].auto_unzip);
            assert!(downloads[0].silent_install);
            assert_eq!(downloads[0].sub_files, ["one.bin", "two.bin"]);
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn migrates_downloads_columns_from_v5() {
        let path = std::env::temp_dir().join(format!(
            "gamehubx-migrate-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        {
            // Simulate a pre-migration (user_version 5) database.
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "PRAGMA user_version=5;
                 CREATE TABLE downloads (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     title TEXT NOT NULL, url TEXT NOT NULL, dest TEXT NOT NULL,
                     bytes_done INTEGER NOT NULL DEFAULT 0,
                     bytes_total INTEGER NOT NULL DEFAULT 0,
                     status TEXT NOT NULL, sha256 TEXT, error TEXT,
                     created_at INTEGER NOT NULL, completed_at INTEGER
                 );",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO downloads (title, url, dest, bytes_total, status, created_at)
                 VALUES ('legacy', 'https://example.com/legacy', '/tmp/legacy', 10, 'queued', 1)",
                [],
            )
            .unwrap();
        }
        {
            let store = LibraryStore::open(&path).unwrap();
            let legacy = store.download_by_id(1).unwrap().unwrap();
            assert_eq!(legacy.title, "legacy");
            assert_eq!(legacy.group_id, None);
            assert_eq!(legacy.priority, 0);
            assert!(!legacy.auto_unzip);
            assert!(!legacy.silent_install);
            assert!(legacy.sub_files.is_empty());

            let spec = DownloadSpec {
                title: "after-migration".into(),
                url: "https://example.com/new".into(),
                dest: PathBuf::from("/tmp/new"),
                group_id: Some("batch".into()),
                priority: 3,
                auto_unzip: true,
                silent_install: true,
                sub_files: vec!["part1".into(), "part2".into()],
                ..Default::default()
            };
            let id = store.insert_download(&spec, 2).unwrap();
            let loaded = store.download_by_id(id).unwrap().unwrap();
            assert_eq!(loaded.group_id.as_deref(), Some("batch"));
            assert_eq!(loaded.priority, 3);
            assert!(loaded.auto_unzip);
            assert!(loaded.silent_install);
            assert_eq!(loaded.sub_files, ["part1", "part2"]);
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn priority_and_group_queries() {
        let store = LibraryStore::open_in_memory().unwrap();
        let spec = |title: &str, group: Option<&str>, priority: i32| DownloadSpec {
            title: title.into(),
            url: format!("https://example.com/{title}"),
            dest: PathBuf::from(format!("/tmp/{title}")),
            group_id: group.map(str::to_string),
            priority,
            ..Default::default()
        };
        let a = store.insert_download(&spec("a", Some("g"), 1), 10).unwrap();
        let b = store.insert_download(&spec("b", Some("g"), 5), 20).unwrap();
        let c = store.insert_download(&spec("c", None, 9), 30).unwrap();

        // Legacy FIFO queue is unchanged.
        assert_eq!(store.next_queued_download().unwrap().unwrap().id, a);
        // Priority queue prefers the highest priority.
        assert_eq!(store.next_queued_by_priority().unwrap().unwrap().id, c);

        let group = store.downloads_in_group("g").unwrap();
        assert_eq!(group.len(), 2);
        assert_eq!(group[0].id, b);
        assert_eq!(group[1].id, a);

        let children = store.children_of(a).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, b);
        assert!(store.children_of(c).unwrap().is_empty());
        assert!(store.children_of(9999).unwrap().is_empty());

        // Equal priorities fall back to creation time (oldest first).
        let d = store.insert_download(&spec("d", None, 9), 5).unwrap();
        assert_eq!(store.next_queued_by_priority().unwrap().unwrap().id, d);

        store.set_priority(a, 100).unwrap();
        assert_eq!(store.download_by_id(a).unwrap().unwrap().priority, 100);
        assert_eq!(store.next_queued_by_priority().unwrap().unwrap().id, a);
        assert_eq!(store.next_queued_download().unwrap().unwrap().id, a);
    }

    #[test]
    fn spec_serde_defaults_legacy_json() {
        let old =
            r#"{"title":"x","url":"https://e/x","dest":"/tmp/x","bytes_total":1,"sha256":null}"#;
        let spec: DownloadSpec = serde_json::from_str(old).unwrap();
        assert_eq!(spec.title, "x");
        assert_eq!(spec.group_id, None);
        assert_eq!(spec.priority, 0);
        assert!(!spec.auto_unzip);
        assert!(!spec.silent_install);
        assert!(spec.sub_files.is_empty());
        let json = serde_json::to_string(&spec).unwrap();
        let back: DownloadSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }
}
