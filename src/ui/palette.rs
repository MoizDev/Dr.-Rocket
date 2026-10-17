//! The command palette: commands (`>`), files, go to line (`:`), definitions (`@`), themes.

use crate::app::{App, Modal};
use crate::commands::{COMMANDS, shortcut_text};
use crate::fuzzy::FuzzyMatcher;
use crate::ui::{self, file_icons, ic};
use egui::{Align2, CornerRadius, Id, Key, Rect, Sense, Ui, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Commands,
    Files,
    Line,
    Symbols,
    Themes,
}

pub struct Palette {
    pub mode: Mode,
    pub query: String,
    pub selected: usize,
    focus: bool,
    matcher: FuzzyMatcher,
    /// Theme to restore if the theme picker is cancelled.
    original_theme: Option<String>,
}

enum Item {
    Command(usize),
    File(std::path::PathBuf, Vec<u32>),
    Symbol(String, usize, crate::editor::document::OutlineKind),
    Theme(String, &'static str),
    Line(usize),
}

impl Palette {
    pub fn new(mode: Mode, query: &str) -> Self {
        Self { mode, query: query.into(), selected: 0, focus: true, matcher: FuzzyMatcher::default(), original_theme: None }
    }

    pub fn themes(current: &str) -> Self {
        let mut p = Self::new(Mode::Themes, "");
        p.original_theme = Some(current.to_string());
        p
    }

    /// Prefix characters switch modes, as in VS Code and Zed.
    fn effective(&self) -> (Mode, String) {
        let q = self.query.as_str();
        match self.mode {
            Mode::Files | Mode::Commands => {
                if let Some(r) = q.strip_prefix('>') {
                    (Mode::Commands, r.trim().to_string())
                } else if let Some(r) = q.strip_prefix(':') {
                    (Mode::Line, r.trim().to_string())
                } else if let Some(r) = q.strip_prefix('@') {
                    (Mode::Symbols, r.trim().to_string())
                } else {
                    (self.mode, q.trim().to_string())
                }
            }
            m => (m, q.trim().to_string()),
        }
    }
}

impl App {
    fn palette_items(&mut self, p: &mut Palette) -> (Mode, Vec<Item>) {
        let (mode, q) = p.effective();
        let items = match mode {
            Mode::Commands => {
                let labels: Vec<String> = COMMANDS.iter().map(|c| format!("{}: {}", c.group, c.label)).collect();
                if q.is_empty() {
                    (0..COMMANDS.len()).map(Item::Command).collect()
                } else {
                    p.matcher.rank(&q, labels.iter().map(|s| s.as_str())).into_iter().map(|(i, _, _)| Item::Command(i)).collect()
                }
            }
            Mode::Files => {
                let (root, files) = match &self.workspace {
                    Some(ws) => (Some(ws.root.clone()), ws.index.lock().clone()),
                    None => (None, Vec::new()),
                };
                let mut out: Vec<Item> = Vec::new();
                if q.is_empty() {
                    // Recent files first.
                    for r in self.recents.files.iter().take(12) {
                        out.push(Item::File(r.path.clone(), Vec::new()));
                    }
                } else if root.is_none() {
                    // No folder open: search the recent files (as their rows show them).
                    let recent: Vec<std::path::PathBuf> = self.recents.files.iter().map(|r| r.path.clone()).collect();
                    let strs: Vec<String> = recent.iter().map(|f| crate::config::tildify(f)).collect();
                    for (i, _, idx) in p.matcher.rank(&q, strs.iter().map(|s| s.as_str())).into_iter().take(80) {
                        out.push(Item::File(recent[i].clone(), idx));
                    }
                } else {
                    let strs: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
                    for (i, _, idx) in p.matcher.rank(&q, strs.iter().map(|s| s.as_str())).into_iter().take(80) {
                        let full = root.as_ref().map(|r| r.join(&files[i])).unwrap_or_else(|| files[i].clone());
                        out.push(Item::File(full, idx));
                    }
                }
                out
            }
            Mode::Symbols => {
                let outline = self.tabs.get_mut(self.active).map(|t| t.editor.doc.outline()).unwrap_or_default();
                let names: Vec<&str> = outline.iter().map(|o| o.name.as_str()).collect();
                if q.is_empty() {
                    outline.iter().map(|o| Item::Symbol(o.name.clone(), o.name_pos, o.kind)).collect()
                } else {
                    p.matcher.rank(&q, names.into_iter()).into_iter().map(|(i, _, _)| Item::Symbol(outline[i].name.clone(), outline[i].name_pos, outline[i].kind)).collect()
                }
            }
            Mode::Themes => {
                let entries: Vec<(String, &'static str)> = self.themes.themes.iter().map(|e| (e.theme.name.clone(), e.source.label())).collect();
                if q.is_empty() {
                    entries.into_iter().map(|(n, s)| Item::Theme(n, s)).collect()
                } else {
                    p.matcher.rank(&q, entries.iter().map(|e| e.0.as_str())).into_iter().map(|(i, _, _)| Item::Theme(entries[i].0.clone(), entries[i].1)).collect()
                }
            }
            Mode::Line => q.parse::<usize>().ok().filter(|n| *n > 0).map(|n| vec![Item::Line(n - 1)]).unwrap_or_default(),
        };
        (mode, items)
    }

    fn accept(&mut self, item: &Item) {
        match item {
            Item::Command(i) => {
                let cmd = COMMANDS[*i].cmd;
                self.modal = Modal::None;
                self.run_command(cmd);
            }
            Item::File(path, _) => {
                self.modal = Modal::None;
                self.open_file(path);
            }
            Item::Symbol(_, pos, _) | Item::Line(pos) => {
                let is_line = matches!(item, Item::Line(_));
                self.modal = Modal::None;
                if let Some(t) = self.tabs.get_mut(self.active) {
                    let p = if is_line { t.editor.doc.buffer.line_start(*pos) } else { *pos };
                    t.editor.reveal(p..p);
                    t.editor.request_focus();
                }
            }
            Item::Theme(name, _) => {
                self.set_theme(name, true);
                self.modal = Modal::None;
            }
        }
    }

    pub(crate) fn palette_ui(&mut self, ctx: &egui::Context, mut p: Palette) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let (mode, items) = self.palette_items(&mut p);
        p.selected = p.selected.min(items.len().saturating_sub(1));
        let screen = ctx.content_rect();
        let width = 620.0_f32.min(screen.width() - 40.0);
        let mut close = false;
        let mut accept = None;
        // Dim the background.
        egui::Area::new(Id::unique("palette-scrim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::ZERO, egui::Color32::from_black_alpha(if theme.dark { 90 } else { 40 }));
            if resp.clicked() {
                close = true;
            }
        });
        egui::Area::new(Id::unique("palette")).order(egui::Order::Foreground).fixed_pos(pos2(screen.center().x - width / 2.0, screen.top() + 70.0)).show(ctx, |ui: &mut Ui| {
            egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(12)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                ui.set_width(width - 16.0);
                let hint = match mode {
                    Mode::Commands => "Type a command…",
                    Mode::Files => "Search files by name  ( > commands   : line   @ definitions )",
                    Mode::Line => "Go to line…",
                    Mode::Symbols => "Go to definition…",
                    Mode::Themes => "Select a theme…",
                };
                let id = Id::unique("palette-input");
                let resp = crate::ui::widgets::search_field(ui, &theme, &mut p.query, hint, id, width - 16.0);
                if p.focus {
                    resp.request_focus();
                    p.focus = false;
                }
                if resp.changed() {
                    p.selected = 0;
                }
                let (up, down, enter, esc) = ui.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
                if !items.is_empty() {
                    if down {
                        p.selected = (p.selected + 1) % items.len();
                    }
                    if up {
                        p.selected = (p.selected + items.len() - 1) % items.len();
                    }
                }
                if esc {
                    close = true;
                }
                if enter && !items.is_empty() {
                    accept = Some(p.selected);
                }
                ui.memory_mut(|m| m.request_focus(id));
                ui.add_space(6.0);
                if items.is_empty() {
                    let msg = match mode {
                        Mode::Files if self.workspace.is_none() && !p.query.is_empty() => "No recent file matches. Open a folder to search all its files.",
                        Mode::Line => "Type a line number",
                        _ => "No matches",
                    };
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(msg).color(theme.text_muted));
                    ui.add_space(6.0);
                    return;
                }
                let row_h = 34.0;
                let shown = items.len().min(12);
                let start = p.selected.saturating_sub(shown - 1).min(items.len() - shown);
                for (k, item) in items.iter().enumerate().skip(start).take(shown) {
                    let (r, rresp) = ui.allocate_exact_size(vec2(width - 16.0, row_h), Sense::click());
                    if k == p.selected {
                        ui.painter().rect_filled(r, CornerRadius::same(7), tk.selected);
                    } else if rresp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(7), tk.hover);
                    }
                    if rresp.clicked() {
                        accept = Some(k);
                    }
                    if rresp.hovered() && ui.input(|i| i.pointer.delta().length() > 0.0) {
                        p.selected = k;
                    }
                    self.palette_row(ui, r, item, k == p.selected);
                }
            });
        });
        // Live theme preview.
        if mode == Mode::Themes
            && let Some(Item::Theme(name, _)) = items.get(p.selected)
            && &self.theme.name != name
        {
            self.theme = self.themes.get_or_default(name);
            self.apply_theme_overrides();
        }
        if close {
            if let Some(orig) = &p.original_theme {
                self.set_theme(orig, false);
            }
            self.modal = Modal::None;
            if let Some(t) = self.tabs.get_mut(self.active) {
                t.editor.request_focus();
            }
            return;
        }
        if let Some(k) = accept {
            self.modal = Modal::None;
            self.accept(&items[k]);
            return;
        }
        self.modal = Modal::Palette(p);
    }

    fn palette_row(&self, ui: &Ui, r: Rect, item: &Item, selected: bool) {
        let theme = &self.theme;
        let p = ui.painter();
        let text_color = if selected { theme.text } else { crate::theme::mix(theme.text, theme.text_muted, 0.3) };
        match item {
            Item::Command(i) => {
                let c = &COMMANDS[*i];
                p.text(pos2(r.left() + 18.0, r.center().y), Align2::CENTER_CENTER, c.icon, ui::icon_font(18.0), theme.icon);
                p.text(pos2(r.left() + 38.0, r.center().y), Align2::LEFT_CENTER, c.group, ui::ui_font(13.5), theme.text_placeholder);
                let gw = p.layout_no_wrap(c.group.to_string(), ui::ui_font(13.5), theme.text).size().x;
                p.text(pos2(r.left() + 38.0 + gw + 8.0, r.center().y), Align2::LEFT_CENTER, c.label, ui::ui_font(14.0), text_color);
                if let Some(s) = &c.shortcut {
                    p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, shortcut_text(s), ui::medium(12.5), theme.text_muted);
                }
            }
            Item::File(path, idx) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                file_icons::image(file_icons::file_icon(&name), 16.0).paint_at(ui, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(16.0, 16.0)));
                let rel = match &self.workspace {
                    Some(ws) => ws.relative(path).display().to_string(),
                    None => crate::config::tildify(path),
                };
                // Zed's layout: the file name, then its folder dimmed, cut to one line with an
                // ellipsis. `idx` holds matched character positions in `rel`.
                let chars: Vec<char> = rel.chars().collect();
                let name_start = chars.len().saturating_sub(name.chars().count());
                let dir_end = chars[..name_start].iter().rposition(|c| *c == '/' || *c == '\\').unwrap_or(0);
                let push = |job: &mut egui::text::LayoutJob, range: std::ops::Range<usize>, size: f32, base: egui::Color32| {
                    for ci in range {
                        let hit = idx.binary_search(&(ci as u32)).is_ok();
                        let font = if hit { ui::semibold(size) } else { ui::ui_font(size) };
                        job.append(&chars[ci].to_string(), 0.0, egui::TextFormat { font_id: font, color: if hit { theme.text_accent } else { base }, valign: egui::Align::Center, ..Default::default() });
                    }
                };
                // The folder part from character `from`, with "…" in front when shortened.
                let build = |from: usize| {
                    let mut job = egui::text::LayoutJob::default();
                    push(&mut job, name_start..chars.len(), 14.0, text_color);
                    if dir_end > 0 {
                        let dim = egui::TextFormat { font_id: ui::ui_font(12.5), color: theme.text_placeholder, valign: egui::Align::Center, ..Default::default() };
                        job.append("  ", 0.0, dim.clone());
                        if from > 0 {
                            job.append("…", 0.0, dim);
                        }
                        push(&mut job, from..dir_end, 12.5, theme.text_placeholder);
                    }
                    job
                };
                let left = r.left() + 36.0;
                let max_w = (r.right() - 12.0 - left).max(40.0);
                // Long folders lose their start, so the folders nearest the file stay visible.
                let fits = |from: usize| p.layout_job(build(from)).size().x <= max_w;
                let from = if fits(0) {
                    0
                } else {
                    let (mut lo, mut hi) = (1, dir_end);
                    while lo < hi {
                        let mid = (lo + hi) / 2;
                        if fits(mid) { hi = mid } else { lo = mid + 1 }
                    }
                    lo
                };
                let mut job = build(from);
                // A file name too long on its own still ends in an ellipsis.
                job.wrap = egui::text::TextWrapping { max_width: max_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
                let g = p.layout_job(job);
                p.galley(pos2(left, r.center().y - g.size().y / 2.0), g, text_color);
            }
            Item::Symbol(name, _, kind) => {
                use crate::editor::document::OutlineKind as K;
                let (glyph, c) = match kind {
                    K::Function => (ic::FUNCTION, theme.syntax.function.color),
                    K::Value => (ic::DATA_OBJECT, theme.syntax.variable.color),
                    K::Macro => (ic::AUTO_FIX_HIGH, theme.syntax.keyword.color),
                    K::Struct => (ic::SCHEMA, theme.syntax.type_.color),
                    K::Module => (ic::DEPLOYED_CODE, theme.syntax.lang.color),
                    K::Test => (ic::SCIENCE, theme.success),
                };
                p.text(pos2(r.left() + 18.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(17.0), c);
                p.text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, name, ui::mono(14.0), text_color);
            }
            Item::Theme(name, source) => {
                if let Some(t) = self.themes.get(name) {
                    // Little swatch of the theme's colors.
                    let sw = Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(28.0, 18.0));
                    p.rect_filled(sw, CornerRadius::same(4), t.editor_bg);
                    for (k, c) in [t.syntax.keyword.color, t.syntax.function.color, t.syntax.string.color].iter().enumerate() {
                        p.circle_filled(pos2(sw.left() + 7.0 + k as f32 * 7.0, sw.center().y), 2.6, *c);
                    }
                    p.rect_stroke(sw, CornerRadius::same(4), egui::Stroke::new(1.0, crate::theme::with_alpha(theme.text, 30)), egui::StrokeKind::Inside);
                }
                p.text(pos2(r.left() + 46.0, r.center().y), Align2::LEFT_CENTER, name, ui::ui_font(14.0), text_color);
                p.text(pos2(r.right() - 12.0, r.center().y), Align2::RIGHT_CENTER, *source, ui::ui_font(12.5), theme.text_placeholder);
            }
            Item::Line(n) => {
                p.text(pos2(r.left() + 18.0, r.center().y), Align2::CENTER_CENTER, ic::FORMAT_LIST_NUMBERED, ui::icon_font(17.0), theme.icon);
                p.text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, format!("Go to line {}", n + 1), ui::ui_font(14.0), text_color);
            }
        }
    }
}
