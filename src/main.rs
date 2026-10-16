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
    let options = eframe::NativeOptions {
        viewport,
        persist_window: std::env::var("DRROCKET_AUTOMATION").is_err(),
        ..Default::default()
    };
    eframe::run_native(ui::welcome::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
