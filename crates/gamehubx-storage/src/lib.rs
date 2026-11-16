use std::path::{Path, PathBuf};

use gamehubx_core::{Game, GameId, GameKind, GameSettings};
use gamehubx_download::{DownloadSpec, DownloadStatus, DownloadTask};
use rusqlite::{params, Connection, OptionalExtension};

const GAME_COLUMNS: &str =
    "id, kind, label, intro, exe_path, cover_path, container_id, favorite, created_at, last_played";

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
                 cpu_affinity_big INTEGER NOT NULL DEFAULT 0
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
                 completed_at INTEGER
             );
             CREATE INDEX IF NOT EXISTS downloads_status ON downloads(status, created_at);
             CREATE TABLE IF NOT EXISTS settings (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             PRAGMA user_version=5;",
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

    pub fn settings(&self, id: GameId) -> rusqlite::Result<GameSettings> {
        self.conn
            .query_row(
                "SELECT width, height, fps_cap, dxvk_version, audio_backend, touch_profile,
                        memory_limit_mb, cpu_affinity_big
                 FROM game_settings WHERE game_id = ?1",
                [id.0 as i64],
                |row| {
                    Ok(GameSettings {
                        width: row.get::<_, i64>(0)? as u32,
                        height: row.get::<_, i64>(1)? as u32,
                        fps_cap: row.get::<_, i64>(2)? as u32,
                        dxvk_version: row.get(3)?,
                        audio_backend: row.get(4)?,
                        touch_profile: row.get(5)?,
                        memory_limit_mb: row.get::<_, i64>(6)? as u32,
                        cpu_affinity_big: row.get::<_, i64>(7)? != 0,
                    })
                },
            )
            .optional()
            .map(|value| value.unwrap_or_default())
    }

    pub fn save_settings(&self, id: GameId, settings: &GameSettings) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO game_settings
                 (game_id, width, height, fps_cap, dxvk_version, audio_backend, touch_profile,
                  memory_limit_mb, cpu_affinity_big)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(game_id) DO UPDATE SET
                 width = excluded.width,
                 height = excluded.height,
                 fps_cap = excluded.fps_cap,
                 dxvk_version = excluded.dxvk_version,
                 audio_backend = excluded.audio_backend,
                 touch_profile = excluded.touch_profile,
                 memory_limit_mb = excluded.memory_limit_mb,
                 cpu_affinity_big = excluded.cpu_affinity_big",
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
            ],
        )?;
        Ok(())
    }

    pub fn insert_download(&self, spec: &DownloadSpec, created_at: i64) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO downloads
                 (title, url, dest, bytes_done, bytes_total, status, sha256, error,
                  created_at, completed_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, NULL, ?7, NULL)",
            params![
                spec.title,
                spec.url,
                spec.dest.to_string_lossy().to_string(),
                spec.bytes_total as i64,
                DownloadStatus::Queued.as_str(),
                spec.sha256,
                created_at,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn downloads(&self) -> rusqlite::Result<Vec<DownloadTask>> {
        let mut statement = self.conn.prepare(
            "SELECT id, title, url, dest, bytes_done, bytes_total, status, sha256,
                    error, created_at, completed_at
             FROM downloads ORDER BY created_at DESC, id DESC",
        )?;
        let tasks = statement.query_map([], row_to_download)?.collect();
        tasks
    }

    pub fn download_by_id(&self, id: i64) -> rusqlite::Result<Option<DownloadTask>> {
        self.conn
            .query_row(
                "SELECT id, title, url, dest, bytes_done, bytes_total, status, sha256,
                        error, created_at, completed_at
                 FROM downloads WHERE id = ?1",
                [id],
                row_to_download,
            )
            .optional()
    }

    pub fn next_queued_download(&self) -> rusqlite::Result<Option<DownloadTask>> {
        self.conn
            .query_row(
                "SELECT id, title, url, dest, bytes_done, bytes_total, status, sha256,
                        error, created_at, completed_at
                 FROM downloads WHERE status = ?1 ORDER BY created_at, id LIMIT 1",
                [DownloadStatus::Queued.as_str()],
                row_to_download,
            )
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
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
