#[cfg(target_os = "android")]
mod android;
mod app;
mod download_queue;
mod focus;
mod i18n;
mod overlays;
mod screens;
mod theme;
mod widgets;

pub use app::App;

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1612.0, 720.0])
            .with_title("GameHubX"),
        ..Default::default()
    };
    eframe::run_native(
        "GameHubX",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    if let Err(err) = eframe::run_native(
        "GameHubX",
        options,
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    ) {
        log::error!("gamehubx: {err}");
    }
}
