//! The application: workspace state, the islands layout, commands, files and sessions.

use crate::commands::{COMMANDS, Command};
use crate::config::{Recents, Settings};
use crate::editor::view::{EditorMode, EditorStyle, EditorView};
use crate::theme::{Theme, ThemeRegistry};
use crate::ui::{self, GAP, ISLAND_RADIUS, STATUS_BAR_H, TAB_BAR_H, Tokens};
use crate::workspace::Workspace;
use egui::{Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
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
    Visualizer,
    Docs,
    Problems,
    Debugger,
    Profiler,
    Modules,
    Log,
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
            PanelItem::Visualizer => "Visualizer",
            PanelItem::Docs => "Docs",
            PanelItem::Problems => "Problems",
            PanelItem::Debugger => "Debugger",
            PanelItem::Profiler => "Profiler",
            PanelItem::Modules => "Modules",
            PanelItem::Log => "Log",
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
            PanelItem::Visualizer => crate::visualizer::ICON,
            PanelItem::Docs => ic::MENU_BOOK,
            PanelItem::Problems => ic::ERROR,
            PanelItem::Debugger => ic::BUG_REPORT,
            PanelItem::Profiler => ic::SPEED,
            PanelItem::Modules => ic::HUB,
            PanelItem::Log => ic::NOTES,
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
                items: vec![PanelItem::Interactions, PanelItem::Stepper, PanelItem::Visualizer, PanelItem::Tests, PanelItem::Macros],
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
    /// Every open project tab (newer sessions); `current` is the one in front.
    pub projects: Vec<ProjectSession>,
    pub current: usize,
    /// Each file's editor zoom (only files not at 100%), kept after the file is closed.
    pub zooms: std::collections::BTreeMap<PathBuf, f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectSession {
    pub project: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub active: usize,
}

impl Session {
    fn path() -> PathBuf {
        crate::config::config_dir().join("session.json")
    }
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn write(json: &str) {
        let _ = std::fs::create_dir_all(crate::config::config_dir());
        let _ = crate::config::atomic_write(&Self::path(), json.as_bytes());
    }
}

pub struct Tab {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub untitled: usize,
    pub editor: EditorView,
    /// The language level (DrRacket's "Choose Language"); teaching languages are saved as a
    /// hidden DrRacket metadata header.
    pub language: crate::racket::lang::Language,
    pub lang_settings: crate::racket::lang::TeachingSettings,
    /// The language changed since the last save (the header must be rewritten).
    pub lang_changed: bool,
    pub disk_mtime: Option<SystemTime>,
    pub rk: crate::racket_ui::RacketTab,
    /// The editor's zoom (⌘+ / ⌘-), separate for each file; panels don't zoom.
    pub zoom: f32,
    /// (buffer version, time it was first seen) for autosave debouncing.
    edit_seen: (u64, f64),
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
        self.editor.doc.buffer.is_dirty() || self.lang_changed
    }
}

/// Modal overlays.
pub enum Modal {
    None,
    Palette(crate::ui::palette::Palette),
    ConfirmClose { tab: u64, then_quit: bool },
    ConfirmQuit,
    /// Close the current project tab, which has unsaved files.
    ConfirmCloseProject,
    Settings(crate::ui::settings_view::SettingsView),
    Language(crate::ui::language_dialog::LanguageDialog),
    Prompt(crate::ui::language_dialog::Prompt),
    About,
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
    dev_themes: Option<crate::dev::ThemeWatcher>,
    pub settings: Settings,
    pub themes: ThemeRegistry,
    pub theme: Theme,
    pub recents: Recents,
    pub workspace: Option<Workspace>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// Project tabs in the title bar; the current one's state lives in the three fields above.
    pub(crate) projects: Vec<crate::projects::ProjectSlot>,
    pub current_project: usize,
    pub closed: Vec<PathBuf>,
    pub layout: Layout,
    pub modal: Modal,
    pub toasts: Vec<Toast>,
    next_id: u64,
    /// Editor zoom by file (see `Tab::zoom`), remembered across sessions.
    pub file_zooms: std::collections::BTreeMap<PathBuf, f32>,
    pub sidebar: crate::ui::sidebar::SidebarState,
    pub ctx: egui::Context,
    styled_for: Option<(String, f32)>,
    pub quit_confirmed: bool,
    /// The tab whose visualizer was drawn this frame (its step is highlighted in the editor).
    pub viz_drawn: Option<usize>,
    /// The tab whose Stepper panel was drawn this frame.
    pub stepper_drawn: Option<usize>,
    /// Start the app again once it has closed (applying a new renderer).
    pub restart_on_exit: bool,
    pub updater: crate::update::Updater,
    /// The new version last announced in a toast (background checks announce each one once).
    update_announced: Option<String>,
    pub automation: Option<crate::automation::Automation>,
    pub frame_count: u64,
    pub terminals: Vec<TerminalTab>,
    pub racket: crate::racket_ui::RacketState,
    /// `DRROCKET_FRAME_STATS=1`: frame timing printed every 2 s.
    frame_stats: Option<FrameStats>,
    /// The session.json contents last written, and when it was last compared.
    saved_session: String,
    session_checked: f64,
}

/// Frame timing for `DRROCKET_FRAME_STATS`: our UI time, eframe's whole-frame CPU time
/// (UI + tessellation + rendering), the gap between frames, and input events per frame.
#[derive(Default)]
struct FrameStats {
    frames: u32,
    ui_total: std::time::Duration,
    ui_max: std::time::Duration,
    cpu_total: f32,
    cpu_max: f32,
    gap_max: std::time::Duration,
    scroll_events: u32,
    atlas_fill: f32,
    atlas_resets: u32,
    last: Option<std::time::Instant>,
    since: Option<std::time::Instant>,
}

impl FrameStats {
    fn record(&mut self, ui: std::time::Duration, cpu: Option<f32>, scroll_events: u32, atlas_fill: f32) {
        if atlas_fill + 0.05 < self.atlas_fill {
            self.atlas_resets += 1;
        }
        self.atlas_fill = atlas_fill;
        let now = std::time::Instant::now();
        if let Some(last) = self.last {
            self.gap_max = self.gap_max.max(now - last);
        }
        self.last = Some(now);
        let since = *self.since.get_or_insert(now);
        self.frames += 1;
        self.ui_total += ui;
        self.ui_max = self.ui_max.max(ui);
        let cpu = cpu.unwrap_or(0.0);
        self.cpu_total += cpu;
        self.cpu_max = self.cpu_max.max(cpu);
        self.scroll_events += scroll_events;
        let elapsed = now - since;
        if elapsed.as_secs_f32() >= 2.0 {
            let n = self.frames.max(1);
            eprintln!(
                "frame stats: {:.0} fps | ui avg {:?} max {:?} | frame cpu avg {:.1}ms max {:.1}ms | longest gap {:?} | {} scroll events | font atlas {:.0}% full, {} resets",
                self.frames as f32 / elapsed.as_secs_f32(),
                self.ui_total / n,
                self.ui_max,
                self.cpu_total * 1000.0 / n as f32,
                self.cpu_max * 1000.0,
                self.gap_max,
                self.scroll_events,
                self.atlas_fill * 100.0,
                self.atlas_resets
            );
            *self = FrameStats { last: self.last, atlas_fill: self.atlas_fill, ..Default::default() };
        }
    }
}

/// Draw calls, vertices and indices for the frame so far, per layer (diagnostics: software
/// renderers pay for each of these).
fn paint_summary(ctx: &egui::Context) -> String {
    let layers: Vec<egui::LayerId> = ctx.memory(|m| m.layer_ids().collect());
    let mut all = Vec::new();
    let mut per_layer = Vec::new();
    ctx.graphics_mut(|g| {
        for layer in &layers {
            let Some(list) = g.get_mut(*layer) else { continue };
            let n = list.next_idx().0;
            let mut shapes = Vec::with_capacity(n);
            for i in 0..n {
                list.mutate_shape(egui::layers::ShapeIdx(i), |s| shapes.push(s.clone()));
            }
            per_layer.push((format!("{:?}", layer.id), shapes.len()));
            all.extend(shapes);
        }
    });
    let prims = ctx.tessellate(all, ctx.pixels_per_point());
    let (mut verts, mut idx) = (0, 0);
    for p in &prims {
        if let egui::epaint::Primitive::Mesh(m) = &p.primitive {
            verts += m.vertices.len();
            idx += m.indices.len();
        }
    }
    per_layer.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    per_layer.truncate(4);
    format!("{} draw calls, {verts} vertices, {} triangles | most shapes: {per_layer:?}", prims.len(), idx / 3)
}

pub struct TerminalTab {
    pub id: u64,
    pub view: crate::terminal::TerminalView,
    pub focus: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Vec<PathBuf>) -> Self {
        crate::platform::setup_window(cc);
        if let Some(rs) = &cc.wgpu_render_state {
            crate::startup::set_adapter(&rs.adapter.get_info());
        }
        if std::env::var_os("DRROCKET_FRAME_STATS").is_some()
            && let Some(rs) = &cc.wgpu_render_state
        {
            let info = rs.adapter.get_info();
            eprintln!("renderer: {} ({:?}, {:?}, driver {} {}), surface {:?}", info.name, info.backend, info.device_type, info.driver, info.driver_info, rs.target_format);
        }
        crate::platform::set_repaint_context(&cc.egui_ctx);
        crate::platform::install_menu(crate::ui::brand::APP_NAME, &crate::commands::menu_specs());
        egui_extras::install_image_loaders(&cc.egui_ctx);
        ui::install_fonts(&cc.egui_ctx);
        let settings = Settings::load();
        let updater = crate::update::Updater::start(&cc.egui_ctx, settings.check_for_updates);
        let themes = ThemeRegistry::load();
        let theme = themes.get_or_default(&settings.theme);
        let mut recents = Recents::load();
        recents.prune();
        let mut app = App {
            dev_themes: crate::dev::ThemeWatcher::start(&cc.egui_ctx),
            settings,
            themes,
            theme,
            recents,
            workspace: None,
            tabs: Vec::new(),
            projects: vec![crate::projects::ProjectSlot::default()],
            current_project: 0,
            active: 0,
            closed: Vec::new(),
            layout: Layout::default(),
            modal: Modal::None,
            toasts: Vec::new(),
            next_id: 1,
            file_zooms: Session::load().zooms,
            sidebar: Default::default(),
            ctx: cc.egui_ctx.clone(),
            styled_for: None,
            quit_confirmed: false,
            viz_drawn: None,
            stepper_drawn: None,
            restart_on_exit: false,
            updater,
            update_announced: None,
            automation: crate::automation::Automation::from_env(),
            frame_count: 0,
            terminals: Vec::new(),
            racket: Default::default(),
            frame_stats: std::env::var("DRROCKET_FRAME_STATS").ok().map(|_| FrameStats::default()),
            saved_session: String::new(),
            session_checked: 0.0,
        };
        app.apply_theme_overrides();
        if !crate::crash::unseen().is_empty() && app.automation.is_none() {
            app.toast(crate::platform::keys("Dr. Rocket quit unexpectedly last time. A report was saved: choose Report a Problem… (☰ menu or ⇧⌘P) to copy it and send it to us."), ToastKind::Error);
        }
        if let Some((version, why)) = app.updater.take_failure() {
            app.toast(format!("Dr. Rocket couldn't install {version}: {why}. Download it from doctorocket.com instead."), ToastKind::Error);
        }
        let restore = app.settings.restore_session && open.is_empty() && app.automation.is_none();
        if restore {
            let s = Session::load();
            app.layout = s.layout.clone();
            // Docs has no permanent tab button; it opens on demand (F1, Search Documentation).
            app.layout.remove(PanelItem::Docs);
            // Panels added since the layout was saved.
            if app.layout.find(PanelItem::Visualizer).is_none() {
                let items = &mut app.layout.right.items;
                let at = items.iter().position(|i| *i == PanelItem::Stepper).map(|k| k + 1).unwrap_or(items.len());
                items.insert(at, PanelItem::Visualizer);
            }
            let projects = if s.projects.is_empty() {
                vec![ProjectSession { project: s.project.clone(), files: s.files.clone(), active: s.active }]
            } else {
                s.projects.clone()
            };
            let mut restored = 0;
            for p in &projects {
                let root = p.project.clone().filter(|r| r.is_dir());
                let files: Vec<&PathBuf> = p.files.iter().filter(|f| f.is_file()).collect();
                if root.is_none() && files.is_empty() {
                    continue;
                }
                if restored > 0 {
                    app.new_project();
                }
                restored += 1;
                if let Some(root) = root {
                    app.open_folder(&root);
                }
                for f in files {
                    app.open_file(f);
                }
                app.active = p.active.min(app.tabs.len().saturating_sub(1));
            }
            app.switch_project(s.current.min(app.projects.len().saturating_sub(1)));
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

    /// The code font size panels use (the REPL, tests, stepper...): the setting, unzoomed.
    pub fn font_size(&self) -> f32 {
        self.settings.buffer_font_size.clamp(8.0, 48.0)
    }

    /// Zoom the active file's editor (`None`: back to 100%), and remember it for the file.
    fn zoom_active(&mut self, step: Option<f32>) {
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        tab.zoom = match step {
            Some(d) => ((tab.zoom + d) * 10.0).round() / 10.0,
            None => 1.0,
        }
        .clamp(0.5, 2.5);
        if let Some(p) = &tab.path {
            if (tab.zoom - 1.0).abs() < 0.01 {
                self.file_zooms.remove(p);
            } else {
                self.file_zooms.insert(p.clone(), tab.zoom);
            }
        }
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
        self.apply_theme_overrides();
        if persist {
            self.settings.theme = self.theme.name.clone();
            let _ = self.settings.save();
        }
    }

    /// Re-derive the active theme from the registry plus user overrides (caret color,
    /// bracket palette). Call after the theme or those settings change.
    pub fn apply_theme_overrides(&mut self) {
        let mut t = self.themes.get_or_default(&self.theme.name);
        if let Some(c) = self.settings.cursor_color.as_deref().and_then(crate::theme::parse_color) {
            t.cursor = c;
        }
        let palette: Vec<_> = self.settings.rainbow_colors.iter().filter_map(|c| crate::theme::parse_color(c)).collect();
        if !palette.is_empty() {
            t.rainbow = palette;
        }
        self.theme = t;
        self.styled_for = None;
    }

    // ------------------------------------------------------------------ files

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn new_tab(&mut self, path: Option<PathBuf>, text: &str) -> usize {
        let id = self.alloc_id();
        let untitled = if path.is_none() { self.tabs.iter().filter(|t| t.path.is_none()).map(|t| t.untitled).max().unwrap_or(0) + 1 } else { 0 };
        let (language, lang_settings, body) = crate::racket_ui::load_text(self, path.as_deref(), text);
        let mut editor = EditorView::new(&body, Id::unique(("editor", id)));
        editor.doc.buffer.mark_saved();
        let disk_mtime = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        let mut rk = crate::racket_ui::RacketTab::new(id);
        if let Some(words) = self.racket.completions.get(&language.completions_language(&body.chars().take(400).collect::<String>())) {
            editor.completion_words = words.clone();
            rk.input.completion_words = words.clone();
        }
        let zoom = path.as_ref().and_then(|p| self.file_zooms.get(p)).copied().unwrap_or(1.0);
        self.tabs.push(Tab { id, path, untitled, editor, language, lang_settings, lang_changed: false, disk_mtime, rk, zoom, edit_seen: (0, 0.0) });
        self.tabs.len() - 1
    }

    /// The language new files start in: the one last chosen, with its options.
    pub fn default_language(&self) -> (crate::racket::lang::Language, crate::racket::lang::TeachingSettings) {
        let lang = crate::racket::lang::Language::from_id(&self.settings.default_language).unwrap_or(crate::racket::lang::Language::Module);
        let settings = self.settings.default_language_settings.clone().unwrap_or_else(|| lang.default_settings());
        (lang, settings)
    }

    pub fn new_file(&mut self) {
        let (lang, settings) = self.default_language();
        let initial = if lang == crate::racket::lang::Language::Module { "#lang racket\n\n".to_string() } else { String::new() };
        let i = self.new_tab(None, &initial);
        if lang != crate::racket::lang::Language::Module {
            self.tabs[i].language = lang;
            self.tabs[i].lang_settings = settings;
        }
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
        let (text, wxme) = match self.read_source(&path) {
            Ok(x) => x,
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
        if let Some(w) = wxme {
            self.tabs[i].editor.doc.images.load(&w.images);
            if !w.unsupported.is_empty() {
                // Saving would drop these boxes: keep DrRacket's file intact.
                self.tabs[i].editor.read_only = true;
                self.toast(format!("Opened read-only: this file has boxes Dr. Rocket can't edit yet ({}). Use DrRacket to change it.", w.unsupported.join(", ")), ToastKind::Info);
            } else if w.comments > 0 {
                self.toast(format!("Comment boxes are shown as #| … |# comments ({}).", w.comments), ToastKind::Info);
            }
        }
        self.active = i;
        self.tabs[i].editor.request_focus();
        self.recents.add_file(&path);
        if let Some(ws) = &mut self.workspace {
            ws.expand_to(&path);
        }
    }

    /// A file's program text. Files DrRacket saved in its editor format (with images or boxes)
    /// are read through Racket; their images come back alongside the text.
    fn read_source(&mut self, path: &Path) -> Result<(String, Option<crate::racket::protocol::WxmeFile>), String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if crate::racket_ui::is_wxme(&bytes) {
            let w = crate::racket_ui::read_wxme(self, path)?;
            return Ok((w.text.replace("\r\n", "\n"), Some(w)));
        }
        Ok((String::from_utf8_lossy(&bytes).replace("\r\n", "\n"), None))
    }

    /// Open a folder as a project tab (or switch to it if it is already open).
    pub fn open_folder(&mut self, path: &Path) {
        self.open_project(path);
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
        self.tabs[idx].path = Some(path.clone());
        let text = crate::racket_ui::save_text(&self.tabs[idx]);
        // Text with pasted images is saved in DrRacket's editor format; everything else as text.
        let images = self.tabs[idx].editor.doc.images.for_text(&text);
        let written = if images.is_empty() {
            crate::config::atomic_write(&path, text.as_bytes()).map_err(|e| e.to_string())
        } else {
            crate::racket_ui::write_wxme(self, &path, &text, images)
        };
        match written {
            Ok(()) => {
                let language = self.tabs[idx].language;
                crate::racket_ui::remember_language(self, &path, language);
                let tab = &mut self.tabs[idx];
                tab.editor.doc.buffer.mark_saved();
                tab.lang_changed = false;
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
        let stale: Vec<(usize, PathBuf)> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, tab)| {
                let p = tab.path.as_ref()?;
                let mtime = std::fs::metadata(p).ok().and_then(|m| m.modified().ok());
                (changed.contains(p) && mtime != tab.disk_mtime && !tab.dirty()).then(|| (i, p.clone()))
            })
            .collect();
        for (i, p) in stale {
            let mtime = std::fs::metadata(&p).ok().and_then(|m| m.modified().ok());
            if let Ok((text, wxme)) = self.read_source(&p) {
                // The editor shows the program without DrRacket's hidden header.
                let (_, _, body) = crate::racket_ui::load_text(self, Some(&p), &text);
                let tab = &mut self.tabs[i];
                if let Some(w) = wxme {
                    tab.editor.doc.images.load(&w.images);
                }
                let text = body;
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

    /// Save dirty files after the configured delay since their last edit.
    fn autosave(&mut self, now: f64) {
        if !self.settings.autosave {
            return;
        }
        let delay = self.settings.autosave_delay_ms as f64 / 1000.0;
        let mut due = Vec::new();
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let v = t.editor.doc.version();
            if t.edit_seen.0 != v {
                t.edit_seen = (v, now);
            }
            if t.path.is_some() && t.dirty() {
                if now - t.edit_seen.1 >= delay {
                    due.push(i);
                } else {
                    self.ctx.request_repaint_after(std::time::Duration::from_secs_f64(delay - (now - t.edit_seen.1) + 0.01));
                }
            }
        }
        for i in due {
            self.save_tab(i, false);
        }
    }

    /// Files dropped on the window open as tabs (folders open as the project).
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let p = f.path().to_path_buf();
            if p.as_os_str().is_empty() {
                continue;
            }
            if p.is_dir() {
                self.open_folder(&p);
            } else {
                self.open_file(&p);
            }
        }
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let screen = ctx.content_rect();
            let theme = self.theme.clone();
            egui::Area::new(Id::unique("drop-overlay")).order(egui::Order::Foreground).fixed_pos(screen.min).interactable(false).show(ctx, |ui| {
                let r = screen.shrink(10.0);
                ui.painter().rect(r, CornerRadius::same(14), crate::theme::with_alpha(theme.text_accent, 30), Stroke::new(2.0, crate::theme::with_alpha(theme.text_accent, 180)), StrokeKind::Inside);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "Drop to open", ui::semibold(22.0), theme.text);
            });
        }
    }

    fn session(&self) -> Session {
        let (projects, current) = self.project_sessions();
        Session {
            project: self.workspace.as_ref().map(|w| w.root.clone()),
            files: self.tabs.iter().filter_map(|t| t.path.clone()).collect(),
            active: self.active,
            layout: self.layout.without_terminals(),
            projects,
            current,
            zooms: self.file_zooms.clone(),
        }
    }

    pub fn save_session(&mut self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.session()) {
            Session::write(&json);
            self.saved_session = json;
        }
    }

    /// Keep session.json current as files and panels change (cheap: writes only on change),
    /// so a crash or a dev-loop restart reopens the same workspace.
    fn save_session_if_changed(&mut self, now: f64) {
        if self.automation.is_some() || now - self.session_checked < 1.0 {
            return;
        }
        self.session_checked = now;
        if let Ok(json) = serde_json::to_string_pretty(&self.session())
            && json != self.saved_session
        {
            Session::write(&json);
            self.saved_session = json;
        }
    }

    // ------------------------------------------------------------------ commands

    pub fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::NextProject | Command::PrevProject => {
                let n = self.projects.len();
                if n > 1 {
                    let step = if cmd == Command::NextProject { 1 } else { n - 1 };
                    self.switch_project((self.current_project + step) % n);
                }
            }
            // In a project, name the file up front (in the app); otherwise an untitled tab.
            Command::NewFile if self.workspace.is_some() => self.new_file_in_project(),
            Command::NewFile => self.new_file(),
            Command::NewFolder => self.new_folder_in_project(),
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
            Command::CloseFolder => self.close_project(self.current_project, false),
            Command::RevealInFinder => {
                if let Some(p) = self.active_tab().and_then(|t| t.path.clone()) {
                    crate::platform::reveal(&p);
                }
            }
            Command::Palette => self.modal = Modal::Palette(crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, "")),
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
                    t.editor.deco.editing = None; // asked for: include what is being typed
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
            Command::ZoomIn => self.zoom_active(Some(0.1)),
            Command::ZoomOut => self.zoom_active(Some(-0.1)),
            Command::ZoomReset => self.zoom_active(None),
            Command::ToggleVimMode => {
                self.settings.vim_mode = !self.settings.vim_mode;
                let _ = self.settings.save();
                self.toast(if self.settings.vim_mode { "Vim mode on: Space shows the leader keys, : opens commands" } else { "Vim mode off" }, ToastKind::Info);
            }
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
            Command::ReportProblem => {
                let report = crate::crash::problem_report(self.racket.version.as_deref());
                self.ctx.copy_text(report);
                crate::crash::mark_seen();
                self.toast("Problem report copied (version, system and the latest crash report). Paste it into a message, with what you were doing.", ToastKind::Info);
            }
            Command::CommentOutWithBox => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.doc.toggle_block_comment();
                }
            }
            Command::CommentOutWithDatum => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.doc.toggle_datum_comment();
                }
            }
            Command::Revert => {
                let Some(path) = self.active_tab().and_then(|t| t.path.clone()) else { return };
                match self.read_source(&path) {
                    Ok((text, wxme)) => {
                        let (_, _, body) = crate::racket_ui::load_text(self, Some(&path), &text);
                        if let Some(t) = self.active_tab_mut() {
                            if let Some(w) = wxme {
                                t.editor.doc.images.load(&w.images);
                            }
                            // One undoable edit: ⌘Z brings the discarded changes back.
                            let b = &mut t.editor.doc.buffer;
                            let len = b.len();
                            b.apply(vec![crate::editor::buffer::Replace { range: 0..len, text: body }], None, crate::editor::buffer::EditKind::Other);
                            b.mark_saved();
                            t.lang_changed = false;
                        }
                    }
                    Err(e) => self.toast(format!("Could not read {}: {e}", path.display()), ToastKind::Error),
                }
            }
            Command::Print => {
                let light = self.themes.get_or_default("One Light");
                let Some(t) = self.tabs.get_mut(self.active) else { return };
                let title = t.title();
                let page = crate::print::html(&title, &mut t.editor.doc, &light);
                let path = std::env::temp_dir().join(format!("dr-rocket-print-{}.html", t.id));
                match std::fs::write(&path, page) {
                    Ok(()) => {
                        let _ = open::that(&path);
                    }
                    Err(e) => self.toast(format!("Could not prepare printing: {e}"), ToastKind::Error),
                }
            }
            Command::InsertImage => {
                if self.active_tab().is_none_or(|t| t.editor.read_only) {
                    return;
                }
                let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]).pick_file() else { return };
                match std::fs::read(&path).ok().and_then(|b| crate::editor::images::EmbeddedImage::from_file_bytes(&b)) {
                    Some(img) => {
                        if let Some(t) = self.active_tab_mut() {
                            t.editor.insert_image(img);
                        }
                    }
                    None => self.toast(format!("Could not read {} as an image.", path.display()), ToastKind::Error),
                }
            }
            Command::ReindentAll => {
                if let Some(t) = self.active_tab_mut() {
                    t.editor.doc.reindent_all();
                }
            }
            Command::Settings => self.modal = Modal::Settings(crate::ui::settings_view::SettingsView::default()),
            Command::About => self.modal = Modal::About,
            Command::CheckForUpdates => {
                if self.updater.check_now() {
                    self.toast("Checking for updates…", ToastKind::Info);
                } else {
                    self.toast("Update checks are off in this run.", ToastKind::Info);
                }
            }
            Command::RestartToUpdate => match self.updater.ready() {
                Some(_) => {
                    self.restart_on_exit = true;
                    self.quit();
                }
                None => self.toast("No update is ready to install. Choose Check for Updates… to look for one.", ToastKind::Info),
            },
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

    pub fn refresh_tree(&mut self) {
        if let Some(ws) = &mut self.workspace {
            ws.invalidate();
        }
    }

    /// Open a terminal whose working directory is `dir`.
    pub fn terminal_at(&mut self, dir: &Path, dock: Dock) {
        match crate::terminal::TerminalView::spawn(dir, self.settings.shell.as_deref(), &self.ctx) {
            Ok(view) => {
                let id = self.alloc_id();
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
        // egui matches shortcuts ignoring extra Shift/Alt (so ⌘+ works whether "+" needs Shift),
        // so check the most specific first: ⇧⌘S must not be taken by ⌘S, nor ⌃⇧Tab by ⌃Tab.
        let mut shortcuts: Vec<(egui::KeyboardShortcut, Command)> = COMMANDS.iter().filter_map(|c| c.shortcut.map(|s| (s, c.cmd))).collect();
        let weight = |m: egui::Modifiers| [m.alt, m.ctrl, m.shift, m.mac_cmd || m.command].iter().filter(|b| **b).count();
        shortcuts.sort_by_key(|(s, _)| std::cmp::Reverse(weight(s.modifiers)));
        let mut fired = Vec::new();
        ctx.input_mut(|i| {
            for (s, cmd) in &shortcuts {
                // Except on symbol keys, Shift and Alt must match exactly: ⇧⌘L is the editor's
                // select-all-occurrences, not ⌘L.
                let symbol = s.logical_key.name().chars().count() == 1 && !s.logical_key.name().chars().all(char::is_alphanumeric);
                if !symbol && (i.modifiers.shift != s.modifiers.shift || i.modifiers.alt != s.modifiers.alt) {
                    continue;
                }
                if i.consume_shortcut(s) {
                    fired.push(*cmd);
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
        let resp = ui.interact(rect, Id::unique(("splitter", id)), Sense::drag());
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
        let left_t = ctx.animate_bool_with_time(Id::unique("left-anim"), has(&self.layout.left), 0.16);
        let right_t = ctx.animate_bool_with_time(Id::unique("right-anim"), has(&self.layout.right), 0.16);
        let bottom_t = ctx.animate_bool_with_time(Id::unique("bottom-anim"), has(&self.layout.bottom), 0.16);
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
        let zoom = self.tabs.get(self.active).map_or(1.0, |t| t.zoom);
        let font_size = (self.font_size() * zoom).clamp(6.0, 60.0);
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
        crate::racket_ui::call_box(self, ui, ed_rect);
        crate::racket_ui::after_editor(self);
    }

    /// Error / warning / test counters in the editor's top-right corner (as in the mockup).
    fn diagnostics_counter(&mut self, ui: &mut Ui, ed_rect: Rect) {
        let Some(tab) = self.active_tab() else { return };
        let errors = tab.editor.deco.shown_diagnostics().filter(|d| d.severity == crate::editor::view::Severity::Error).count();
        let warnings = tab.editor.deco.shown_diagnostics().filter(|d| d.severity == crate::editor::view::Severity::Warning).count();
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
        let resp = ui.interact(r, Id::unique("diag-counter"), Sense::click());
        if resp.clicked() {
            self.run_command(Command::NextProblem);
        }
        resp.on_hover_text("Next problem (F8)");
    }

    /// Quit, asking first about unsaved changes.
    pub fn quit(&mut self) {
        if !self.all_dirty_titles().is_empty() {
            self.modal = Modal::ConfirmQuit;
        } else {
            self.quit_confirmed = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Tell the user about update checks as they finish.
    fn poll_updates(&mut self) {
        use crate::update::Status;
        self.updater.set_enabled(self.settings.check_for_updates);
        let Some((status, asked)) = self.updater.take_change() else { return };
        if let Status::Ready { version } | Status::Available { version, .. } = &status {
            if !asked && self.update_announced.as_ref() == Some(version) {
                return;
            }
            self.update_announced = Some(version.clone());
        }
        match status {
            Status::Ready { version } => self.toast(format!("Dr. Rocket {version} is ready. It installs when you quit, or choose Restart to Update in the status bar."), ToastKind::Success),
            Status::Available { version, why } => self.toast(format!("Dr. Rocket {version} is available. {why} Click Update in the status bar to download it."), ToastKind::Info),
            Status::UpToDate if asked => self.toast(format!("Dr. Rocket is up to date ({}).", crate::update::CURRENT), ToastKind::Success),
            Status::Failed(e) if asked => self.toast(e, ToastKind::Error),
            _ => {}
        }
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
            let area = egui::Area::new(Id::unique(("toast", k))).order(egui::Order::Tooltip).pivot(egui::Align2::RIGHT_BOTTOM).fixed_pos(pos2(screen.right() - 16.0, y));
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
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(mut auto) = self.automation.take() {
            auto.step(self, ctx, raw);
            self.automation = Some(auto);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        crate::startup::frame_painted(ctx.cumulative_frame_nr());
        if self.dev_themes.as_ref().is_some_and(|w| w.changed()) {
            self.themes = ThemeRegistry::load();
            self.apply_theme_overrides();
        }
        if self.automation.is_some() {
            // Scripted runs keep rendering even when macOS considers the window occluded
            // (display asleep, other Space). Holds for one frame, so send it every frame.
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        }
        // Close requests: confirm unsaved changes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed && !self.all_dirty_titles().is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.modal = Modal::ConfirmQuit;
        }
        for action in crate::platform::take_menu_actions() {
            match action {
                crate::platform::MenuAction::Quit => self.quit(),
                crate::platform::MenuAction::Command(i) => {
                    if let Some(c) = COMMANDS.get(i) {
                        // Menus act like their shortcuts: ignored while a dialog is open,
                        // except for commands that open one.
                        if matches!(self.modal, Modal::None) {
                            self.run_command(c.cmd);
                        }
                    }
                }
            }
        }
        for p in crate::platform::take_opened_files() {
            if p.is_dir() {
                self.open_folder(&p);
            } else {
                self.open_file(&p);
            }
        }
        if let Some(ws) = &mut self.workspace {
            ws.poll();
        }
        self.reload_changed_files();
        let now = ctx.input(|i| i.time);
        self.autosave(now);
        self.save_session_if_changed(now);
        crate::racket_ui::poll(self, ctx);
        self.poll_updates();
    }

    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let started = std::time::Instant::now();
        let ctx = ui.ctx().clone();
        self.frame_count += 1;
        crate::platform::tag_layer_display_p3();
        if self.frame_count == 3 && std::env::var_os("DRROCKET_TRACE").is_some() {
            crate::platform::trace_layer_colorspace();
        }
        self.ensure_style(&ctx);
        self.handle_shortcuts(&ctx);
        self.viz_drawn = None;
        self.stepper_drawn = None;
        self.layout_and_paint(ui);
        // The visualizer's highlight and note belong to a visible visualizer: clear them from
        // every other tab (they come back when it's shown again).
        for (k, tab) in self.tabs.iter_mut().enumerate() {
            if Some(k) != self.viz_drawn
                && let Some(v) = tab.rk.viz.as_mut()
                && v.highlighted.is_some()
            {
                v.highlighted = None;
                tab.editor.deco.marks.retain(|m| !matches!(m.kind, crate::editor::view::MarkKind::Viz | crate::editor::view::MarkKind::VizWork));
                tab.editor.deco.viz_notes.clear();
            }
            // Likewise the stepper's highlight of the current step.
            if Some(k) != self.stepper_drawn
                && let Some(st) = tab.rk.stepper.as_mut()
                && st.highlighted
            {
                st.highlighted = false;
                tab.editor.deco.marks.retain(|m| m.kind != crate::editor::view::MarkKind::StepBefore);
            }
        }
        self.modal_ui(&ctx);
        self.toasts_ui(&ctx);
        self.handle_dropped_files(&ctx);
        if let Some(stats) = &mut self.frame_stats {
            let scrolls = ctx.input(|i| i.events.iter().filter(|e| matches!(e, egui::Event::MouseWheel { .. })).count() as u32);
            let fill = ctx.fonts(|f| f.font_atlas_fill_ratio());
            stats.record(started.elapsed(), frame.info().cpu_usage, scrolls, fill);
            // `DRROCKET_FRAME_STATS=paint`: what this frame hands the renderer, every 60 frames.
            if self.frame_count.is_multiple_of(60) && std::env::var("DRROCKET_FRAME_STATS").is_ok_and(|v| v == "paint") {
                eprintln!("paint: {}", paint_summary(&ctx));
            }
            if std::env::var("DRROCKET_FRAME_STATS").is_ok_and(|v| v == "causes") {
                for cause in ctx.repaint_causes() {
                    eprintln!("repaint cause: {cause}");
                }
            }
            // `DRROCKET_FRAME_STATS=continuous`: redraw every frame to measure render throughput.
            if std::env::var("DRROCKET_FRAME_STATS").is_ok_and(|v| v == "continuous") {
                ctx.request_repaint();
            }
        }
    }

    fn on_exit(&mut self) {
        // A downloaded update goes in now; with a restart, the new version is what starts.
        if self.updater.install(self.restart_on_exit) {
            return;
        }
        if self.restart_on_exit {
            crate::startup::spawn_again();
        }
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
