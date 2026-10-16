//! The application: workspace state, the islands layout, commands, files and sessions.

use crate::commands::{COMMANDS, Command};
use crate::config::{Recents, Settings};
use crate::editor::view::{EditorMode, EditorStyle, EditorView};
use crate::theme::{Theme, ThemeRegistry};
use crate::ui::{self, GAP, ISLAND_RADIUS, STATUS_BAR_H, TAB_BAR_H, Tokens};
use crate::workspace::Workspace;
use egui::{Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Something that can be shown in a dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelItem {
    Files,
    Outline,
    Search,
    Interactions,
    Stepper,
    Tests,
    Macros,
    Docs,
    Problems,
    Terminal(u64),
}

impl PanelItem {
    pub fn label(&self) -> &'static str {
        match self {
            PanelItem::Files => "Files",
            PanelItem::Outline => "Outline",
            PanelItem::Search => "Search",
            PanelItem::Interactions => "Interactions",
            PanelItem::Stepper => "Stepper",
            PanelItem::Tests => "Tests",
            PanelItem::Macros => "Macro Stepper",
            PanelItem::Docs => "Docs",
            PanelItem::Problems => "Problems",
            PanelItem::Terminal(_) => "Terminal",
        }
    }
    pub fn icon(&self) -> &'static str {
        use crate::ui::ic;
        match self {
            PanelItem::Files => ic::FOLDER,
            PanelItem::Outline => ic::TOC,
            PanelItem::Search => ic::SEARCH,
            PanelItem::Interactions => ic::TERMINAL,
            PanelItem::Stepper => ic::STEP,
            PanelItem::Tests => ic::SCIENCE,
            PanelItem::Macros => ic::AUTO_FIX_HIGH,
            PanelItem::Docs => ic::MENU_BOOK,
            PanelItem::Problems => ic::ERROR,
            PanelItem::Terminal(_) => ic::TERMINAL,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dock {
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockState {
    pub open: bool,
    pub size: f32,
    pub items: Vec<PanelItem>,
    pub active: usize,
}

impl DockState {
    pub fn active_item(&self) -> Option<PanelItem> {
        self.items.get(self.active).or(self.items.first()).copied()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub left: DockState,
    pub right: DockState,
    pub bottom: DockState,
    /// DrRacket-style: the right dock is shown below the definitions instead of beside them.
    pub interactions_below: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: DockState { open: true, size: 260.0, items: vec![PanelItem::Files, PanelItem::Outline, PanelItem::Search], active: 0 },
            right: DockState {
                open: true,
                size: 520.0,
                items: vec![PanelItem::Interactions, PanelItem::Stepper, PanelItem::Tests, PanelItem::Macros, PanelItem::Docs],
                active: 0,
            },
            bottom: DockState { open: false, size: 260.0, items: vec![PanelItem::Problems], active: 0 },
            interactions_below: false,
        }
    }
}

impl Layout {
    pub fn dock(&self, d: Dock) -> &DockState {
        match d {
            Dock::Left => &self.left,
            Dock::Right => &self.right,
            Dock::Bottom => &self.bottom,
        }
    }
    pub fn dock_mut(&mut self, d: Dock) -> &mut DockState {
        match d {
            Dock::Left => &mut self.left,
            Dock::Right => &mut self.right,
            Dock::Bottom => &mut self.bottom,
        }
    }
    pub fn find(&self, item: PanelItem) -> Option<(Dock, usize)> {
        for d in [Dock::Left, Dock::Right, Dock::Bottom] {
            if let Some(i) = self.dock(d).items.iter().position(|x| *x == item) {
                return Some((d, i));
            }
        }
        None
    }
    /// Open the dock containing `item` and make it active (adding it to `default` if absent).
    pub fn show(&mut self, item: PanelItem, default: Dock) {
        let (d, i) = match self.find(item) {
            Some(x) => x,
            None => {
                let ds = self.dock_mut(default);
                ds.items.push(item);
                (default, ds.items.len() - 1)
            }
        };
        let ds = self.dock_mut(d);
        ds.open = true;
        ds.active = i;
    }
    pub fn is_visible(&self, item: PanelItem) -> bool {
        self.find(item).is_some_and(|(d, i)| self.dock(d).open && self.dock(d).active == i)
    }
    pub fn remove(&mut self, item: PanelItem) {
        for d in [Dock::Left, Dock::Right, Dock::Bottom] {
            let ds = self.dock_mut(d);
            if let Some(i) = ds.items.iter().position(|x| *x == item) {
                ds.items.remove(i);
                if ds.active >= ds.items.len() {
                    ds.active = ds.items.len().saturating_sub(1);
                }
                if ds.items.is_empty() {
                    ds.open = false;
                }
            }
        }
    }
    pub fn move_to(&mut self, item: PanelItem, to: Dock) {
        self.remove(item);
        let ds = self.dock_mut(to);
        ds.items.push(item);
        ds.active = ds.items.len() - 1;
        ds.open = true;
    }
    /// Drop terminals (not persisted) before saving.
    pub fn without_terminals(&self) -> Layout {
        let mut l = self.clone();
        for d in [Dock::Left, Dock::Right, Dock::Bottom] {
            let ds = l.dock_mut(d);
            ds.items.retain(|i| !matches!(i, PanelItem::Terminal(_)));
            ds.active = ds.active.min(ds.items.len().saturating_sub(1));
        }
        l
    }
}

/// Persisted between launches.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub project: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub active: usize,
    pub layout: Layout,
}

impl Session {
    fn path() -> PathBuf {
        crate::config::config_dir().join("session.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(crate::config::config_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = crate::config::atomic_write(&Self::path(), s.as_bytes());
        }
    }
}

pub struct Tab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub untitled: usize,
    pub editor: EditorView,
    /// DrRacket metadata header hidden from the editor (teaching languages).
    pub header: Option<String>,
    pub lang_label: String,
    pub disk_mtime: Option<SystemTime>,
}

impl Tab {
    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            None => {
                if self.untitled <= 1 {
                    "Untitled".into()
                } else {
                    format!("Untitled {}", self.untitled)
                }
            }
        }
    }
    pub fn dirty(&self) -> bool {
        self.editor.doc.buffer.is_dirty()
    }
}

/// Modal overlays.
pub enum Modal {
    None,
    Palette(crate::ui::palette::Palette),
    ConfirmClose { tab: u64, then_quit: bool },
    ConfirmQuit,
    Settings(crate::ui::settings_view::SettingsView),
    About,
    Message { title: String, body: String },
}

/// Transient notification shown at the bottom-right.
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub until: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct App {
    pub settings: Settings,
    pub themes: ThemeRegistry,
    pub theme: Theme,
    pub recents: Recents,
    pub workspace: Option<Workspace>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub closed: Vec<PathBuf>,
    pub layout: Layout,
    pub modal: Modal,
    pub toasts: Vec<Toast>,
    next_id: u64,
    pub zoom: f32,
    pub sidebar: crate::ui::sidebar::SidebarState,
    pub ctx: egui::Context,
    styled_for: Option<(String, f32)>,
    pub quit_confirmed: bool,
    pub automation: Option<crate::automation::Automation>,
    pub frame_count: u64,
    pub terminals: Vec<TerminalTab>,
}

pub struct TerminalTab {
    pub id: u64,
    pub view: crate::terminal::TerminalView,
    pub focus: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Vec<PathBuf>) -> Self {
        crate::platform::setup_window(cc);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        ui::install_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let themes = ThemeRegistry::load();
        let theme = themes.get_or_default(&settings.theme);
        let mut recents = Recents::load();
        recents.prune();
        let mut app = App {
            settings,
            themes,
            theme,
            recents,
            workspace: None,
            tabs: Vec::new(),
            active: 0,
            closed: Vec::new(),
            layout: Layout::default(),
            modal: Modal::None,
            toasts: Vec::new(),
            next_id: 1,
            zoom: 1.0,
            sidebar: Default::default(),
            ctx: cc.egui_ctx.clone(),
            styled_for: None,
            quit_confirmed: false,
            automation: crate::automation::Automation::from_env(),
            frame_count: 0,
            terminals: Vec::new(),
        };
        let restore = app.settings.restore_session && open.is_empty() && app.automation.is_none();
        if restore {
            let s = Session::load();
            app.layout = s.layout.clone();
            if let Some(p) = s.project.filter(|p| p.is_dir()) {
                app.open_folder(&p);
            }
            for f in s.files.iter().filter(|f| f.is_file()) {
                app.open_file(f);
            }
            app.active = s.active.min(app.tabs.len().saturating_sub(1));
        }
        for p in open {
            if p.is_dir() {
                app.open_folder(&p);
            } else {
                app.open_file(&p);
            }
        }
        app
    }

    pub fn tokens(&self) -> Tokens {
        Tokens::of(&self.theme)
    }

    pub fn font_size(&self) -> f32 {
        (self.settings.buffer_font_size * self.zoom).clamp(8.0, 48.0)
    }

    pub fn ui_size(&self) -> f32 {
        self.settings.ui_font_size
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        let now = self.ctx.input(|i| i.time);
        self.toasts.push(Toast { text: text.into(), kind, until: now + 3.5 });
        self.ctx.request_repaint();
    }

    pub fn set_theme(&mut self, name: &str, persist: bool) {
        self.theme = self.themes.get_or_default(name);
        if persist {
            self.settings.theme = self.theme.name.clone();
            let _ = self.settings.save();
        }
    }

    // ------------------------------------------------------------------ files

    fn new_tab(&mut self, path: Option<PathBuf>, text: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let untitled = if path.is_none() { self.tabs.iter().filter(|t| t.path.is_none()).map(|t| t.untitled).max().unwrap_or(0) + 1 } else { 0 };
        let mut editor = EditorView::new(text, Id::new(("editor", id)));
        editor.doc.buffer.mark_saved();
        let disk_mtime = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        let lang_label = Self::detect_language_label(text);
        self.tabs.push(Tab { id, path, untitled, editor, header: None, lang_label, disk_mtime });
        self.tabs.len() - 1
    }

    fn detect_language_label(text: &str) -> String {
        let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        if let Some(rest) = first.trim().strip_prefix("#lang") {
            return format!("#lang {}", rest.trim());
        }
        "#lang racket".into()
    }

    pub fn new_file(&mut self) {
        let i = self.new_tab(None, "#lang racket\n\n");
        let tab = &mut self.tabs[i];
        let n = tab.editor.doc.buffer.len();
        tab.editor.doc.buffer.selections = crate::editor::buffer::Selections::single(crate::editor::buffer::Selection::caret(n));
        tab.editor.doc.buffer.mark_saved();
        tab.editor.request_focus();
        self.active = i;
    }

    pub fn open_file(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref() == Some(&path)) {
            self.active = i;
            self.tabs[i].editor.request_focus();
            return;
        }
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
            Err(e) => {
                self.toast(format!("Could not open {}: {e}", path.display()), ToastKind::Error);
                return;
            }
        };
        // Replace an untouched empty Untitled tab, like most editors.
        if let Some(t) = self.active_tab()
            && t.path.is_none()
            && !t.dirty()
            && self.tabs.len() == 1
        {
            self.tabs.clear();
        }
        let i = self.new_tab(Some(path.clone()), &text);
        self.active = i;
        self.tabs[i].editor.request_focus();
        self.recents.add_file(&path);
        if let Some(ws) = &mut self.workspace {
            ws.expand_to(&path);
        }
    }

    pub fn open_folder(&mut self, path: &Path) {
        self.workspace = Some(Workspace::open(path, &self.ctx));
        self.recents.add_project(path);
        self.layout.show(PanelItem::Files, Dock::Left);
    }

    pub fn save_tab(&mut self, idx: usize, save_as: bool) -> bool {
        let Some(tab) = self.tabs.get(idx) else { return false };
        let path = match (&tab.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let mut dlg = rfd::FileDialog::new().add_filter("Racket", &["rkt", "rktl", "rktd", "scrbl", "ss", "scm"]).set_file_name(format!("{}.rkt", tab.title().trim_end_matches(".rkt")));
                if let Some(ws) = &self.workspace {
                    dlg = dlg.set_directory(&ws.root);
                }
                match dlg.save_file() {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        let tab = &self.tabs[idx];
        let mut text = tab.editor.doc.text();
        if let Some(h) = &tab.header {
            text = format!("{h}{text}");
        }
        match crate::config::atomic_write(&path, text.as_bytes()) {
            Ok(()) => {
                let tab = &mut self.tabs[idx];
                tab.editor.doc.buffer.mark_saved();
                tab.path = Some(path.clone());
                tab.disk_mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
                self.recents.add_file(&path);
                true
            }
            Err(e) => {
                self.toast(format!("Save failed: {e}"), ToastKind::Error);
                false
            }
        }
    }

    pub fn close_tab(&mut self, id: u64, force: bool) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == id) else { return };
        if !force && self.tabs[idx].dirty() {
            self.modal = Modal::ConfirmClose { tab: id, then_quit: false };
            return;
        }
        let tab = self.tabs.remove(idx);
        if let Some(p) = tab.path {
            self.closed.push(p);
        }
        if self.active >= idx && self.active > 0 {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.editor.request_focus();
        }
    }

    /// Reload buffers whose files changed on disk (when they have no unsaved edits).
    fn reload_changed_files(&mut self) {
        let changed: Vec<PathBuf> = match &mut self.workspace {
            Some(ws) => std::mem::take(&mut ws.changed_files),
            None => return,
        };
        if changed.is_empty() {
            return;
        }
        for tab in &mut self.tabs {
            let Some(p) = &tab.path else { continue };
            if !changed.iter().any(|c| c == p) {
                continue;
            }
            let mtime = std::fs::metadata(p).ok().and_then(|m| m.modified().ok());
            if mtime == tab.disk_mtime || tab.dirty() {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(p) {
                let sel = tab.editor.doc.buffer.selections.clone();
                tab.editor.doc.buffer.set_text(&text);
                tab.editor.doc.buffer.mark_saved();
                let n = tab.editor.doc.buffer.len();
                let mut sel = sel;
                for s in sel.list.iter_mut() {
                    *s = crate::editor::buffer::Selection::new(s.anchor.min(n), s.head.min(n));
                }
                tab.editor.doc.buffer.selections = sel;
                tab.disk_mtime = mtime;
            }
        }
    }

    pub fn save_session(&self) {
        let s = Session {
            project: self.workspace.as_ref().map(|w| w.root.clone()),
            files: self.tabs.iter().filter_map(|t| t.path.clone()).collect(),
            active: self.active,
            layout: self.layout.without_terminals(),
        };
        s.save();
    }

    // ------------------------------------------------------------------ commands

    pub fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::NewFile => self.new_file(),
            Command::OpenFile => {
                let mut dlg = rfd::FileDialog::new().add_filter("Racket", &["rkt", "rktl", "rktd", "scrbl", "ss", "scm"]).add_filter("All files", &["*"]);
                if let Some(ws) = &self.workspace {
                    dlg = dlg.set_directory(&ws.root);
                }
                if let Some(files) = dlg.pick_files() {
                    for f in files {
                        self.open_file(&f);
                    }
                }
            }
            Command::OpenFolder => {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(&d);
                }
            }
            Command::Save => {
                self.save_tab(self.active, false);
            }
            Command::SaveAs => {
                self.save_tab(self.active, true);
            }
            Command::SaveAll => {
                for i in 0..self.tabs.len() {
                    if self.tabs[i].dirty() && self.tabs[i].path.is_some() {
                        self.save_tab(i, false);
                    }
                }
            }
            Command::CloseTab => {
                if let Some(id) = self.active_tab().map(|t| t.id) {
                    self.close_tab(id, false);
                }
            }
            Command::ReopenClosedTab => {
                if let Some(p) = self.closed.pop() {
                    self.open_file(&p);
                }
            }
            Command::CloseFolder => self.workspace = None,
            Command::RevealInFinder => {
                if let Some(p) = self.active_tab().and_then(|t| t.path.clone()) {
                    let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
                }
            }
            Command::CommandPalette => self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, "")),
            Command::QuickOpen => self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Files, "")),
            Command::GoToLine => self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Line, "")),
            Command::GoToSymbol => self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Symbols, "")),
            Command::SelectTheme => self.modal = Modal::Palette(crate::ui::palette::Palette::themes(&self.theme.name)),
            Command::NextTab => {
                if !self.tabs.is_empty() {
                    self.active = (self.active + 1) % self.tabs.len();
                }
            }
            Command::PrevTab => {
                if !self.tabs.is_empty() {
                    self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                }
            }
            Command::NextProblem => {
                if let Some(t) = self.active_tab_mut() {
                    let head = t.editor.doc.buffer.selections.primary().head;
                    let mut ds: Vec<_> = t.editor.deco.diagnostics.iter().map(|d| d.range.clone()).collect();
                    ds.sort_by_key(|r| r.start);
                    if let Some(r) = ds.iter().find(|r| r.start > head).or(ds.first()).cloned() {
                        t.editor.reveal(r);
                    }
                }
            }
            Command::ToggleLeftPanel => self.layout.left.open = !self.layout.left.open,
            Command::ToggleRightPanel => self.layout.right.open = !self.layout.right.open,
            Command::ToggleBottomPanel => self.layout.bottom.open = !self.layout.bottom.open,
            Command::ShowFiles => self.toggle_item(PanelItem::Files, Dock::Left),
            Command::ShowOutline => self.toggle_item(PanelItem::Outline, Dock::Left),
            Command::ShowSearch => {
                self.layout.show(PanelItem::Search, Dock::Left);
                self.sidebar.focus_search = true;
            }
            Command::NewTerminal => self.new_terminal(Dock::Bottom),
            Command::FocusEditor => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.request_focus();
                }
            }
            Command::FocusInteractions => self.layout.show(PanelItem::Interactions, Dock::Right),
            Command::MoveInteractions => self.layout.interactions_below = !self.layout.interactions_below,
            Command::ZoomIn => self.zoom = (self.zoom + 0.1).min(2.5),
            Command::ZoomOut => self.zoom = (self.zoom - 0.1).max(0.5),
            Command::ZoomReset => self.zoom = 1.0,
            Command::ToggleRainbowBrackets => {
                self.settings.rainbow_brackets = !self.settings.rainbow_brackets;
                let _ = self.settings.save();
            }
            Command::ToggleBindingArrows => {
                self.settings.show_binding_arrows = !self.settings.show_binding_arrows;
                let _ = self.settings.save();
            }
            Command::InsertLambda => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.doc.type_text("λ", false);
                }
            }
            Command::ReindentAll => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.doc.reindent_all();
                }
            }
            Command::Settings => self.modal = Modal::Settings(crate::ui::settings_view::SettingsView::default()),
            Command::About => self.modal = Modal::About,
            Command::KeyboardShortcuts => {
                self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, ""));
            }
            other => self.run_racket_command(other),
        }
    }

    /// Show `item`, or hide its dock if it is already the visible item.
    fn toggle_item(&mut self, item: PanelItem, default: Dock) {
        if self.layout.is_visible(item) {
            if let Some((d, _)) = self.layout.find(item) {
                self.layout.dock_mut(d).open = false;
            }
        } else {
            self.layout.show(item, default);
        }
    }

    pub fn new_terminal(&mut self, dock: Dock) {
        let cwd = self
            .workspace
            .as_ref()
            .map(|w| w.root.clone())
            .or_else(|| self.active_tab().and_then(|t| t.path.as_ref()).and_then(|p| p.parent().map(Path::to_path_buf)))
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        match crate::terminal::TerminalView::spawn(&cwd, self.settings.shell.as_deref(), &self.ctx) {
            Ok(view) => {
                let id = self.next_id;
                self.next_id += 1;
                self.terminals.push(TerminalTab { id, view, focus: true });
                self.layout.show(PanelItem::Terminal(id), dock);
            }
            Err(e) => self.toast(format!("Could not start terminal: {e}"), ToastKind::Error),
        }
    }

    pub fn close_terminal(&mut self, id: u64) {
        self.terminals.retain(|t| t.id != id);
        self.layout.remove(PanelItem::Terminal(id));
    }

    /// Racket commands are wired to the backend in `racket_ui.rs`.
    fn run_racket_command(&mut self, cmd: Command) {
        crate::racket_ui::run(self, cmd);
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if !matches!(self.modal, Modal::None) {
            return;
        }
        let mut fired = Vec::new();
        ctx.input_mut(|i| {
            for c in COMMANDS {
                if let Some(s) = c.shortcut
                    && i.consume_shortcut(&s)
                {
                    fired.push(c.cmd);
                }
            }
        });
        for c in fired {
            self.run_command(c);
        }
    }

    // ------------------------------------------------------------------ layout

    /// Apply the theme to egui when it or the UI size changes.
    fn ensure_style(&mut self, ctx: &egui::Context) {
        let key = (self.theme.name.clone(), self.ui_size());
        if self.styled_for.as_ref() != Some(&key) {
            ui::apply_style(ctx, &self.theme, self.ui_size());
            self.styled_for = Some(key);
        }
    }

    fn splitter(&mut self, ui: &mut Ui, rect: Rect, id: &str, vertical: bool) -> f32 {
        let resp = ui.interact(rect, Id::new(("splitter", id)), Sense::drag());
        let active = resp.hovered() || resp.dragged();
        if active {
            ui.ctx().set_cursor_icon(if vertical { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
            let c = crate::theme::with_alpha(self.theme.border_focused, if resp.dragged() { 200 } else { 110 });
            let r = if vertical { Rect::from_center_size(rect.center(), vec2(2.0, rect.height() - 2.0 * ISLAND_RADIUS as f32)) } else { Rect::from_center_size(rect.center(), vec2(rect.width() - 2.0 * ISLAND_RADIUS as f32, 2.0)) };
            ui.painter().rect_filled(r, CornerRadius::same(1), c);
        }
        if vertical { resp.drag_delta().x } else { resp.drag_delta().y }
    }

    fn layout_and_paint(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        let tk = self.tokens();
        ui.painter().rect_filled(full, CornerRadius::ZERO, tk.window);
        let title_h = crate::platform::NATIVE_TITLEBAR_H.max(ui::TITLE_BAR_H);
        let title = Rect::from_min_max(full.min, pos2(full.right(), full.top() + title_h));
        let status = Rect::from_min_max(pos2(full.left(), full.bottom() - STATUS_BAR_H), full.max);
        self.title_bar(ui, title);
        self.status_bar(ui, status);

        let body = Rect::from_min_max(pos2(full.left() + GAP, title.bottom()), pos2(full.right() - GAP, status.top()));
        let ctx = ui.ctx().clone();
        let has = |d: &DockState| d.open && !d.items.is_empty();
        let left_t = ctx.animate_bool_with_time(Id::new("left-anim"), has(&self.layout.left), 0.16);
        let right_t = ctx.animate_bool_with_time(Id::new("right-anim"), has(&self.layout.right), 0.16);
        let bottom_t = ctx.animate_bool_with_time(Id::new("bottom-anim"), has(&self.layout.bottom), 0.16);
        let ease = |t: f32| 1.0 - (1.0 - t).powi(3);

        let max_side = (body.width() * 0.45).max(200.0);
        self.layout.left.size = self.layout.left.size.clamp(180.0, max_side);
        self.layout.right.size = self.layout.right.size.clamp(280.0, max_side.max(300.0));
        let left_w = self.layout.left.size * ease(left_t);
        let below = self.layout.interactions_below;
        let right_w = if below { 0.0 } else { self.layout.right.size * ease(right_t) };

        let mut x = body.left();
        if left_w > 1.0 {
            let full_w = self.layout.left.size;
            let r = Rect::from_min_max(pos2(x + left_w - full_w, body.top()), pos2(x + left_w, body.bottom()));
            let clip = Rect::from_min_max(body.min, pos2(x + left_w, body.bottom()));
            self.island(ui, r, clip, |app, ui, rect| app.dock_ui(ui, rect, Dock::Left));
            x += left_w;
            let split = Rect::from_min_max(pos2(x, body.top()), pos2(x + GAP, body.bottom()));
            let d = self.splitter(ui, split, "left", true);
            self.layout.left.size += d;
            x += GAP;
        }
        let mut right_x = body.right();
        if right_w > 1.0 {
            let full_w = self.layout.right.size;
            let r = Rect::from_min_max(pos2(body.right() - right_w, body.top()), pos2(body.right() - right_w + full_w, body.bottom()));
            let clip = Rect::from_min_max(pos2(body.right() - right_w, body.top()), body.max);
            self.island(ui, r, clip, |app, ui, rect| app.dock_ui(ui, rect, Dock::Right));
            right_x = body.right() - right_w - GAP;
            let split = Rect::from_min_max(pos2(right_x, body.top()), pos2(right_x + GAP, body.bottom()));
            let d = self.splitter(ui, split, "right", true);
            self.layout.right.size -= d;
        }
        let center = Rect::from_min_max(pos2(x, body.top()), pos2(right_x, body.bottom()));
        // Bottom dock, and the right dock when DrRacket-style "below" layout is on.
        let bottom_h = self.layout.bottom.size.clamp(120.0, (center.height() * 0.7).max(140.0)) * ease(bottom_t);
        let mut editor_bottom = center.bottom();
        if bottom_h > 1.0 {
            let r = Rect::from_min_max(pos2(center.left(), center.bottom() - bottom_h), center.max);
            self.island(ui, r, r, |app, ui, rect| app.dock_ui(ui, rect, Dock::Bottom));
            editor_bottom = r.top() - GAP;
            let split = Rect::from_min_max(pos2(center.left(), editor_bottom), pos2(center.right(), editor_bottom + GAP));
            let d = self.splitter(ui, split, "bottom", false);
            self.layout.bottom.size -= d;
        }
        if below && right_t > 0.0 {
            let h = self.layout.right.size.min(center.height() * 0.6) * ease(right_t);
            let r = Rect::from_min_max(pos2(center.left(), editor_bottom - h), pos2(center.right(), editor_bottom));
            self.island(ui, r, r, |app, ui, rect| app.dock_ui(ui, rect, Dock::Right));
            editor_bottom = r.top() - GAP;
            let split = Rect::from_min_max(pos2(center.left(), editor_bottom), pos2(center.right(), editor_bottom + GAP));
            let d = self.splitter(ui, split, "below", false);
            self.layout.right.size -= d;
        }
        let editor_rect = Rect::from_min_max(center.min, pos2(center.right(), editor_bottom));
        self.island(ui, editor_rect, editor_rect, |app, ui, rect| app.editor_island(ui, rect));
    }

    /// Paint an island and run `content` inside it, clipped to its rounded rect.
    fn island(&mut self, ui: &mut Ui, rect: Rect, clip: Rect, content: impl FnOnce(&mut App, &mut Ui, Rect)) {
        let tk = self.tokens();
        let painter = ui.painter().with_clip_rect(clip);
        painter.rect(rect, CornerRadius::same(ISLAND_RADIUS), tk.island, Stroke::NONE, StrokeKind::Inside);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(clip.intersect(rect));
        content(self, &mut child, rect);
        // Border on top so content never covers the rounded edge.
        painter.rect_stroke(rect, CornerRadius::same(ISLAND_RADIUS), Stroke::new(1.0, tk.island_border), StrokeKind::Inside);
    }

    /// The editor island: tab strip + editor (or the welcome screen).
    fn editor_island(&mut self, ui: &mut Ui, rect: Rect) {
        if self.tabs.is_empty() {
            self.welcome_ui(ui, rect);
            return;
        }
        let tabs_rect = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_BAR_H));
        self.tab_strip(ui, tabs_rect);
        let ed_rect = Rect::from_min_max(pos2(rect.left() + 1.0, tabs_rect.bottom()), pos2(rect.right() - 1.0, rect.bottom() - ISLAND_RADIUS as f32 * 0.5));
        let font_size = self.font_size();
        let theme = self.theme.clone();
        let settings = self.settings.clone();
        let style = EditorStyle { theme: &theme, settings: &settings, font_size, mode: EditorMode::Code };
        let active = self.active;
        if let Some(tab) = self.tabs.get_mut(active) {
            let mut child = ui.new_child(UiBuilder::new().max_rect(ed_rect));
            child.set_clip_rect(ed_rect);
            tab.editor.show(&mut child, ed_rect, &style);
        }
        self.diagnostics_counter(ui, ed_rect);
        crate::racket_ui::after_editor(self);
    }

    /// Error / warning / test counters in the editor's top-right corner (as in the mockup).
    fn diagnostics_counter(&mut self, ui: &mut Ui, ed_rect: Rect) {
        let Some(tab) = self.active_tab() else { return };
        let errors = tab.editor.deco.diagnostics.iter().filter(|d| d.severity == crate::editor::view::Severity::Error).count();
        let warnings = tab.editor.deco.diagnostics.iter().filter(|d| d.severity == crate::editor::view::Severity::Warning).count();
        let passed = tab.editor.deco.inlays.iter().filter(|i| i.kind == crate::editor::view::InlayKind::Pass).count();
        let failed = tab.editor.deco.inlays.iter().filter(|i| i.kind == crate::editor::view::InlayKind::Fail).count();
        if errors + warnings + passed + failed == 0 || tab.editor.find.open {
            return;
        }
        let t = &self.theme;
        let mut items: Vec<(&str, Color32, usize)> = Vec::new();
        if errors > 0 {
            items.push((ui::ic::ERROR, t.error, errors));
        }
        if warnings > 0 {
            items.push((ui::ic::WARNING, t.warning, warnings));
        }
        if failed > 0 {
            items.push((ui::ic::CANCEL, t.error, failed));
        }
        if passed > 0 {
            items.push((ui::ic::DONE_ALL, t.success, passed));
        }
        let font = ui::medium(13.0);
        let mut w = 10.0;
        let galleys: Vec<_> = items.iter().map(|(_, c, n)| ui.painter().layout_no_wrap(n.to_string(), font.clone(), *c)).collect();
        for g in &galleys {
            w += 18.0 + g.size().x + 10.0;
        }
        let r = Rect::from_min_size(pos2(ed_rect.right() - w - 22.0, ed_rect.top() + 8.0), vec2(w, 26.0));
        let tk = self.tokens();
        ui.painter().rect(r, CornerRadius::same(7), crate::theme::with_alpha(tk.popup, 235), Stroke::new(1.0, tk.island_border), StrokeKind::Inside);
        let mut x = r.left() + 8.0;
        for ((glyph, c, _), g) in items.iter().zip(galleys) {
            ui.painter().text(pos2(x + 8.0, r.center().y), egui::Align2::CENTER_CENTER, *glyph, ui::icon_font_filled(16.0), *c);
            x += 18.0;
            ui.painter().galley(pos2(x, r.center().y - g.size().y / 2.0), g.clone(), *c);
            x += g.size().x + 10.0;
        }
        let resp = ui.interact(r, Id::new("diag-counter"), Sense::click());
        if resp.clicked() {
            self.run_command(Command::NextProblem);
        }
        resp.on_hover_text("Next problem (F8)");
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let screen = ctx.content_rect();
        let tk = self.tokens();
        let theme = self.theme.clone();
        let mut y = screen.bottom() - STATUS_BAR_H - 16.0;
        for (k, t) in self.toasts.iter().enumerate().rev() {
            let (glyph, color) = match t.kind {
                ToastKind::Info => (ui::ic::INFO, theme.info),
                ToastKind::Success => (ui::ic::CHECK_CIRCLE, theme.success),
                ToastKind::Error => (ui::ic::ERROR, theme.error),
            };
            let area = egui::Area::new(Id::new(("toast", k))).order(egui::Order::Tooltip).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(pos2(screen.right() - 16.0, y));
            let resp = area.show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_max_width(420.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(glyph).font(ui::icon_font_filled(18.0)).color(color));
                        ui.label(egui::RichText::new(&t.text).color(theme.text));
                    });
                });
            });
            y -= resp.response.rect.height() + 8.0;
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((t.until - now).max(0.05)));
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(mut auto) = self.automation.take() {
            auto.step(self, ctx);
            self.automation = Some(auto);
        }
        // Close requests: confirm unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed && self.tabs.iter().any(|t| t.dirty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.modal = Modal::ConfirmQuit;
        }
        if let Some(ws) = &mut self.workspace {
            ws.poll();
        }
        self.reload_changed_files();
        crate::racket_ui::poll(self, ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame_count += 1;
        self.ensure_style(&ctx);
        self.handle_shortcuts(&ctx);
        self.layout_and_paint(ui);
        self.modal_ui(&ctx);
        self.toasts_ui(&ctx);
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if self.automation.is_none() {
            self.save_session();
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.tokens().window.to_normalized_gamma_f32()
    }
}

/// Helper used by several panels: a header row with a title and trailing buttons.
pub fn panel_header(ui: &mut Ui, rect: Rect, theme: &Theme, title: &str) -> Rect {
    let r = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_BAR_H));
    ui.painter().text(pos2(r.left() + 14.0, r.center().y), egui::Align2::LEFT_CENTER, title, ui::medium(13.0), theme.text_muted);
    r
}

pub fn point_in(rect: Rect, p: Option<Pos2>) -> bool {
    p.is_some_and(|p| rect.contains(p))
}

pub fn vec_x(v: f32) -> Vec2 {
    vec2(v, 0.0)
}
