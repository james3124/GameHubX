use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use eframe::egui::Id;

static IDS: Mutex<Vec<Id>> = Mutex::new(Vec::new());
static SUPPRESS: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_suppress(suppress: bool) {
    SUPPRESS.store(suppress, Ordering::Relaxed);
}

pub(crate) fn register(id: Id) {
    if SUPPRESS.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut ids) = IDS.lock() {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
}

/// Returns the first focusable widget from the previous frame so controller
/// input has a starting point when nothing has focus yet.
pub(crate) fn take_first() -> Option<Id> {
    let mut ids = IDS.lock().ok()?;
    let first = ids.first().copied();
    ids.clear();
    first
}
