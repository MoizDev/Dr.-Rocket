//! Dr. Rocket: a fast, beautiful IDE for Racket.

// Release builds on Windows are GUI apps (no console window); debug builds keep the console
// for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod automation;
mod commands;
mod config;
mod crash;
mod dev;
mod editor;
mod fuzzy;
mod platform;
mod print;
mod projects;
mod startup;
mod racket;
mod racket_ui;
mod terminal;
mod theme;
mod visualizer;
mod ui;
mod update;
mod workspace;

use std::path::PathBuf;

fn main() -> eframe::Result {
    // Report the version without starting the UI.
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", update::CURRENT);
        return Ok(());
    }
    crash::install();
    let renderer = startup::begin();
    platform::install_app_handlers();
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
    // Automation windows must never take focus or cover other apps: they open inactive and
    // behind normal windows (and invisible, see platform::setup_window). They keep painting
    // anyway (RequestPaintWhileHidden in app.rs), which is all screenshots need.
    let automated = std::env::var("DRROCKET_AUTOMATION").is_ok();
    let viewport = if automated { viewport.with_active(false).with_window_level(egui::WindowLevel::AlwaysOnBottom) } else { viewport };
    let options = eframe::NativeOptions {
        viewport,
        persist_window: !automated,
        // eframe defaults to a frame latency of 1, which on Metal halves the frame rate (60 -> 30
        // fps) as soon as a frame has real work in it. Queue two frames, as egui-wgpu recommends.
        // DRROCKET_NO_VSYNC renders unthrottled, for measuring (see DRROCKET_FRAME_STATS).
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            surface: eframe::egui_wgpu::SurfaceConfig {
                present_mode: if std::env::var_os("DRROCKET_NO_VSYNC").is_some() { eframe::wgpu::PresentMode::AutoNoVsync } else { eframe::wgpu::PresentMode::AutoVsync },
                // DRROCKET_FRAME_LATENCY=1: one queued frame, for measuring.
                // The software renderer is slow per frame: queue one, not two, so input isn't
                // a frame further behind.
                ..if renderer == startup::Renderer::Software || std::env::var("DRROCKET_FRAME_LATENCY").is_ok_and(|v| v == "1") {
                    eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY
                } else {
                    eframe::egui_wgpu::SurfaceConfig::HIGH_THROUGHPUT
                }
            },
            // On machines where the default renderer failed, a fallback (see startup.rs).
            wgpu_setup: renderer.wgpu_setup().unwrap_or_else(|| eframe::egui_wgpu::WgpuConfiguration::default().wgpu_setup),
            ..Default::default()
        },
        event_loop_builder: platform::event_loop_hook(),
        // DRROCKET_DITHER=0: skip egui's dithering pass, for measuring software rendering.
        dithering: std::env::var("DRROCKET_DITHER").map_or(true, |v| v != "0"),
        ..Default::default()
    };
    let result = eframe::run_native(ui::welcome::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))));
    if let Err(e) = &result {
        // A renderer that can't create the window: start again with the next one.
        if startup::relaunch_after(renderer) {
            return Ok(());
        }
        crash::startup_failed(&e.to_string());
    }
    result
}
