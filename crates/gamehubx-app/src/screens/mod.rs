pub(crate) mod app_settings;
pub(crate) mod dashboard;
pub(crate) mod detail;
pub(crate) mod diag;
pub(crate) mod downloads;
pub(crate) mod files;
pub(crate) mod gallery;
pub(crate) mod game_settings;
pub(crate) mod management;
pub(crate) mod pc_library;
pub(crate) mod runtime;
pub(crate) mod search;
pub(crate) mod session;
pub(crate) mod touch;

// The HUD lives at src/hud.rs (a top-level module physically) but is declared
// here so it compiles without touching lib.rs. Host integrations can call
// `crate::screens::hud::draw(app, ctx)` from app.rs when a session is active.
#[path = "../hud.rs"]
pub(crate) mod hud;
