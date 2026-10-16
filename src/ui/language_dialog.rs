//! The Choose Language dialog (every DrRacket language level and its options) and small
//! text-prompt dialogs (rename, install package).

use crate::app::{App, Modal};
use crate::racket::lang::{FractionStyle, GROUPS, Language, TEACHPACKS, TeachingSettings};
use crate::theme::{mix, with_alpha};
use crate::ui::settings_view::{primary_button, radio, toggle};
use crate::ui::{self, ic};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

pub struct LanguageDialog {
    pub language: Language,
    pub settings: TeachingSettings,
    /// Settings per language while browsing, so switching back keeps edits.
    edited: Vec<(Language, TeachingSettings)>,
}

impl LanguageDialog {
    pub fn new(language: Language, settings: TeachingSettings) -> Self {
        Self { language, settings: settings.clone(), edited: vec![(language, settings)] }
    }

    fn select(&mut self, l: Language) {
        if l == self.language {
            return;
        }
        if let Some(e) = self.edited.iter_mut().find(|e| e.0 == self.language) {
            e.1 = self.settings.clone();
        } else {
            self.edited.push((self.language, self.settings.clone()));
        }
        self.language = l;
        self.settings = self.edited.iter().find(|e| e.0 == l).map(|e| e.1.clone()).unwrap_or_else(|| l.default_settings());
    }
}

pub enum PromptKind {
    Rename { tab: u64, ranges: Vec<(usize, usize)> },
    InstallPackage,
    UpdatePackage,
    RemovePackage,
    NewFile { dir: std::path::PathBuf },
    NewFolder { dir: std::path::PathBuf },
    RenamePath { path: std::path::PathBuf },
}

pub struct Prompt {
    pub kind: PromptKind,
    pub title: String,
    pub hint: String,
    pub text: String,
    focus: bool,
}

impl Prompt {
    pub fn rename(tab: u64, ranges: Vec<(usize, usize)>, current: String) -> Self {
        let n = ranges.len();
        Self { kind: PromptKind::Rename { tab, ranges }, title: format!("Rename “{current}” ({n} occurrences)"), hint: "New name".into(), text: current, focus: true }
    }
    pub fn new_file(dir: std::path::PathBuf) -> Self {
        Self { kind: PromptKind::NewFile { dir }, title: "New File".into(), hint: "name.rkt".into(), text: String::new(), focus: true }
    }
    pub fn new_folder(dir: std::path::PathBuf) -> Self {
        Self { kind: PromptKind::NewFolder { dir }, title: "New Folder".into(), hint: "folder name".into(), text: String::new(), focus: true }
    }
    pub fn rename_path(path: std::path::PathBuf) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Self { kind: PromptKind::RenamePath { path }, title: format!("Rename “{name}”"), hint: "new name".into(), text: name, focus: true }
    }
    pub fn update_package() -> Self {
        Self { kind: PromptKind::UpdatePackage, title: "Update Package".into(), hint: "Package name, or leave empty to update all".into(), text: String::new(), focus: true }
    }
    pub fn remove_package() -> Self {
        Self { kind: PromptKind::RemovePackage, title: "Remove Package".into(), hint: "Package name".into(), text: String::new(), focus: true }
    }
    pub fn install_package() -> Self {
        Self { kind: PromptKind::InstallPackage, title: "Install Package".into(), hint: "Package name or URL, e.g. threading".into(), text: String::new(), focus: true }
    }
}

impl App {
    pub(crate) fn prompt_ui(&mut self, ctx: &egui::Context, mut p: Prompt) {
        let mut done: Option<bool> = None;
        let title = p.title.clone();
        self.dialog(ctx, &title, 420.0, |app, ui| {
            let resp = ui.add(egui::TextEdit::singleline(&mut p.text).hint_text(&p.hint).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if p.focus {
                resp.request_focus();
                p.focus = false;
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                done = Some(true);
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button(&app.theme, "OK")).clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            done = Some(false);
        }
        match done {
            None => self.modal = Modal::Prompt(p),
            Some(false) => {}
            Some(true) => match p.kind {
                PromptKind::Rename { tab, ranges } => {
                    let name = p.text.trim().to_string();
                    let valid = !name.is_empty() && !name.chars().any(crate::editor::lexer::is_delimiter);
                    if !valid {
                        self.toast("Not a valid identifier", crate::app::ToastKind::Error);
                        return;
                    }
                    if let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) {
                        let rs: Vec<std::ops::Range<usize>> = ranges.iter().map(|r| r.0..r.1).collect();
                        t.editor.doc.rename(&rs, &name);
                    }
                }
                PromptKind::InstallPackage => {
                    let name = p.text.trim().to_string();
                    if !name.is_empty() {
                        crate::racket_ui::install_package(self, &name);
                    }
                }
                PromptKind::UpdatePackage => {
                    let name = p.text.trim().to_string();
                    let args = if name.is_empty() { vec!["--all".to_string()] } else { vec![name] };
                    crate::racket_ui::raco_pkg(self, "update", args);
                }
                PromptKind::RemovePackage => {
                    let name = p.text.trim().to_string();
                    if !name.is_empty() {
                        crate::racket_ui::raco_pkg(self, "remove", vec![name]);
                    }
                }
                PromptKind::NewFile { dir } => {
                    let name = p.text.trim();
                    if name.is_empty() || name.contains('/') {
                        return;
                    }
                    // `foo` means `foo.rkt`.
                    let path = if std::path::Path::new(name).extension().is_none() { dir.join(format!("{name}.rkt")) } else { dir.join(name) };
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let default = crate::racket::lang::Language::from_id(&self.settings.default_language).unwrap_or(crate::racket::lang::Language::Module);
                    let racket = path.extension().is_some_and(|e| e == "rkt");
                    let initial = if racket && default == crate::racket::lang::Language::Module { "#lang racket\n\n" } else { "" };
                    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, initial.as_bytes())) {
                        Ok(()) => {
                            self.refresh_tree();
                            self.open_file(&path);
                            // A teaching-language default: the file is in that language (DrRacket
                            // writes its header on the first save).
                            if racket && default != crate::racket::lang::Language::Module {
                                crate::racket_ui::set_language(self, default, default.default_settings());
                            }
                        }
                        Err(e) => self.toast(format!("Could not create {name}: {e}"), crate::app::ToastKind::Error),
                    }
                }
                PromptKind::NewFolder { dir } => {
                    let name = p.text.trim();
                    if name.is_empty() {
                        return;
                    }
                    match std::fs::create_dir_all(dir.join(name)) {
                        Ok(()) => self.refresh_tree(),
                        Err(e) => self.toast(format!("Could not create folder: {e}"), crate::app::ToastKind::Error),
                    }
                }
                PromptKind::RenamePath { path } => {
                    let name = p.text.trim();
                    if name.is_empty() || name.contains('/') {
                        return;
                    }
                    let Some(parent) = path.parent() else { return };
                    let to = parent.join(name);
                    match std::fs::rename(&path, &to) {
                        Ok(()) => {
                            for t in self.tabs.iter_mut() {
                                if let Some(tp) = &t.path
                                    && let Ok(rest) = tp.strip_prefix(&path)
                                {
                                    t.path = Some(if rest.as_os_str().is_empty() { to.clone() } else { to.join(rest) });
                                }
                            }
                            self.refresh_tree();
                        }
                        Err(e) => self.toast(format!("Rename failed: {e}"), crate::app::ToastKind::Error),
                    }
                }
            },
        }
    }

    pub(crate) fn language_dialog_ui(&mut self, ctx: &egui::Context, mut d: LanguageDialog) {
        let screen = ctx.content_rect();
        let theme = self.theme.clone();
        let tk = self.tokens();
        let mut result: Option<bool> = None;
        let size = vec2(880.0_f32.min(screen.width() - 60.0), 640.0_f32.min(screen.height() - 70.0));
        egui::Area::new(Id::unique("lang-scrim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::ZERO, egui::Color32::from_black_alpha(if theme.dark { 110 } else { 50 }));
            if resp.clicked() {
                result = Some(false);
            }
        });
        egui::Area::new(Id::unique("lang-dialog")).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(screen.center()).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                let p = ui.painter();
                p.text(pos2(rect.left() + 24.0, rect.top() + 30.0), Align2::LEFT_CENTER, "Choose Language", ui::semibold(18.0), theme.text);
                p.text(pos2(rect.left() + 24.0, rect.top() + 52.0), Align2::LEFT_CENTER, "Every DrRacket language level. Teaching-language files stay compatible with DrRacket.", ui::ui_font(12.5), theme.text_placeholder);
                let footer_h = 60.0;
                let left = Rect::from_min_max(pos2(rect.left() + 14.0, rect.top() + 74.0), pos2(rect.left() + 400.0, rect.bottom() - footer_h));
                let right = Rect::from_min_max(pos2(left.right() + 18.0, rect.top() + 74.0), pos2(rect.right() - 24.0, rect.bottom() - footer_h));
                p.vline(left.right() + 8.0, left.y_range(), Stroke::new(1.0, tk.island_border));
                p.hline(rect.x_range(), rect.bottom() - footer_h, Stroke::new(1.0, tk.island_border));

                // Language list.
                let mut lc = ui.new_child(egui::UiBuilder::new().max_rect(left));
                egui::ScrollArea::vertical().id_salt("lang-list").auto_shrink([false, false]).show(&mut lc, |ui| {
                    let mut last_heading = "";
                    for g in GROUPS {
                        if g.heading != last_heading {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(g.heading.to_uppercase()).font(ui::semibold(11.0)).color(theme.text_placeholder));
                            last_heading = g.heading;
                        }
                        if !g.title.is_empty() {
                            ui.label(egui::RichText::new(g.title).font(ui::medium(12.5)).color(theme.text_muted));
                        }
                        for &l in g.languages {
                            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 6.0, 50.0), Sense::click());
                            let sel = d.language == l;
                            if sel {
                                ui.painter().rect(r, CornerRadius::same(9), mix(tk.popup, theme.text_accent, 0.12), Stroke::new(1.0, with_alpha(theme.text_accent, 120)), StrokeKind::Inside);
                            } else if resp.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(9), tk.hover);
                            }
                            let c = pos2(r.left() + 18.0, r.center().y);
                            ui.painter().circle_stroke(c, 7.0, Stroke::new(1.5, if sel { theme.text_accent } else { theme.text_placeholder }));
                            if sel {
                                ui.painter().circle_filled(c, 4.0, theme.text_accent);
                            }
                            ui.painter().text(pos2(r.left() + 36.0, r.top() + 16.0), Align2::LEFT_CENTER, l.name(), ui::medium(14.0), theme.text);
                            crate::ui::widgets::truncated_label(ui, pos2(r.left() + 36.0, r.top() + 35.0), l.summary(), ui::ui_font(12.0), theme.text_muted, r.width() - 44.0);
                            if resp.double_clicked() {
                                result = Some(true);
                            }
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                d.select(l);
                            }
                        }
                    }
                });

                // Details.
                let mut rc = ui.new_child(egui::UiBuilder::new().max_rect(right));
                egui::ScrollArea::vertical().id_salt("lang-details").auto_shrink([false, false]).show(&mut rc, |ui| {
                    ui.set_width(right.width() - 8.0);
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(d.language.name()).font(ui::semibold(16.0)).color(theme.text));
                    ui.label(egui::RichText::new(d.language.summary()).color(theme.text_muted));
                    ui.add_space(10.0);
                    if d.language.is_teaching() {
                        teaching_options(ui, &theme, &mut d);
                    } else if d.language == Language::Module {
                        ui.label(egui::RichText::new("The first line of the file selects the language, e.g. #lang racket, #lang racket/base, #lang typed/racket, #lang htdp/bsl, #lang scribble/manual.").color(theme.text_muted));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Runs with errortrace debugging, runs the main and test submodules, and supports the debugger, profiler and coverage.").color(theme.text_placeholder));
                    } else {
                        ui.label(egui::RichText::new("Programs run at the top level, one form at a time, as in DrRacket's legacy languages.").color(theme.text_muted));
                    }
                });

                // Footer.
                let fr = Rect::from_min_max(pos2(rect.left() + 20.0, rect.bottom() - footer_h), rect.max - vec2(20.0, 0.0));
                let mut fc = ui.new_child(egui::UiBuilder::new().max_rect(fr).layout(egui::Layout::right_to_left(egui::Align::Center)));
                if fc.add(primary_button(&theme, "OK")).clicked() {
                    result = Some(true);
                }
                if fc.button("Cancel").clicked() {
                    result = Some(false);
                }
                let mut fl = ui.new_child(egui::UiBuilder::new().max_rect(fr).layout(egui::Layout::left_to_right(egui::Align::Center)));
                if d.language.is_teaching() && fl.button("Revert to Language Defaults").clicked() {
                    d.settings = d.language.default_settings();
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            result = Some(false);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            result = Some(true);
        }
        match result {
            None => self.modal = Modal::Language(d),
            Some(false) => {}
            Some(true) => crate::racket_ui::set_language(self, d.language, d.settings),
        }
    }
}

fn section(ui: &mut Ui, theme: &crate::theme::Theme, title: &str) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title.to_uppercase()).font(ui::semibold(11.0)).color(theme.text_placeholder));
    ui.add_space(2.0);
}

fn toggle_row(ui: &mut Ui, theme: &crate::theme::Theme, label: &str, on: &mut bool) {
    ui.horizontal(|ui| {
        toggle(ui, theme, on);
        ui.add_space(6.0);
        ui.label(egui::RichText::new(label).color(theme.text));
    });
    ui.add_space(2.0);
}

fn teaching_options(ui: &mut Ui, theme: &crate::theme::Theme, d: &mut LanguageDialog) {
    let lang = d.language;
    let s = &mut d.settings;
    section(ui, theme, "Input Syntax");
    toggle_row(ui, theme, "Case sensitive", &mut s.case_sensitive);

    section(ui, theme, "Output Syntax");
    for &style in lang.printing_styles() {
        if radio(ui, theme, s.printing_style == style, style.label()) {
            s.printing_style = style;
        }
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Fraction style").color(theme.text_muted));
    for f in [FractionStyle::MixedFraction, FractionStyle::RepeatingDecimal] {
        if radio(ui, theme, s.fraction_style == f, f.label()) {
            s.fraction_style = f;
        }
    }
    ui.add_space(4.0);
    if !matches!(lang, Language::Beginner | Language::BeginnerAbbr) {
        toggle_row(ui, theme, "Show sharing in values", &mut s.show_sharing);
    }
    toggle_row(ui, theme, "Insert newlines in printed values", &mut s.insert_newlines);
    if lang.family() == crate::racket::lang::Family::Htdp {
        toggle_row(ui, theme, "Print true/false/empty instead of #true/#false/'()", &mut s.true_false_empty_as_ids);
        toggle_row(ui, theme, "Tracing (show function calls)", &mut s.tracing);
    }

    section(ui, theme, "Teachpacks");
    let mut remove = None;
    for (k, tp) in s.teachpacks.iter().enumerate() {
        let label = TEACHPACKS.iter().find(|t| t.1 == tp).map(|t| t.0.to_string()).unwrap_or_else(|| tp.clone());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(ic::EXTENSION).font(ui::icon_font(16.0)).color(theme.text_accent));
            ui.label(egui::RichText::new(label).font(ui::mono(13.0)).color(theme.text));
            if ui.small_button("Remove").clicked() {
                remove = Some(k);
            }
        });
    }
    if let Some(k) = remove {
        s.teachpacks.remove(k);
    }
    let available: Vec<_> = TEACHPACKS.iter().filter(|t| !s.teachpacks.iter().any(|x| x == t.1)).collect();
    if !available.is_empty() {
        egui::ComboBox::from_id_salt("add-teachpack").selected_text("Add teachpack…").width(220.0).show_ui(ui, |ui| {
            for (name, spec) in available {
                if ui.selectable_label(false, *name).clicked() {
                    s.teachpacks.push(spec.to_string());
                }
            }
        });
    }
    if lang.stepper_level("").is_some() {
        ui.add_space(12.0);
        let r = ui.available_rect_before_wrap();
        let card = Rect::from_min_size(r.min, vec2(r.width(), 44.0));
        ui.painter().rect_filled(card, CornerRadius::same(8), with_alpha(theme.success, 26));
        ui.painter().text(pos2(card.left() + 18.0, card.center().y), Align2::CENTER_CENTER, ic::STEP, ui::icon_font(18.0), theme.success);
        ui.painter().text(pos2(card.left() + 36.0, card.center().y), Align2::LEFT_CENTER, crate::platform::keys("Supports the algebraic stepper (⇧⌘E)"), ui::ui_font(13.0), theme.text);
        ui.allocate_exact_size(card.size(), Sense::hover());
    }
}
