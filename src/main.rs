//! Dr. Rocket: a fast, beautiful IDE for Racket.

mod app;
mod automation;
mod commands;
mod config;
mod editor;
mod fuzzy;
mod platform;
mod racket;
mod racket_ui;
mod terminal;
mod theme;
mod ui;
mod workspace;

use std::path::PathBuf;

fn main() -> eframe::Result {
    platform::install_open_handler();
    let open: Vec<PathBuf> = std::env::args().skip(1).filter(|a| !a.starts_with('-')).map(PathBuf::from).collect();
    let size = automation::Automation::window_size().unwrap_or([1480.0, 940.0]);
    let viewport = egui::ViewportBuilder::default()
        .with_title(ui::welcome::APP_NAME)
        .with_app_id("dev.moiz.drrocket")
        .with_inner_size(size)
        .with_min_inner_size([760.0, 480.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    let viewport = match ui::brand::window_icon() {
        Some(icon) => viewport.with_icon(icon),
        None => viewport,
    };
    // Automation windows float above everything so macOS keeps redrawing them.
    let automated = std::env::var("DRROCKET_AUTOMATION").is_ok();
    let viewport = if automated { viewport.with_window_level(egui::WindowLevel::AlwaysOnTop) } else { viewport };
    let options = eframe::NativeOptions {
        viewport,
        persist_window: !automated,
        ..Default::default()
    };
    eframe::run_native(ui::welcome::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
