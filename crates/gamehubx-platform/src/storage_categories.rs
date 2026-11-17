//! Bounded, dependency-free scanner that summarizes storage usage by category.
//!
//! Files are classified solely by their (case-insensitive) extension. The walk
//! uses an explicit stack so it cannot overflow the call stack on deeply nested
//! trees, never follows symlinks (avoiding cycles), and caps the amount of work
//! it will perform. Any I/O error encountered while walking is ignored.

use std::fs;
use std::path::{Path, PathBuf};

/// The maximum number of filesystem entries examined in a single scan.
const MAX_VISITED: u32 = 200_000;

/// Byte totals for each storage category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StorageCategories {
    pub game_bytes: u64,
    pub media_bytes: u64,
    pub other_bytes: u64,
}

impl StorageCategories {
    pub fn total(&self) -> u64 {
        self.game_bytes + self.media_bytes + self.other_bytes
    }
}

#[derive(Clone, Copy)]
enum Category {
    Game,
    Media,
    Other,
}

fn classify(path: &Path) -> Category {
    let extension = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => ext.to_ascii_lowercase(),
        None => return Category::Other,
    };

    match extension.as_str() {
        "exe" | "msi" | "zip" | "tar" | "gz" | "zst" | "rar" | "7z" | "iso" | "img" | "apk"
        | "xapk" | "obb" => Category::Game,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "mp4" | "mkv" | "avi" | "mov" | "mp3"
        | "wav" | "flac" | "ogg" | "aac" | "m4a" => Category::Media,
        _ => Category::Other,
    }
}

/// Recursively summarize the bytes stored under `root`.
///
/// Symlinks are never followed, directories whose name starts with `.` are
/// skipped, and at most [`MAX_VISITED`] entries are examined. Errors reading
/// any directory or file are silently ignored.
pub fn scan_storage_categories(root: &Path) -> StorageCategories {
    let mut totals = StorageCategories::default();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut visited: u32 = 0;

    while let Some(path) = stack.pop() {
        if visited >= MAX_VISITED {
            break;
        }
        visited += 1;

        // symlink_metadata does not follow symlinks, so a symlink (including a
        // cycle) is reported as a symlink and skipped rather than traversed.
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_file() {
            let size = metadata.len();
            match classify(&path) {
                Category::Game => totals.game_bytes = totals.game_bytes.saturating_add(size),
                Category::Media => totals.media_bytes = totals.media_bytes.saturating_add(size),
                Category::Other => totals.other_bytes = totals.other_bytes.saturating_add(size),
            }
            continue;
        }

        if file_type.is_dir() {
            let entries = match fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };

                // DirEntry::file_type() does not follow symlinks.
                let child_type = match entry.file_type() {
                    Ok(child_type) => child_type,
                    Err(_) => continue,
                };
                if child_type.is_symlink() {
                    continue;
                }
                if child_type.is_dir() && entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }

                stack.push(entry.path());
            }
        }
    }

    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE: AtomicU64 = AtomicU64::new(0);

    /// A uniquely named temporary directory tree that is removed on drop.
    struct TempTree(PathBuf);

    impl TempTree {
        fn new(tag: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let counter = UNIQUE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "gamehubx-storage-categories-{tag}-{}-{nanos}-{counter}",
                std::process::id()
            ));
            // Clear any stale tree from a previous, interrupted run.
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create temp root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: &str, size: usize) {
            let target = self.0.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("create nested dir");
            }
            fs::write(&target, vec![b'x'; size]).expect("write fixed-size file");
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn categorizes_files_across_nested_directories() {
        let tree = TempTree::new("totals");

        // Game files (case-insensitive extensions, including a nested one).
        tree.write("games/game.exe", 100);
        tree.write("games/pack.ZIP", 200);
        tree.write("games/sub/level.7z", 10);
        // Media files.
        tree.write("media/photo.PNG", 50);
        tree.write("media/song.mp3", 75);
        // Everything else.
        tree.write("notes.txt", 30);
        tree.write("data.dat", 40);
        // A hidden directory must be ignored entirely.
        tree.write(".hidden/secret.png", 999);

        let totals = scan_storage_categories(tree.path());
        assert_eq!(totals.game_bytes, 310);
        assert_eq!(totals.media_bytes, 125);
        assert_eq!(totals.other_bytes, 70);
        assert_eq!(totals.total(), 505);
    }

    #[test]
    fn missing_root_returns_default() {
        let tree = TempTree::new("missing");
        let missing = tree.path().join("does-not-exist");

        assert_eq!(
            scan_storage_categories(&missing),
            StorageCategories::default()
        );
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let tree = TempTree::new("symlink");
        tree.write("real/audio.flac", 64);

        // A symlink to a directory and one to a file must both be ignored.
        symlink(tree.path().join("real"), tree.path().join("link_dir"))
            .expect("create dir symlink");
        symlink(
            tree.path().join("real/audio.flac"),
            tree.path().join("link_file.mp3"),
        )
        .expect("create file symlink");
        // A self-referential symlink would form a cycle if it were followed.
        symlink(tree.path().join("loop"), tree.path().join("loop")).expect("create loop");

        let totals = scan_storage_categories(tree.path());
        assert_eq!(totals.game_bytes, 0);
        assert_eq!(totals.media_bytes, 64);
        assert_eq!(totals.other_bytes, 0);
    }
}
