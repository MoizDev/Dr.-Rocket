//! Glue between the UI and the Racket backend: Run, REPL, stepper, check syntax, docs.

use crate::app::{App, PanelItem};
use crate::commands::Command;
use crate::ui::widgets::empty_state;
use crate::ui::ic;
use egui::{Color32, Rect, Ui};

pub fn run(app: &mut App, cmd: Command) {
    let _ = cmd;
    app.toast("The Racket backend is being connected", crate::app::ToastKind::Info);
}

pub fn poll(_app: &mut App, _ctx: &egui::Context) {}

pub fn after_editor(_app: &mut App) {}

pub fn is_running(_app: &App) -> bool {
    false
}

pub fn status_text(_app: &App) -> (String, Color32) {
    (String::new(), Color32::PLACEHOLDER)
}

pub fn condition(_app: &App, _cond: &str) -> bool {
    true
}

pub fn panel(app: &mut App, ui: &mut Ui, _rect: Rect, item: PanelItem) {
    let theme = app.theme.clone();
    let (glyph, title, sub) = match item {
        PanelItem::Interactions => (ic::TERMINAL, "Interactions", "Run your program (⌘R) to start the REPL"),
        PanelItem::Stepper => (ic::STEP, "Stepper", "Step through a teaching-language program"),
        PanelItem::Tests => (ic::SCIENCE, "Tests", "check-expect results appear here"),
        PanelItem::Macros => (ic::AUTO_FIX_HIGH, "Macro Stepper", "Watch macros expand step by step"),
        _ => (ic::MENU_BOOK, "Documentation", "Search the Racket docs"),
    };
    empty_state(ui, &theme, glyph, title, sub);
}
