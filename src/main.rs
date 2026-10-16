//! Racketeer: a fast, beautiful IDE for Racket.

mod config;
mod editor;
mod racket;
mod terminal;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Racketeer", options, Box::new(|_cc| Ok(Box::new(Stub))))
}

struct Stub;

impl eframe::App for Stub {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label("Racketeer");
    }
}
