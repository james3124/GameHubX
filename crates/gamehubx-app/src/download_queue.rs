use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use gamehubx_download::{download, part_path, DownloadError, DownloadSpec, DownloadStatus};
use gamehubx_storage::LibraryStore;

enum Signal {
    Wake,
    Stop,
}

pub(crate) struct DownloadQueue {
    tx: Sender<Signal>,
    cancel: Arc<AtomicBool>,
}

impl DownloadQueue {
    pub(crate) fn start(db_path: impl Into<PathBuf>, downloads_dir: impl Into<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let db_path = db_path.into();
        let downloads_dir = downloads_dir.into();
        thread::spawn(move || run(rx, worker_cancel, db_path, downloads_dir));
        Self { tx, cancel }
    }

    pub(crate) fn wake(&self) {
        let _ = self.tx.send(Signal::Wake);
    }

    pub(crate) fn cancel_current(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Drop for DownloadQueue {
    fn drop(&mut self) {
        let _ = self.tx.send(Signal::Stop);
    }
}

fn run(
    rx: mpsc::Receiver<Signal>,
    cancel: Arc<AtomicBool>,
    db_path: PathBuf,
    downloads_dir: PathBuf,
) {
    let store = match LibraryStore::open(&db_path) {
        Ok(store) => store,
        Err(error) => {
            log::error!("download queue: cannot open {}: {error}", db_path.display());
            return;
        }
    };
    if let Err(error) = fs::create_dir_all(&downloads_dir) {
        log::error!(
            "download queue: cannot create {}: {error}",
            downloads_dir.display()
        );
        return;
    }
    if let Err(error) = store.reset_interrupted_downloads() {
        log::warn!("download queue: cannot reset interrupted tasks: {error}");
    }

    while let Ok(signal) = rx.recv() {
        if matches!(signal, Signal::Stop) {
            return;
        }
        loop {
            let task = match store.next_queued_download() {
                Ok(Some(task)) => task,
                Ok(None) => break,
                Err(error) => {
                    log::error!("download queue: cannot list queued tasks: {error}");
                    break;
                }
            };
            cancel.store(false, Ordering::SeqCst);
            match store.start_queued_download(task.id) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    log::error!("download queue: cannot start task {}: {error}", task.id);
                    break;
                }
            }

            let spec = DownloadSpec {
                title: task.title.clone(),
                url: task.url.clone(),
                dest: task.dest.clone(),
                bytes_total: task.bytes_total,
                sha256: task.sha256.clone(),
            };
            let mut last_update = Instant::now();
            let result = download(&spec, &cancel, |done, total| {
                if last_update.elapsed() >= Duration::from_millis(250) {
                    let _ = store.update_download_progress(task.id, done, total);
                    last_update = Instant::now();
                }
            });

            match result {
                Ok(done) => {
                    let total = task.bytes_total.max(done);
                    match store.download_by_id(task.id) {
                        Ok(Some(_)) => {
                            let _ = store.update_download_progress(task.id, done, total);
                            let _ = store.set_download_status(
                                task.id,
                                DownloadStatus::Completed,
                                None,
                                Some(chrono::Utc::now().timestamp()),
                            );
                        }
                        _ => {
                            let _ = fs::remove_file(&task.dest);
                        }
                    }
                }
                Err(DownloadError::Cancelled) => {
                    if let Ok(Some(current)) = store.download_by_id(task.id) {
                        if current.status == DownloadStatus::Downloading {
                            let _ = store.set_download_status(
                                task.id,
                                DownloadStatus::Paused,
                                None,
                                None,
                            );
                        }
                    }
                }
                Err(error) => {
                    let _ = store.set_download_status(
                        task.id,
                        DownloadStatus::Failed,
                        Some(&error.to_string()),
                        None,
                    );
                }
            }
        }
    }
}

pub(crate) fn remove_files(dest: &Path) {
    let _ = fs::remove_file(dest);
    let _ = fs::remove_file(part_path(dest));
}
