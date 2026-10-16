//! Glue between the UI and the Racket backend: Run + REPL, online check syntax, hover docs and
//! the blue box, go to definition, rename, the stepper, debugger, macro stepper, tests,
//! coverage, profiler and documentation search.

use crate::app::{App, Dock, Modal, PanelItem, Tab, ToastKind};
use crate::commands::Command;
use crate::editor::highlight::racket_job;
use crate::editor::view::{self, Arrow, ArrowKind, Diagnostic, EditorAction, EditorMode, EditorStyle, EditorView, Inlay, InlayKind, Mark, MarkKind, Semantic, Severity};
use crate::racket::lang::{self, Language, TeachingSettings};
use crate::racket::protocol::{self as proto, CheckSyntaxResult, DebugAction, ErrorReport, Instrument, PausedState, ProfileReport, RunEvent, RunOptions, Segment, ServerRequest, ServerResponse, Srcloc, Step, StepperEvent, TestReport};
use crate::racket::runner::{IdleRunner, RunConfig, RunSession};
use crate::racket::server::{RequestId, Server};
use crate::racket::stepper::{StepRequest, StepperSession};
use crate::racket::{Backend, BackendStatus, RacketInstall};
use crate::theme::{mix, with_alpha};
use crate::ui::widgets::{empty_state, icon_button_sized, search_field, truncated_label};
use crate::ui::{self, Tokens, ic};
use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

// =====================================================================================
// State

enum Pending {
    CheckSyntax { tab: u64, version: u64, explicit: bool },
    Hover { tab: u64, pos: usize, version: u64 },
    CallBox { key: String },
    Docs,
    Completions { key: String },
    MacroSteps { tab: u64 },
    Modules { tab: u64 },
}

/// App-wide Racket state.
#[derive(Default)]
pub struct RacketState {
    pub install: Option<RacketInstall>,
    pub backend: Option<Backend>,
    pub server: Option<Server>,
    idle: Option<IdleRunner>,
    pending: HashMap<RequestId, Pending>,
    pub completions: HashMap<String, Arc<Vec<String>>>,
    completions_requested: HashSet<String>,
    call_boxes: HashMap<String, Option<Vec<String>>>,
    pub version: Option<String>,
    version_rx: Option<Receiver<Option<String>>>,
    pub missing: bool,
    started: bool,
    pub docs: DocsPanel,
    lang_memory: LangMemory,
}

#[derive(Default)]
pub struct DocsPanel {
    pub query: String,
    searched: String,
    pub results: Vec<proto::DocsHit>,
    pub searching: bool,
    focus: bool,
}

/// Remembers the language of files whose language is not recorded in the file itself
/// (R5RS, Pretty Big), like DrRacket's preferences do.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct LangMemory {
    files: HashMap<PathBuf, String>,
}

impl LangMemory {
    fn path() -> PathBuf {
        crate::config::config_dir().join("languages.json")
    }
    fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn save(&self) {
        let _ = std::fs::create_dir_all(crate::config::config_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = crate::config::atomic_write(&Self::path(), s.as_bytes());
        }
    }
}

/// An entry in the interactions transcript.
pub enum Entry {
    Banner(String),
    Input(String),
    Output { text: String, err: bool },
    Value(Vec<ValuePart>),
    /// An error; `from_definitions` is false for errors raised by an interaction.
    Error(ErrorReport, bool),
    Tests(TestReport),
    Info(String),
}

pub enum ValuePart {
    Text(String),
    Image(egui::TextureHandle, [f32; 2]),
}

pub struct StepperView {
    session: Option<StepperSession>,
    pub steps: Vec<StepItem>,
    pub current: usize,
    pub finished: bool,
    pub paused: bool,
    pub error: Option<String>,
}

pub enum StepItem {
    Step(Step),
    Error { before: Vec<proto::StepExpr>, message: String, before_src: Option<proto::SrcSpan> },
}

pub struct ModulesView {
    pub graph: Option<proto::ModuleGraph>,
    pub include_libs: bool,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct MacroView {
    pub steps: Vec<proto::MacroStep>,
    pub current: usize,
    pub loading: bool,
}

/// Per-tab Racket state.
pub struct RacketTab {
    pub session: Option<RunSession>,
    pub transcript: Vec<Entry>,
    pub input: EditorView,
    pub history: Vec<String>,
    hist_pos: Option<usize>,
    draft: String,
    pub busy: bool,
    /// The definitions are still running (errors then point into the definitions).
    defs_running: bool,
    pub waiting_input: bool,
    queued: Vec<String>,
    pub tests: Option<TestReport>,
    pub profile: Option<ProfileReport>,
    pub paused: Option<PausedState>,
    pub instrument: Instrument,
    pub stepper: Option<StepperView>,
    pub macros: Option<MacroView>,
    pub modules: Option<ModulesView>,
    pub check: Option<CheckSyntaxResult>,
    check_due: Option<f64>,
    check_req: Option<RequestId>,
    checked_version: u64,
    last_seen_version: u64,
    pub call_box: Option<(String, Vec<String>)>,
    scroll_to_bottom: bool,
}

impl RacketTab {
    pub fn new(id: u64) -> Self {
        let mut input = EditorView::new("", Id::unique(("repl-input", id)));
        input.doc.buffer.mark_saved();
        Self {
            session: None,
            transcript: Vec::new(),
            input,
            history: Vec::new(),
            hist_pos: None,
            draft: String::new(),
            busy: false,
            defs_running: false,
            waiting_input: false,
            queued: Vec::new(),
            tests: None,
            profile: None,
            paused: None,
            instrument: Instrument::Errortrace,
            stepper: None,
            macros: None,
            modules: None,
            check: None,
            check_due: None,
            check_req: None,
            checked_version: u64::MAX,
            last_seen_version: u64::MAX,
            call_box: None,
            scroll_to_bottom: false,
        }
    }

    fn push(&mut self, e: Entry) {
        // Coalesce consecutive output of the same stream.
        if let Entry::Output { text, err } = &e
            && let Some(Entry::Output { text: last, err: le }) = self.transcript.last_mut()
            && le == err
        {
            last.push_str(text);
            const CAP: usize = 400_000;
            if last.len() > CAP {
                let cut = last.len() - CAP;
                let cut = (cut..last.len()).find(|&i| last.is_char_boundary(i)).unwrap_or(last.len());
                last.drain(..cut);
            }
            self.scroll_to_bottom = true;
            return;
        }
        self.transcript.push(e);
        if self.transcript.len() > 3000 {
            self.transcript.drain(..500);
        }
        self.scroll_to_bottom = true;
    }
}

// =====================================================================================
// Language helpers used by app.rs

/// Load a file: detect its language (DrRacket header, `#lang`, or remembered) and strip the header.
pub fn load_text(app: &App, path: Option<&Path>, text: &str) -> (Language, TeachingSettings, String) {
    let loaded = lang::load(text);
    let mut language = loaded.language;
    if loaded.header.is_empty()
        && lang::hash_lang(&loaded.body).is_none()
        && let Some(p) = path
        && let Some(id) = app.racket.lang_memory.files.get(p)
        && let Some(l) = Language::from_id(id)
    {
        language = l;
    }
    let settings = if loaded.header.is_empty() && language != loaded.language { language.default_settings() } else { loaded.settings };
    (language, settings, loaded.body)
}

pub fn save_text(tab: &Tab) -> String {
    lang::save(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text())
}

pub fn remember_language(app: &mut App, path: &Path, language: Language) {
    if language.reader_module().is_none() && language != Language::Module {
        app.racket.lang_memory.files.insert(path.to_path_buf(), language.id().to_string());
        app.racket.lang_memory.save();
    } else if app.racket.lang_memory.files.remove(path).is_some() {
        app.racket.lang_memory.save();
    }
}

pub fn language_label(tab: &Tab) -> String {
    if tab.language == Language::Module {
        return match lang::hash_lang(&tab.editor.doc.head(400)) {
            Some(l) => format!("#lang {l}"),
            None => "Racket".into(),
        };
    }
    tab.language.short_name().to_string()
}

// =====================================================================================
// Backend lifecycle

fn ensure_backend(app: &mut App) {
    let st = &mut app.racket;
    if st.started {
        return;
    }
    st.started = true;
    st.lang_memory = LangMemory::load();
    let Some(install) = crate::racket::discover(app.settings.racket_path.as_deref()) else {
        st.missing = true;
        return;
    };
    // The version check spawns Racket, so do it off the UI thread.
    let (tx, rx) = std::sync::mpsc::channel();
    let inst = install.clone();
    let ctx = app.ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(inst.version());
        ctx.request_repaint();
    });
    st.version_rx = Some(rx);
    match Backend::start(install.clone(), Some(app.ctx.clone())) {
        Ok(b) => {
            st.server = Server::start(&b).ok();
            st.idle = IdleRunner::spawn(&b).ok();
            st.backend = Some(b);
        }
        Err(e) => app.toasts.push(crate::app::Toast { text: format!("Could not start Racket: {e}"), kind: ToastKind::Error, until: f64::MAX }),
    }
    app.racket.install = Some(install);
}

fn request(app: &mut App, req: ServerRequest, pending: Pending) -> Option<RequestId> {
    ensure_backend(app);
    let server = app.racket.server.as_mut()?;
    let id = server.request(req);
    app.racket.pending.insert(id, pending);
    Some(id)
}

fn tab_index(app: &App, id: u64) -> Option<usize> {
    app.tabs.iter().position(|t| t.id == id)
}

fn path_string(tab: &Tab) -> Option<String> {
    tab.path.as_ref().map(|p| p.to_string_lossy().into_owned())
}

/// Does this srcloc refer to the tab's own buffer?
fn is_local(tab: &Tab, loc: &Srcloc) -> bool {
    match (&loc.path, &tab.path) {
        (None, _) => true,
        (Some(p), Some(tp)) => Path::new(p) == tp.as_path() || std::fs::canonicalize(p).ok().as_deref() == Some(tp.as_path()),
        _ => false,
    }
}

// =====================================================================================
// Per-frame polling

pub fn poll(app: &mut App, ctx: &egui::Context) {
    ensure_backend(app);
    if let Some(rx) = &app.racket.version_rx
        && let Ok(v) = rx.try_recv()
    {
        app.racket.version = v;
        app.racket.version_rx = None;
    }
    // Language completions for open tabs.
    let keys: Vec<String> = app.tabs.iter().map(|t| t.language.completions_language(&t.editor.doc.head(400))).collect();
    for key in keys {
        if !app.racket.completions_requested.contains(&key) {
            app.racket.completions_requested.insert(key.clone());
            request(app, ServerRequest::Completions { language: key.clone() }, Pending::Completions { key });
        }
    }
    poll_server(app);
    for i in 0..app.tabs.len() {
        poll_session(app, i, ctx);
        poll_stepper(app, i);
        schedule_check(app, i, ctx);
    }
}

fn poll_server(app: &mut App) {
    let Some(server) = app.racket.server.as_mut() else { return };
    server.poll();
    let results = server.take_all();
    for (id, res) in results {
        let Some(p) = app.racket.pending.remove(&id) else { continue };
        match (p, res) {
            (Pending::CheckSyntax { tab, version, explicit }, Ok(ServerResponse::CheckSyntax(r))) => apply_check(app, tab, version, explicit, r),
            (Pending::CheckSyntax { explicit: true, .. }, Err(e)) => app.toast(format!("Check Syntax failed: {e}"), ToastKind::Error),
            (Pending::Hover { tab, pos, version }, Ok(ServerResponse::Bluebox(b))) => {
                if let Some(i) = tab_index(app, tab)
                    && app.tabs[i].editor.doc.version() == version
                    && let Some(b) = b
                {
                    let ed = &mut app.tabs[i].editor;
                    let end = ed.doc.symbol_at(pos).map(|s| s.0.end).unwrap_or(pos + 1);
                    ed.deco.hover_doc = Some((pos..end, b.lines));
                }
            }
            (Pending::CallBox { key }, Ok(ServerResponse::Bluebox(b))) => {
                app.racket.call_boxes.insert(key, b.map(|b| b.lines));
            }
            (Pending::CallBox { key }, Err(_)) => {
                app.racket.call_boxes.insert(key, None);
            }
            (Pending::Docs, Ok(ServerResponse::DocsSearch(hits))) => {
                app.racket.docs.results = hits;
                app.racket.docs.searching = false;
            }
            (Pending::Completions { key }, Ok(ServerResponse::Completions(words))) => {
                let words = Arc::new(words);
                for t in app.tabs.iter_mut() {
                    if t.language.completions_language(&t.editor.doc.head(400)) == key {
                        t.editor.completion_words = words.clone();
                        t.rk.input.completion_words = words.clone();
                    }
                }
                app.racket.completions.insert(key, words);
            }
            (Pending::MacroSteps { tab }, Ok(ServerResponse::MacroSteps(steps))) => {
                if let Some(i) = tab_index(app, tab) {
                    app.tabs[i].rk.macros = Some(MacroView { steps, current: 0, loading: false });
                }
            }
            (Pending::MacroSteps { tab }, Err(e)) => {
                if let Some(i) = tab_index(app, tab) {
                    app.tabs[i].rk.macros = None;
                }
                app.toast(format!("Macro stepper: {e}"), ToastKind::Error);
            }
            (Pending::Modules { tab }, res) => {
                if let Some(i) = tab_index(app, tab)
                    && let Some(mv) = app.tabs[i].rk.modules.as_mut()
                {
                    mv.loading = false;
                    match res {
                        Ok(ServerResponse::ModuleGraph(g)) => {
                            mv.graph = Some(g);
                            mv.error = None;
                        }
                        Err(e) => mv.error = Some(e),
                        Ok(_) => {}
                    }
                }
            }
            (_, Err(e)) if e != "cancelled" => log_error(&e),
            _ => {}
        }
    }
}

fn log_error(e: &str) {
    eprintln!("drrocket: backend: {e}");
}

// ---------------------------------------------------------------------------------- check syntax

fn schedule_check(app: &mut App, i: usize, ctx: &egui::Context) {
    let now = ctx.input(|inp| inp.time);
    let online = app.settings.online_check_syntax;
    let tab = &mut app.tabs[i];
    let v = tab.editor.doc.version();
    if v != tab.rk.last_seen_version {
        tab.rk.last_seen_version = v;
        // Edits invalidate semantic info that the shift logic cannot keep exact.
        if tab.rk.checked_version != v && online {
            tab.rk.check_due = Some(now + 0.45);
            ctx.request_repaint_after(std::time::Duration::from_millis(460));
        }
    }
    if let Some(due) = tab.rk.check_due
        && now >= due
    {
        tab.rk.check_due = None;
        start_check(app, i, false);
    }
}

fn start_check(app: &mut App, i: usize, explicit: bool) {
    let tab = &app.tabs[i];
    let (source, offset) = lang::program_text(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
    let req = ServerRequest::CheckSyntax { path: path_string(tab), source, offset };
    let (id, version) = (tab.id, tab.editor.doc.version());
    if let Some(old) = app.tabs[i].rk.check_req.take()
        && let Some(s) = app.racket.server.as_mut()
    {
        s.cancel(old);
    }
    let rid = request(app, req, Pending::CheckSyntax { tab: id, version, explicit });
    app.tabs[i].rk.check_req = rid;
}

fn apply_check(app: &mut App, tab_id: u64, version: u64, explicit: bool, r: CheckSyntaxResult) {
    let Some(i) = tab_index(app, tab_id) else { return };
    let tab = &mut app.tabs[i];
    if tab.editor.doc.version() != version {
        return; // stale; a newer check is scheduled
    }
    tab.rk.check_req = None;
    tab.rk.checked_version = version;
    let sem = Semantic {
        arrows: r
            .arrows
            .iter()
            .filter(|a| a.actual)
            .map(|a| Arrow {
                from: a.from.0..a.from.1,
                to: a.to.0..a.to.1,
                kind: match a.kind {
                    proto::ArrowKind::Lexical => ArrowKind::Lexical,
                    proto::ArrowKind::Imported => ArrowKind::Imported,
                    proto::ArrowKind::Tail => ArrowKind::Tail,
                },
            })
            .collect(),
        tooltips: r.tooltips.iter().map(|t| (t.start..t.end, t.text.clone())).collect(),
        unused: r.colors.iter().filter(|c| c.style.contains("unused")).map(|c| c.start..c.end).collect(),
        docs: r.docs.iter().filter_map(|d| d.url().map(|u| (d.start..d.end, u))).collect(),
    };
    tab.editor.deco.semantic = Some(sem);
    // Syntax errors become diagnostics; keep runtime-error diagnostics from Run.
    tab.editor.deco.diagnostics.retain(|d| d.severity != Severity::Warning && !d.message.starts_with('\u{200b}'));
    let mut first_error = None;
    for e in &r.errors {
        let locs: Vec<(usize, usize)> = e.srclocs.iter().filter(|l| l.path.is_none() || is_local(tab, l)).filter_map(|l| l.range()).collect();
        for (s, end) in locs {
            let range = s..end.max(s + 1);
            first_error.get_or_insert(range.clone());
            tab.editor.deco.diagnostics.push(Diagnostic { range, severity: Severity::Error, message: format!("\u{200b}{}", e.message) });
        }
        if e.srclocs.is_empty() && explicit {
            app.toasts.push(crate::app::Toast { text: e.message.clone(), kind: ToastKind::Error, until: app.ctx.input(|i| i.time) + 6.0 });
        }
    }
    for r in &r.unused_requires {
        tab.editor.deco.diagnostics.push(Diagnostic { range: r.0..r.1, severity: Severity::Warning, message: "\u{200b}Unused require".into() });
    }
    if std::env::var_os("DRROCKET_TRACE").is_some() {
        for d in &tab.editor.deco.diagnostics {
            eprintln!("diagnostic {:?} line {}: {}", d.severity, tab.editor.doc.buffer.line_of(d.range.start) + 1, d.message);
        }
    }
    tab.rk.check = Some(r);
    if explicit {
        match first_error {
            Some(range) => tab.editor.reveal(range),
            None => app.toast("Check Syntax: no errors", ToastKind::Success),
        }
    }
}

// ---------------------------------------------------------------------------------- run

fn start_run(app: &mut App, i: usize, instrument: Instrument) {
    ensure_backend(app);
    let Some(backend) = app.racket.backend.as_ref() else {
        app.toast("Racket was not found. Install Racket or set its path in Settings.", ToastKind::Error);
        return;
    };
    let tab = &mut app.tabs[i];
    tab.rk.session = None; // kills the previous program (and its windows)
    let (source, offset) = lang::program_text(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
    let breakpoints: Vec<usize> = tab.editor.deco.breakpoints.iter().map(|&l| {
        let b = &tab.editor.doc.buffer;
        b.line_start(l) + b.indentation(l)
    }).collect();
    let config = RunConfig {
        path: tab.path.clone(),
        source,
        offset,
        language: tab.language.run_language(),
        options: RunOptions { instrument, memory_limit_mb: Some(1024), breakpoints, print_width: 80, ..Default::default() },
    };
    let session = match app.racket.idle.take() {
        Some(idle) => RunSession::start_with(idle, &config),
        None => RunSession::start(backend, &config),
    };
    app.racket.idle = IdleRunner::spawn(backend).ok();
    let tab = &mut app.tabs[i];
    match session {
        Ok(s) => {
            tab.rk.session = Some(s);
            tab.rk.busy = true;
            tab.rk.defs_running = true;
            tab.rk.waiting_input = false;
            tab.rk.paused = None;
            tab.rk.tests = None;
            tab.rk.instrument = instrument;
            if instrument != Instrument::Profile {
                tab.rk.profile = None;
            }
            tab.rk.transcript.clear();
            let version = app.racket.version.clone().unwrap_or_default();
            let lang_name = if tab.language == Language::Module { language_label(tab) } else { tab.language.name().to_string() };
            tab.rk.push(Entry::Banner(format!("Welcome to Racket {version}\nLanguage: {lang_name}")));
            // Clear run decorations.
            tab.editor.deco.marks.retain(|m| m.kind == MarkKind::StepBefore);
            tab.editor.deco.inlays.clear();
            tab.editor.deco.diagnostics.retain(|d| d.message.starts_with('\u{200b}'));
            app.layout.show(PanelItem::Interactions, Dock::Right);
            if instrument == Instrument::Debug {
                app.layout.show(PanelItem::Debugger, Dock::Right);
                app.layout.show(PanelItem::Interactions, Dock::Bottom);
            }
            app.tabs[i].rk.input.request_focus();
        }
        Err(e) => app.toast(format!("Could not run: {e}"), ToastKind::Error),
    }
}

fn poll_session(app: &mut App, i: usize, ctx: &egui::Context) {
    let Some(session) = app.tabs[i].rk.session.as_ref() else { return };
    let events = session.drain();
    if events.is_empty() {
        return;
    }
    for ev in events {
        handle_run_event(app, i, ev, ctx);
    }
}

fn decode_image(ctx: &egui::Context, img: &proto::ImageData, key: usize) -> Option<ValuePart> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&img.png).ok()?;
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    let tex = ctx.load_texture(format!("racket-image-{key}"), color, egui::TextureOptions::LINEAR);
    // PNGs come at 1x; show them at their logical size.
    Some(ValuePart::Image(tex, [img.width as f32, img.height as f32]))
}

fn handle_run_event(app: &mut App, i: usize, ev: RunEvent, ctx: &egui::Context) {
    if std::env::var_os("DRROCKET_TRACE").is_some() {
        eprintln!("run event: {ev:?}");
    }
    let now = ctx.input(|inp| inp.time);
    let tab = &mut app.tabs[i];
    match ev {
        RunEvent::Started { racket_version, .. } => {
            if app.racket.version.is_none() {
                app.racket.version = Some(racket_version);
            }
        }
        RunEvent::Output { stream, text, image } => {
            let err = stream == proto::Stream::Err;
            if let Some(img) = image {
                let k = tab.rk.transcript.len() + (now * 1000.0) as usize;
                if let Some(part) = decode_image(ctx, &img, k) {
                    tab.rk.push(Entry::Value(vec![part]));
                }
            }
            if !text.is_empty() {
                tab.rk.push(Entry::Output { text, err });
            }
        }
        RunEvent::Value { text, segments } => {
            let mut parts = Vec::new();
            for (k, s) in segments.iter().enumerate() {
                match s {
                    Segment::Text { text } => parts.push(ValuePart::Text(text.clone())),
                    Segment::Image(img) => {
                        if let Some(p) = decode_image(ctx, img, tab.rk.transcript.len() * 100 + k + (now * 1000.0) as usize) {
                            parts.push(p);
                        }
                    }
                }
            }
            if parts.is_empty() {
                parts.push(ValuePart::Text(text));
            }
            tab.rk.push(Entry::Value(parts));
        }
        RunEvent::Error(report) => {
            // Highlight the error in the definitions (DrRacket's pink highlight). Errors from
            // interactions only count when they point into a saved definitions file.
            let from_defs = tab.rk.defs_running;
            for loc in report.srclocs.iter().chain(report.stack.iter().map(|f| &f.srcloc)).filter(|l| is_local(tab, l) && (from_defs || l.path.is_some())) {
                if let Some((s, e)) = loc.range() {
                    let range = s..e.max(s + 1);
                    tab.editor.deco.marks.push(Mark { range: range.clone(), kind: MarkKind::Error });
                    tab.editor.deco.diagnostics.push(Diagnostic { range: range.clone(), severity: Severity::Error, message: report.message.clone() });
                    tab.editor.reveal(range.start..range.start);
                    break;
                }
            }
            tab.rk.push(Entry::Error(report, from_defs));
        }
        RunEvent::Tests(report) => {
            apply_test_inlays(tab, &report);
            tab.rk.push(Entry::Tests(report.clone()));
            tab.rk.tests = Some(report);
        }
        RunEvent::Coverage { uncovered, .. } => {
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Uncovered);
            // Coverage ranges are [position, span] pairs.
            for (pos, span) in uncovered {
                tab.editor.deco.marks.push(Mark { range: pos..pos + span, kind: MarkKind::Uncovered });
            }
            let n = tab.editor.deco.marks.iter().filter(|m| m.kind == MarkKind::Uncovered).count();
            let msg = match n {
                0 => "All expressions were covered by tests.".to_string(),
                1 => "1 expression was not covered (highlighted).".to_string(),
                n => format!("{n} expressions were not covered (highlighted)."),
            };
            tab.rk.push(Entry::Info(msg));
        }
        RunEvent::Profile(report) => {
            tab.rk.profile = Some(report);
            app.layout.show(PanelItem::Profiler, Dock::Right);
        }
        RunEvent::Paused(state) => {
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Debug);
            if is_local(tab, &state.srcloc)
                && let Some((s, e)) = state.srcloc.range()
            {
                tab.editor.deco.marks.push(Mark { range: s..e.max(s + 1), kind: MarkKind::Debug });
                tab.editor.reveal(s..s);
            }
            tab.rk.paused = Some(state);
            app.layout.show(PanelItem::Debugger, Dock::Right);
        }
        RunEvent::Resumed => {
            tab.rk.paused = None;
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Debug);
        }
        RunEvent::InputWanted => {
            tab.rk.waiting_input = true;
            tab.rk.input.request_focus();
        }
        RunEvent::Completions { .. } => {}
        RunEvent::Ready { .. } => {
            tab.rk.busy = false;
            tab.rk.defs_running = false;
            tab.rk.waiting_input = false;
            tab.rk.paused = None;
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Debug);
            let queued: Vec<String> = std::mem::take(&mut tab.rk.queued);
            for q in queued {
                eval(tab, &q);
            }
        }
        RunEvent::Exit { code } => {
            tab.rk.push(Entry::Info(format!("Program exited with code {code}.")));
            tab.rk.busy = false;
        }
        RunEvent::Terminated { stderr, .. } => {
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Debug);
            tab.rk.session = None;
            tab.rk.busy = false;
            tab.rk.waiting_input = false;
            tab.rk.paused = None;
            let tail: String = stderr.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            let msg = if tail.trim().is_empty() { "The program has ended.".to_string() } else { format!("The program has ended.\n{tail}") };
            tab.rk.push(Entry::Info(msg));
        }
    }
}

/// Put pass/fail chips at the end of each check-expect line.
fn apply_test_inlays(tab: &mut Tab, report: &TestReport) {
    tab.editor.deco.inlays.clear();
    let mut failed_lines = HashSet::new();
    for f in &report.failures {
        if is_local(tab, &f.srcloc)
            && let Some(pos) = f.srcloc.pos
        {
            let line = tab.editor.doc.buffer.line_of(pos);
            let first = f.message.lines().next().unwrap_or("").to_string();
            let short = if first.chars().count() > 70 { format!("{}…", first.chars().take(69).collect::<String>()) } else { first };
            tab.editor.deco.inlays.push(Inlay { line, text: short, kind: InlayKind::Fail });
            failed_lines.insert(line);
        }
    }
    // Every other test form passed.
    for item in tab.editor.doc.outline().iter() {
        if item.kind == crate::editor::document::OutlineKind::Test {
            let line = tab.editor.doc.buffer.line_of(item.pos);
            if !failed_lines.contains(&line) {
                tab.editor.deco.inlays.push(Inlay { line, text: "passed".into(), kind: InlayKind::Pass });
            }
        }
    }
}

fn eval(tab: &mut Tab, text: &str) {
    let Some(s) = tab.rk.session.as_mut() else { return };
    s.eval(text);
    tab.rk.busy = true;
}

/// Submit the REPL input.
fn submit(app: &mut App, i: usize) {
    let tab = &mut app.tabs[i];
    let text = tab.rk.input.doc.text();
    if tab.rk.waiting_input {
        if let Some(s) = tab.rk.session.as_mut() {
            s.send_input(&format!("{text}\n"));
            tab.rk.push(Entry::Output { text: format!("{text}\n"), err: false });
        }
        tab.rk.input.set_text("");
        tab.rk.waiting_input = false;
        return;
    }
    if text.trim().is_empty() {
        return;
    }
    tab.rk.push(Entry::Input(text.trim_end().to_string()));
    if tab.rk.history.last() != Some(&text) {
        tab.rk.history.push(text.clone());
    }
    tab.rk.hist_pos = None;
    tab.rk.input.set_text("");
    let alive = tab.rk.session.as_mut().is_some_and(|s| s.is_alive());
    if alive && !tab.rk.busy {
        eval(tab, &text);
    } else {
        tab.rk.queued.push(text);
        if !alive {
            start_run(app, i, Instrument::Errortrace);
        }
    }
}

// ---------------------------------------------------------------------------------- stepper

fn start_stepper(app: &mut App, i: usize) {
    ensure_backend(app);
    let Some(backend) = app.racket.backend.as_ref() else { return };
    let tab = &app.tabs[i];
    let body = tab.editor.doc.text();
    let Some(level) = tab.language.stepper_level(&body) else {
        app.toast("The stepper works with Beginning Student through Intermediate Student with lambda (Choose Language, ⌘L).", ToastKind::Info);
        return;
    };
    let width = 60;
    let req = StepRequest {
        path: tab.path.clone(),
        source: body,
        level: level.to_string(),
        settings: tab.lang_settings.stepper_json(),
        teachpacks: tab.lang_settings.teachpacks.clone(),
        width,
    };
    match StepperSession::start(backend, &req) {
        Ok(s) => {
            let tab = &mut app.tabs[i];
            tab.rk.stepper = Some(StepperView { session: Some(s), steps: Vec::new(), current: 0, finished: false, paused: false, error: None });
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::StepBefore);
            app.layout.show(PanelItem::Stepper, Dock::Right);
        }
        Err(e) => app.toast(format!("Could not start the stepper: {e}"), ToastKind::Error),
    }
}

fn poll_stepper(app: &mut App, i: usize) {
    let tab = &mut app.tabs[i];
    let Some(st) = tab.rk.stepper.as_mut() else { return };
    let Some(sess) = st.session.as_ref() else { return };
    let first = st.steps.is_empty();
    for ev in sess.drain() {
        match ev {
            StepperEvent::Step(s) => st.steps.push(StepItem::Step(s)),
            StepperEvent::StepError { before, message, before_src, .. } => st.steps.push(StepItem::Error { before, message, before_src }),
            StepperEvent::Error { message } => st.error = Some(message),
            StepperEvent::Paused { .. } => st.paused = true,
            StepperEvent::Finished { .. } => st.finished = true,
            StepperEvent::Terminated { stderr, .. } => {
                if !st.finished && st.error.is_none() && st.steps.is_empty() {
                    st.error = Some(if stderr.trim().is_empty() { "The stepper stopped.".into() } else { stderr });
                }
                st.finished = true;
            }
        }
    }
    if first && !st.steps.is_empty() {
        show_step_marks(tab);
    }
}

fn show_step_marks(tab: &mut Tab) {
    let Some(st) = &tab.rk.stepper else { return };
    tab.editor.deco.marks.retain(|m| m.kind != MarkKind::StepBefore);
    let (b, a) = match st.steps.get(st.current) {
        Some(StepItem::Step(s)) => (s.before_src, s.after_src),
        Some(StepItem::Error { before_src, .. }) => (*before_src, None),
        None => (None, None),
    };
    let n = tab.editor.doc.buffer.len();
    if let Some(b) = b {
        let r = b.pos.min(n)..(b.pos + b.span).min(n);
        tab.editor.deco.marks.push(Mark { range: r.clone(), kind: MarkKind::StepBefore });
        tab.editor.reveal_soft(r.start);
    }
    let _ = a;
}

// =====================================================================================
// Commands

pub fn run(app: &mut App, cmd: Command) {
    if app.tabs.is_empty() && !matches!(cmd, Command::SearchDocs | Command::InstallPackage) {
        app.toast("Open a Racket file first", ToastKind::Info);
        return;
    }
    let i = app.active;
    match cmd {
        Command::Run => start_run(app, i, Instrument::Errortrace),
        Command::Debug => start_run(app, i, Instrument::Debug),
        Command::Profile => start_run(app, i, Instrument::Profile),
        Command::RunWithCoverage => start_run(app, i, Instrument::Coverage),
        Command::RunTests => start_run(app, i, Instrument::Errortrace),
        Command::Break => {
            if let Some(s) = app.tabs[i].rk.session.as_mut() {
                s.interrupt();
            }
        }
        Command::Kill => {
            let tab = &mut app.tabs[i];
            tab.editor.deco.marks.retain(|m| m.kind != MarkKind::Debug);
            if tab.rk.session.take().is_some() {
                tab.rk.busy = false;
                tab.rk.waiting_input = false;
                tab.rk.paused = None;
                tab.rk.push(Entry::Info("Program killed.".into()));
            }
            if let Some(st) = tab.rk.stepper.as_mut() {
                st.session = None;
            }
        }
        Command::Stepper => start_stepper(app, i),
        Command::CheckSyntax => start_check(app, i, true),
        Command::MacroStepper => {
            let tab = &app.tabs[i];
            let (source, offset) = lang::program_text(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
            let req = ServerRequest::MacroSteps { path: path_string(tab), source, offset, width: 70, policy: "standard".into() };
            let id = tab.id;
            app.tabs[i].rk.macros = Some(MacroView { steps: Vec::new(), current: 0, loading: true });
            request(app, req, Pending::MacroSteps { tab: id });
            app.layout.show(PanelItem::Macros, Dock::Right);
        }
        Command::ChooseLanguage => {
            let tab = &app.tabs[i];
            app.modal = Modal::Language(crate::ui::language_dialog::LanguageDialog::new(tab.language, tab.lang_settings.clone()));
        }
        Command::SearchDocs => {
            app.layout.show(PanelItem::Docs, Dock::Right);
            app.racket.docs.focus = true;
            if let Some(t) = app.tabs.get_mut(i) {
                let head = t.editor.doc.buffer.selections.primary().head;
                if let Some((_, name)) = t.editor.doc.symbol_at(head) {
                    app.racket.docs.query = name;
                }
            }
        }
        Command::ClearInteractions => {
            let tab = &mut app.tabs[i];
            tab.rk.transcript.clear();
        }
        Command::CreateExecutable => {
            let Some(path) = app.tabs[i].path.clone() else {
                app.toast("Save the file first", ToastKind::Info);
                return;
            };
            let raco = app.racket.install.as_ref().and_then(|r| r.raco()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "raco".into());
            let out = path.with_extension("");
            run_in_terminal(app, &raco, &["exe".into(), "-o".into(), out.to_string_lossy().into_owned(), path.to_string_lossy().into_owned()]);
        }
        Command::InstallPackage => app.modal = Modal::Prompt(crate::ui::language_dialog::Prompt::install_package()),
        Command::ModuleBrowser => {
            let include_libs = app.tabs[i].rk.modules.as_ref().is_some_and(|m| m.include_libs);
            request_modules(app, i, include_libs);
            app.layout.show(PanelItem::Modules, Dock::Right);
        }
        _ => {}
    }
}

fn request_modules(app: &mut App, i: usize, include_libs: bool) {
    let tab = &app.tabs[i];
    let Some(path) = path_string(tab) else {
        app.toast("Save the file to browse its modules", ToastKind::Info);
        return;
    };
    let source = lang::save(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
    let id = tab.id;
    app.tabs[i].rk.modules = Some(ModulesView { graph: None, include_libs, loading: true, error: None });
    request(app, ServerRequest::ModuleGraph { path, source, include_libs }, Pending::Modules { tab: id });
}

pub fn run_in_terminal(app: &mut App, program: &str, args: &[String]) {
    let cwd = app.active_tab().and_then(|t| t.path.as_ref()).and_then(|p| p.parent().map(Path::to_path_buf)).or_else(|| app.workspace.as_ref().map(|w| w.root.clone())).unwrap_or_else(|| PathBuf::from("."));
    match crate::terminal::TerminalView::spawn_command(&cwd, program, args, &app.ctx) {
        Ok(view) => {
            let id = app.alloc_id();
            app.terminals.push(crate::app::TerminalTab { id, view, focus: true });
            app.layout.show(PanelItem::Terminal(id), Dock::Bottom);
        }
        Err(e) => app.toast(format!("Could not run {program}: {e}"), ToastKind::Error),
    }
}

pub fn install_package(app: &mut App, name: &str) {
    let raco = app.racket.install.as_ref().and_then(|r| r.raco()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "raco".into());
    run_in_terminal(app, &raco, &["pkg".into(), "install".into(), "--auto".into(), name.to_string()]);
}

/// Apply a new language (from the Choose Language dialog) to the active tab.
pub fn set_language(app: &mut App, language: Language, settings: TeachingSettings) {
    let Some(tab) = app.tabs.get_mut(app.active) else { return };
    let text = tab.editor.doc.text();
    let has_lang = lang::hash_lang(&text).is_some();
    if language == Language::Module && !has_lang {
        let b = &mut tab.editor.doc.buffer;
        b.apply(vec![crate::editor::buffer::Replace { range: 0..0, text: "#lang racket\n".into() }], None, crate::editor::buffer::EditKind::Other);
    } else if language != Language::Module && has_lang {
        // Teaching languages don't use a #lang line; drop it (DrRacket would reject it).
        let b = &mut tab.editor.doc.buffer;
        let end = if b.line_count() > 1 { b.line_start(1) } else { b.len() };
        b.apply(vec![crate::editor::buffer::Replace { range: 0..end, text: String::new() }], None, crate::editor::buffer::EditKind::Other);
    }
    tab.language = language;
    tab.lang_settings = settings;
    tab.lang_changed = true;
    tab.rk.session = None;
    tab.rk.check_due = Some(0.0);
    let words = app.racket.completions.get(&language.completions_language(&tab.editor.doc.head(400))).cloned();
    if let Some(w) = words {
        tab.editor.completion_words = w.clone();
        tab.rk.input.completion_words = w;
    }
    if let Some(p) = tab.path.clone() {
        remember_language(app, &p, language);
    }
}

// =====================================================================================
// Editor actions (called after the editor is shown)

pub fn after_editor(app: &mut App) {
    let i = app.active;
    let Some(tab) = app.tabs.get_mut(i) else { return };
    let actions = std::mem::take(&mut tab.editor.actions);
    for a in actions {
        editor_action(app, i, a);
    }
}

fn editor_action(app: &mut App, i: usize, a: EditorAction) {
    match a {
        EditorAction::Hover(pos) => {
            let tab = &app.tabs[i];
            if tab.editor.deco.hover_doc.as_ref().is_some_and(|(r, _)| r.start == pos) {
                return;
            }
            let (source, offset) = lang::program_text(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
            let req = ServerRequest::Bluebox { path: path_string(tab), source, offset, pos: Some(pos), name: None };
            let p = Pending::Hover { tab: tab.id, pos, version: tab.editor.doc.version() };
            request(app, req, p);
        }
        EditorAction::CallContext(name) => {
            let tab = &app.tabs[i];
            let key = format!("{}\u{0}{name}", tab.language.completions_language(&tab.editor.doc.head(400)));
            match app.racket.call_boxes.get(&key) {
                Some(Some(lines)) => app.tabs[i].rk.call_box = Some((name, lines.clone())),
                Some(None) => app.tabs[i].rk.call_box = None,
                None => {
                    let (source, offset) = lang::program_text(tab.language, &tab.lang_settings, tab.path.as_deref(), &tab.editor.doc.text());
                    let req = ServerRequest::Bluebox { path: path_string(tab), source, offset, pos: None, name: Some(name) };
                    request(app, req, Pending::CallBox { key });
                }
            }
        }
        EditorAction::GotoDefinition(pos) => goto_definition(app, i, pos),
        EditorAction::OpenDocs(pos) => {
            let tab = &app.tabs[i];
            let url = tab.rk.check.as_ref().and_then(|c| c.docs.iter().find(|d| d.start <= pos && pos <= d.end)).and_then(|d| d.url());
            match url {
                Some(u) => {
                    let _ = open::that(u);
                }
                None => run(app, Command::SearchDocs),
            }
        }
        EditorAction::Rename(pos) => {
            let tab = &mut app.tabs[i];
            let Some(check) = tab.rk.check.as_ref() else {
                app.toast("Rename needs Check Syntax information; it runs automatically after edits (or ⇧⌘C).", ToastKind::Info);
                return;
            };
            let occ = check.occurrences(pos);
            if occ.is_empty() {
                let imported = check.arrows.iter().any(|a| a.kind == proto::ArrowKind::Imported && a.to.0 <= pos && pos <= a.to.1);
                let msg = if imported { "This identifier is imported from another module, so it can only be renamed where it is defined." } else { "Place the caret on an identifier defined in this file to rename it." };
                app.toast(msg, ToastKind::Info);
                return;
            }
            let name = tab.editor.doc.buffer.slice(occ[0].0..occ[0].1);
            app.modal = Modal::Prompt(crate::ui::language_dialog::Prompt::rename(tab.id, occ, name));
        }
        EditorAction::ToggleBreakpoint(line) => {
            let tab = &mut app.tabs[i];
            if !tab.editor.deco.breakpoints.remove(&line) {
                tab.editor.deco.breakpoints.insert(line);
            }
            let positions: Vec<usize> = tab.editor.deco.breakpoints.iter().map(|&l| tab.editor.doc.buffer.line_start(l) + tab.editor.doc.buffer.indentation(l)).collect();
            if let Some(s) = tab.rk.session.as_mut() {
                s.set_breakpoints(&positions);
            }
        }
        EditorAction::RunSelection => {
            let tab = &mut app.tabs[i];
            let sel = tab.editor.doc.buffer.selections.primary();
            let text = if sel.is_empty() {
                let l = tab.editor.doc.buffer.line_of(sel.head);
                tab.editor.doc.buffer.line_text(l)
            } else {
                tab.editor.doc.buffer.slice(sel.range())
            };
            tab.rk.input.set_text(&text);
            submit(app, i);
        }
        EditorAction::Submit | EditorAction::HistoryPrev | EditorAction::HistoryNext => {}
    }
}

fn goto_definition(app: &mut App, i: usize, pos: usize) {
    let tab = &mut app.tabs[i];
    let Some(check) = tab.rk.check.clone() else {
        app.toast("Go to Definition needs Check Syntax information (⇧⌘C).", ToastKind::Info);
        return;
    };
    let inside = |r: &proto::Range| r.0 <= pos && pos <= r.1;
    if let Some(a) = check.arrows.iter().find(|a| a.kind == proto::ArrowKind::Lexical && inside(&a.to)) {
        tab.editor.reveal(a.from.0..a.from.1);
        return;
    }
    if let Some(j) = check.jumps.iter().find(|j| j.start <= pos && pos <= j.end).cloned()
        && let Some(p) = j.path.clone() {
            let path = PathBuf::from(&p);
            // Ask the server where the name is defined in that file, then open it.
            let req = ServerRequest::JumpTarget { path: p.clone(), name: j.id.clone() };
            ensure_backend(app);
            let target = app.racket.server.as_mut().and_then(|s| {
                let id = s.request(req);
                s.wait(id, std::time::Duration::from_secs(5))
            });
            app.open_file(&path);
            if let Some(Ok(ServerResponse::JumpTarget(Some(t)))) = target
                && let Some(tab) = app.tabs.get_mut(app.active)
            {
                tab.editor.reveal(t.pos..t.pos);
            }
            return;
        }
    if let Some(d) = check.docs.iter().find(|d| d.start <= pos && pos <= d.end)
        && let Some(u) = d.url()
    {
        let _ = open::that(u);
    }
}

// =====================================================================================
// Status, conditions

pub fn is_running(app: &App) -> bool {
    app.active_tab().is_some_and(|t| t.rk.session.is_some() && t.rk.busy)
}

pub fn status_text(app: &App) -> (String, Color32) {
    let theme = &app.theme;
    if app.racket.missing {
        return ("Racket not found".into(), theme.error);
    }
    if let Some(b) = &app.racket.backend
        && b.status() == BackendStatus::Compiling
    {
        return ("Preparing Racket tools…".into(), theme.text_muted);
    }
    if let Some(t) = app.active_tab() {
        if t.rk.paused.is_some() {
            return ("Paused".into(), theme.warning);
        }
        if t.rk.waiting_input {
            return ("Waiting for input".into(), theme.info);
        }
        if t.rk.busy {
            return ("Running…".into(), theme.success);
        }
    }
    match &app.racket.version {
        Some(v) => (format!("Racket {v}"), theme.text_muted),
        None => (String::new(), theme.text_muted),
    }
}

/// Automation conditions.
pub fn condition(app: &App, cond: &str) -> bool {
    match cond {
        "racket-ready" => app.racket.backend.as_ref().is_some_and(|b| b.status() != BackendStatus::Compiling) && app.racket.server.is_some(),
        "run-finished" => app.active_tab().is_some_and(|t| !t.rk.busy),
        "stepper-ready" => app.active_tab().and_then(|t| t.rk.stepper.as_ref()).is_some_and(|s| !s.steps.is_empty() || s.finished),
        "check-done" => app.active_tab().is_some_and(|t| t.rk.check.is_some() && t.rk.check_req.is_none() && t.rk.check_due.is_none()),
        "macros-ready" => app.active_tab().and_then(|t| t.rk.macros.as_ref()).is_some_and(|m| !m.loading),
        "docs-ready" => !app.racket.docs.searching,
        _ => true,
    }
}

// =====================================================================================
// Panels

pub fn panel(app: &mut App, ui: &mut Ui, rect: Rect, item: PanelItem) {
    match item {
        PanelItem::Interactions => interactions_panel(app, ui, rect),
        PanelItem::Stepper => stepper_panel(app, ui, rect),
        PanelItem::Tests => tests_panel(app, ui),
        PanelItem::Macros => macros_panel(app, ui, rect),
        PanelItem::Docs => docs_panel(app, ui),
        PanelItem::Debugger => debugger_panel(app, ui),
        PanelItem::Profiler => profiler_panel(app, ui),
        PanelItem::Modules => modules_panel(app, ui, rect),
        _ => {}
    }
}

fn no_tab(app: &App, ui: &mut Ui, glyph: &str, title: &str, sub: &str) -> bool {
    if app.tabs.is_empty() {
        empty_state(ui, &app.theme, glyph, title, sub);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------------- interactions

fn interactions_panel(app: &mut App, ui: &mut Ui, rect: Rect) {
    if no_tab(app, ui, ic::TERMINAL, "Interactions", "Open a file and press ⌘R") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let settings = app.settings.clone();
    let i = app.active;
    let font_size = (app.font_size() * 0.85).round().max(11.0);
    let mono = ui::mono(font_size);
    let line_h = (font_size * settings.line_height).round();

    // Input box at the bottom, sized to its content.
    let lines = app.tabs[i].rk.input.doc.buffer.line_count().clamp(1, 10) as f32;
    let input_h = lines * line_h + 16.0;
    let box_h = input_h + 40.0;
    let pad = 10.0;
    let box_rect = Rect::from_min_max(pos2(rect.left() + pad, rect.bottom() - box_h - pad), pos2(rect.right() - pad, rect.bottom() - pad));
    let transcript_rect = Rect::from_min_max(rect.min + vec2(0.0, 2.0), pos2(rect.right(), box_rect.top() - 6.0));

    // Transcript.
    let mut jump: Option<Srcloc> = None;
    {
        let tab = &mut app.tabs[i];
        let mut child = ui.new_child(UiBuilder::new().max_rect(transcript_rect.shrink2(vec2(14.0, 4.0))));
        child.set_clip_rect(transcript_rect.intersect(ui.clip_rect()));
        let stick = tab.rk.scroll_to_bottom;
        tab.rk.scroll_to_bottom = false;
        let sa = egui::ScrollArea::vertical().id_salt(("transcript", tab.id)).auto_shrink([false, false]).stick_to_bottom(true);
        sa.show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add_space(6.0);
            if tab.rk.transcript.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Press ⌘R to run the definitions, then evaluate expressions below.").color(theme.text_placeholder));
            }
            for e in &tab.rk.transcript {
                transcript_entry(ui, e, &theme, &tk, &mono, &mut jump);
            }
            ui.add_space(6.0);
            if stick {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
        });
    }
    if let Some(loc) = jump {
        jump_to_srcloc(app, &loc);
    }

    // Input box.
    let tab = &mut app.tabs[i];
    let waiting = tab.rk.waiting_input;
    let focused = tab.rk.input.has_focus(ui.ctx());
    let border = if waiting { theme.info } else if focused { with_alpha(theme.border_focused, 170) } else { tk.island_border };
    ui.painter().rect(box_rect, CornerRadius::same(12), mix(tk.island, theme.text, if theme.dark { 0.035 } else { 0.02 }), Stroke::new(1.0, border), StrokeKind::Inside);
    let ed_rect = Rect::from_min_max(box_rect.min + vec2(14.0, 10.0), pos2(box_rect.right() - 14.0, box_rect.top() + 10.0 + input_h - 16.0));
    let style = EditorStyle { theme: &theme, settings: &settings, font_size, mode: EditorMode::Repl };
    {
        let mut c = ui.new_child(UiBuilder::new().max_rect(ed_rect));
        c.set_clip_rect(ed_rect.expand(2.0));
        tab.rk.input.show(&mut c, ed_rect, &style);
    }
    if tab.rk.input.doc.buffer.is_empty() {
        let hint = if waiting { "The program is waiting for input…" } else if tab.rk.busy { "Evaluating…  (⌘. to break)" } else { "Evaluate a Racket expression…" };
        ui.painter().text(pos2(ed_rect.left() + 2.0, ed_rect.top() + line_h / 2.0), Align2::LEFT_CENTER, hint, ui::ui_font(font_size), theme.text_placeholder);
    }
    // Bottom row: language chip, history, send.
    let row = Rect::from_min_max(pos2(box_rect.left() + 10.0, box_rect.bottom() - 36.0), pos2(box_rect.right() - 8.0, box_rect.bottom() - 6.0));
    let chip = language_label(tab);
    let chip_g = ui.painter().layout_no_wrap(chip, ui::medium(12.5), theme.text_muted);
    let chip_r = Rect::from_min_size(pos2(row.left(), row.center().y - 12.0), vec2(chip_g.size().x + 30.0, 24.0));
    ui.painter().rect_filled(chip_r, CornerRadius::same(12), tk.hover);
    ui.painter().text(pos2(chip_r.left() + 13.0, chip_r.center().y), Align2::CENTER_CENTER, ic::SCHOOL, ui::icon_font_filled(14.0), theme.syntax.keyword.color);
    ui.painter().galley(pos2(chip_r.left() + 23.0, chip_r.center().y - chip_g.size().y / 2.0), chip_g, theme.text_muted);
    let chip_resp = ui.interact(chip_r, Id::unique("repl-lang-chip"), Sense::click()).on_hover_text("Choose Language (⌘L)");
    let send_r = Rect::from_center_size(pos2(row.right() - 14.0, row.center().y), vec2(28.0, 28.0));
    let send = ui.interact(send_r, Id::unique("repl-send"), Sense::click());
    let can_send = !tab.rk.input.doc.buffer.is_empty() || waiting;
    let send_bg = if can_send { if send.hovered() { mix(theme.text_accent, Color32::WHITE, 0.12) } else { theme.text_accent } } else { tk.pressed };
    ui.painter().circle_filled(send_r.center(), 14.0, send_bg);
    ui.painter().text(send_r.center(), Align2::CENTER_CENTER, ic::ARROW_UPWARD, ui::icon_font(18.0), if can_send { Color32::WHITE } else { theme.text_placeholder });
    let busy = tab.rk.busy;
    if busy && !waiting {
        // Running indicator + break button.
        let br = Rect::from_center_size(pos2(send_r.left() - 22.0, row.center().y), vec2(28.0, 28.0));
        let b = ui.interact(br, Id::unique("repl-break"), Sense::click()).on_hover_text("Break (⌘.)");
        if b.hovered() {
            ui.painter().rect_filled(br, CornerRadius::same(6), tk.hover);
        }
        ui.painter().text(br.center(), Align2::CENTER_CENTER, ic::STOP_CIRCLE, ui::icon_font(19.0), theme.error);
        if b.clicked()
            && let Some(s) = tab.rk.session.as_mut() {
                s.interrupt();
            }
        let t = ui.ctx().input(|inp| inp.time) as f32;
        let c = pos2(chip_r.right() + 18.0, row.center().y);
        for k in 0..3 {
            let a = ((t * 4.0 - k as f32 * 0.6).sin() * 0.5 + 0.5) * 200.0 + 40.0;
            ui.painter().circle_filled(c + vec2(k as f32 * 8.0, 0.0), 2.5, with_alpha(theme.text_accent, a as u8));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }

    // Input actions.
    let mut do_submit = send.clicked() && can_send;
    let actions = std::mem::take(&mut tab.rk.input.actions);
    for a in actions {
        match a {
            EditorAction::Submit => do_submit = true,
            EditorAction::HistoryPrev | EditorAction::HistoryNext => {
                let n = tab.rk.history.len();
                if n == 0 {
                    continue;
                }
                let prev = matches!(a, EditorAction::HistoryPrev);
                let pos = match (tab.rk.hist_pos, prev) {
                    (None, true) => {
                        tab.rk.draft = tab.rk.input.doc.text();
                        Some(n - 1)
                    }
                    (None, false) => None,
                    (Some(p), true) => Some(p.saturating_sub(1)),
                    (Some(p), false) => (p + 1 < n).then_some(p + 1),
                };
                tab.rk.hist_pos = pos;
                let text = pos.map(|p| tab.rk.history[p].clone()).unwrap_or_else(|| tab.rk.draft.clone());
                tab.rk.input.set_text(&text);
            }
            _ => {}
        }
    }
    if chip_resp.clicked() {
        run(app, Command::ChooseLanguage);
    }
    if do_submit {
        submit(app, i);
    }
}

fn transcript_entry(ui: &mut Ui, e: &Entry, theme: &crate::theme::Theme, tk: &Tokens, mono: &egui::FontId, jump: &mut Option<Srcloc>) {
    let w = ui.available_width();
    match e {
        Entry::Banner(text) => {
            let mut lines = text.lines();
            if let Some(first) = lines.next() {
                ui.label(egui::RichText::new(first).font(ui::medium(13.0)).color(theme.text_muted));
            }
            for l in lines {
                ui.label(egui::RichText::new(l).font(ui::ui_font(12.5)).color(theme.text_placeholder));
            }
        }
        Entry::Input(code) => {
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new("›").font(mono.clone()).color(theme.text_accent));
                let job = racket_job(code, theme, mono, &[], Color32::TRANSPARENT);
                ui.add(egui::Label::new(job).wrap());
            });
        }
        Entry::Output { text, err } => {
            let color = if *err { theme.error } else { theme.editor_fg };
            let shown = if text.lines().count() > 3000 {
                let skip = text.lines().count() - 3000;
                format!("… {skip} earlier lines hidden …\n{}", text.lines().skip(skip).collect::<Vec<_>>().join("\n"))
            } else {
                text.trim_end_matches('\n').to_string()
            };
            let mut job = egui::text::LayoutJob::single_section(shown, egui::TextFormat { font_id: mono.clone(), color, italics: *err, ..Default::default() });
            job.wrap.max_width = w;
            ui.label(job);
        }
        Entry::Value(parts) => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for p in parts {
                    match p {
                        ValuePart::Text(t) => {
                            let mut job = racket_job(t, theme, mono, &[], Color32::TRANSPARENT);
                            job.wrap.max_width = w;
                            ui.label(job);
                        }
                        ValuePart::Image(tex, size) => {
                            let scale = (w / size[0]).min(1.0);
                            ui.add(egui::Image::new((tex.id(), vec2(size[0], size[1]) * scale)));
                        }
                    }
                }
            });
        }
        Entry::Error(report, from_defs) => {
            egui::Frame::NONE.fill(theme.error_bg).stroke(Stroke::new(1.0, with_alpha(theme.error, 70))).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
                ui.set_width(w - 24.0);
                ui.horizontal_top(|ui| {
                    ui.label(egui::RichText::new(ic::ERROR).font(ui::icon_font_filled(17.0)).color(theme.error));
                    let job = egui::text::LayoutJob::single_section(report.message.clone(), egui::TextFormat { font_id: mono.clone(), color: mix(theme.error, theme.text, 0.35), ..Default::default() });
                    ui.add(egui::Label::new(job).wrap());
                });
                let locs: Vec<&Srcloc> = report.srclocs.iter().chain(report.stack.iter().map(|f| &f.srcloc)).filter(|l| l.line.is_some()).take(1).collect();
                for l in locs {
                    let here = if *from_defs { "definitions" } else { "interactions" };
                    let label = format!("{}:{}:{}", l.path.as_deref().map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_else(|| here.into()), l.line.unwrap_or(0), l.col.unwrap_or(0));
                    let clickable = *from_defs || l.path.is_some();
                    let color = if clickable { theme.text_accent } else { theme.text_muted };
                    let mut text = egui::RichText::new(label).font(ui::mono(12.0)).color(color);
                    if clickable {
                        text = text.underline();
                    }
                    let resp = ui.add(egui::Label::new(text).sense(Sense::click()));
                    if clickable && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *jump = Some(l.clone());
                    }
                }
                if report.stack.len() > 1 {
                    egui::CollapsingHeader::new(egui::RichText::new(format!("Stack ({} frames)", report.stack.len())).size(12.0).color(theme.text_muted)).id_salt(("stack", report as *const _ as usize)).show(ui, |ui| {
                        for f in &report.stack {
                            let name = f.name.clone().unwrap_or_else(|| "?".into());
                            let file = f.srcloc.path.as_deref().map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_else(|| "definitions".into());
                            let label = format!("{name}   {file}:{}", f.srcloc.line.unwrap_or(0));
                            if ui.add(egui::Label::new(egui::RichText::new(label).font(ui::mono(12.0)).color(theme.text_muted)).sense(Sense::click())).clicked() {
                                *jump = Some(f.srcloc.clone());
                            }
                        }
                    });
                }
            });
        }
        Entry::Tests(r) => test_card(ui, r, theme, mono, jump, w),
        Entry::Info(text) => {
            ui.label(egui::RichText::new(text).font(ui::ui_font(12.5)).italics().color(theme.text_placeholder));
        }
    }
    let _ = tk;
}

fn test_card(ui: &mut Ui, r: &TestReport, theme: &crate::theme::Theme, mono: &egui::FontId, jump: &mut Option<Srcloc>, w: f32) {
    let ok = r.failures.is_empty();
    let (c, bg, glyph) = if ok { (theme.success, theme.success_bg, ic::CHECK_CIRCLE) } else { (theme.error, theme.error_bg, ic::CANCEL) };
    egui::Frame::NONE.fill(bg).stroke(Stroke::new(1.0, with_alpha(c, 70))).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.set_width(w - 24.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(glyph).font(ui::icon_font_filled(17.0)).color(c));
            let summary = if r.total == 0 { "No tests".to_string() } else if ok { format!("All {} tests passed", r.total) } else { format!("{} of {} tests failed", r.total - r.passed, r.total) };
            ui.label(egui::RichText::new(summary).font(ui::medium(13.5)).color(theme.text));
        });
        for f in &r.failures {
            ui.add_space(4.0);
            let mut job = egui::text::LayoutJob::single_section(f.message.clone(), egui::TextFormat { font_id: mono.clone(), color: theme.text_muted, ..Default::default() });
            job.wrap.max_width = w - 50.0;
            let resp = ui.add(egui::Label::new(job).sense(Sense::click()));
            if let Some(line) = f.srcloc.line {
                let link = ui.add(egui::Label::new(egui::RichText::new(format!("at line {line}")).font(ui::mono(12.0)).color(theme.text_accent).underline()).sense(Sense::click()));
                if link.clicked() || resp.clicked() {
                    *jump = Some(f.srcloc.clone());
                }
            }
        }
    });
}

fn jump_to_srcloc(app: &mut App, loc: &Srcloc) {
    if let Some(p) = &loc.path {
        let path = PathBuf::from(p);
        if app.active_tab().and_then(|t| t.path.as_ref()) != Some(&path) && path.is_file() {
            app.open_file(&path);
        }
    }
    let Some(tab) = app.active_tab_mut() else { return };
    let n = tab.editor.doc.buffer.len();
    let range = match (loc.pos, loc.line) {
        (Some(p), _) => p.min(n)..(p + loc.span.unwrap_or(0)).min(n),
        (None, Some(l)) => {
            let s = tab.editor.doc.buffer.line_start((l as usize).saturating_sub(1)) + loc.col.unwrap_or(0) as usize;
            s.min(n)..s.min(n)
        }
        _ => return,
    };
    tab.editor.reveal(range);
    tab.editor.request_focus();
}

// ---------------------------------------------------------------------------------- stepper panel

fn stepper_panel(app: &mut App, ui: &mut Ui, rect: Rect) {
    if no_tab(app, ui, ic::STEP, "Stepper", "Open a teaching-language file") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let i = app.active;
    let font_size = (app.font_size() * 0.8).round().max(11.0);
    let mono = ui::mono(font_size);
    if app.tabs[i].rk.stepper.is_none() {
        empty_state(ui, &theme, ic::STEP, "Algebraic Stepper", "Step through Beginning–Intermediate Student programs (⇧⌘E)");
        let r = ui.available_rect_before_wrap();
        let br = Rect::from_center_size(r.center() + vec2(0.0, 64.0), vec2(150.0, 32.0));
        let resp = ui.interact(br, Id::unique("start-stepper"), Sense::click());
        ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { mix(theme.text_accent, Color32::WHITE, 0.1) } else { theme.text_accent });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, "Start Stepping", ui::medium(13.5), Color32::WHITE);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            start_stepper(app, i);
        }
        return;
    }
    // Toolbar.
    let mut nav: Option<isize> = None;
    let mut restart = false;
    {
        let st = app.tabs[i].rk.stepper.as_ref().unwrap();
        let n = st.steps.len();
        let bar = Rect::from_min_size(rect.min + vec2(6.0, 4.0), vec2(rect.width() - 12.0, 34.0));
        let mut b = ui.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
        b.spacing_mut().item_spacing.x = 2.0;
        for (glyph, tip, delta) in [(ic::FIRST_PAGE, "First step", isize::MIN), (ic::CHEVRON_LEFT, "Previous step", -1), (ic::CHEVRON_RIGHT, "Next step", 1), (ic::LAST_PAGE, "Last step", isize::MAX)] {
            if icon_button_sized(&mut b, &theme, glyph, tip, 30.0, 20.0, None, false).clicked() {
                nav = Some(delta);
            }
        }
        let label = if n == 0 {
            if st.finished { "No steps".to_string() } else { "Starting…".to_string() }
        } else {
            format!("Step {} of {}{}", st.current + 1, n, if st.finished { "" } else { "+" })
        };
        b.add_space(8.0);
        b.label(egui::RichText::new(label).font(ui::medium(13.0)).color(theme.text_muted));
        let mut rb = b.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if icon_button_sized(&mut rb, &theme, ic::RESTART_ALT, "Restart stepper", 30.0, 19.0, None, false).clicked() {
            restart = true;
        }
        // Keyboard: ← → step.
        if !ui.ctx().memory(|m| m.focused().is_some()) {
            if ui.input(|inp| inp.key_pressed(egui::Key::ArrowRight)) {
                nav = Some(1);
            }
            if ui.input(|inp| inp.key_pressed(egui::Key::ArrowLeft)) {
                nav = Some(-1);
            }
        }
    }
    if restart {
        start_stepper(app, i);
        return;
    }
    if let Some(d) = nav {
        let tab = &mut app.tabs[i];
        let st = tab.rk.stepper.as_mut().unwrap();
        let n = st.steps.len();
        if n > 0 {
            let cur = st.current as isize;
            let next = match d {
                isize::MIN => 0,
                isize::MAX => n as isize - 1,
                d => (cur + d).clamp(0, n as isize - 1),
            };
            st.current = next as usize;
            if st.current + 1 >= n && st.paused {
                st.paused = false;
                if let Some(s) = st.session.as_mut() {
                    s.more();
                }
            }
            show_step_marks(tab);
        }
    }
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + 44.0), rect.max);
    ui.painter().hline(rect.left()..=rect.right(), body.top() - 2.0, Stroke::new(1.0, tk.island_border));
    let st = app.tabs[i].rk.stepper.as_ref().unwrap();
    let mut child = ui.new_child(UiBuilder::new().max_rect(body.shrink2(vec2(12.0, 8.0))));
    child.set_clip_rect(body.intersect(ui.clip_rect()));
    if let Some(e) = &st.error
        && st.steps.is_empty()
    {
        child.label(egui::RichText::new(e).font(mono.clone()).color(theme.error));
        return;
    }
    let Some(step) = st.steps.get(st.current) else {
        if !st.finished {
            child.spinner();
        }
        return;
    };
    let before_bg = with_alpha(theme.syntax.function.color, 55);
    let after_bg = with_alpha(theme.syntax.string.color, 55);
    let side_by_side = body.width() > 760.0;
    egui::ScrollArea::vertical().id_salt("stepper-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        let w = ui.available_width();
        let col_w = if side_by_side { (w - 36.0) / 2.0 } else { w };
        let expr_block = |ui: &mut Ui, exprs: &[proto::StepExpr], bg: Color32, width: f32| {
            ui.vertical(|ui| {
                ui.set_width(width);
                for (k, e) in exprs.iter().enumerate() {
                    let is_current = !e.highlights.is_empty();
                    let marks: Vec<(usize, usize)> = e.highlights.clone();
                    let mut job = racket_job(&e.text, &theme, &mono, &marks, bg);
                    job.wrap.max_width = width - 20.0;
                    let frame_bg = if is_current { mix(tk.island, theme.text, 0.035) } else { Color32::TRANSPARENT };
                    egui::Frame::NONE.fill(frame_bg).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        let resp = ui.label(job);
                        if !is_current {
                            ui.painter().rect_filled(resp.rect.expand(2.0), CornerRadius::same(4), with_alpha(tk.island, 110));
                        }
                    });
                    if k + 1 < exprs.len() {
                        ui.add_space(2.0);
                    }
                }
            });
        };
        let header = |ui: &mut Ui, title: &str, c: Color32| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, c);
                ui.label(egui::RichText::new(title).font(ui::semibold(11.5)).color(theme.text_placeholder));
            });
        };
        match step {
            StepItem::Step(s) => {
                let kind = if s.kind == "user-application" { "function application" } else { "reduction" };
                ui.label(egui::RichText::new(kind).font(ui::ui_font(12.0)).color(theme.text_placeholder));
                ui.add_space(4.0);
                if side_by_side {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            header(ui, "BEFORE", theme.syntax.function.color);
                            expr_block(ui, &s.before, before_bg, col_w);
                        });
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.add_space(26.0);
                            ui.label(egui::RichText::new(ic::ARROW_FORWARD).font(ui::icon_font(20.0)).color(theme.text_placeholder));
                        });
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            header(ui, "AFTER", theme.syntax.string.color);
                            expr_block(ui, &s.after, after_bg, col_w);
                        });
                    });
                } else {
                    header(ui, "BEFORE", theme.syntax.function.color);
                    expr_block(ui, &s.before, before_bg, col_w);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(ic::ARROW_DOWNWARD).font(ui::icon_font(20.0)).color(theme.text_placeholder));
                    });
                    header(ui, "AFTER", theme.syntax.string.color);
                    expr_block(ui, &s.after, after_bg, col_w);
                }
            }
            StepItem::Error { before, message, .. } => {
                header(ui, "BEFORE", theme.syntax.function.color);
                expr_block(ui, before, before_bg, col_w);
                ui.add_space(6.0);
                egui::Frame::NONE.fill(theme.error_bg).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
                    ui.set_width(col_w - 24.0);
                    ui.label(egui::RichText::new(message).font(mono.clone()).color(theme.error));
                });
            }
        }
        if st.finished && st.current + 1 == st.steps.len() {
            ui.add_space(14.0);
            ui.label(egui::RichText::new("All of the definitions have been successfully evaluated.").font(ui::ui_font(12.5)).italics().color(theme.text_placeholder));
        }
    });
}

// ---------------------------------------------------------------------------------- tests

fn tests_panel(app: &mut App, ui: &mut Ui) {
    if no_tab(app, ui, ic::SCIENCE, "Tests", "") {
        return;
    }
    let theme = app.theme.clone();
    let mono = ui::mono((app.font_size() * 0.8).round());
    let i = app.active;
    let Some(r) = app.tabs[i].rk.tests.clone() else {
        empty_state(ui, &theme, ic::SCIENCE, "No test results yet", "Run the program (⌘R) to run check-expect and rackunit tests");
        return;
    };
    let mut jump = None;
    let w = ui.available_width();
    egui::ScrollArea::vertical().id_salt("tests").auto_shrink([false, false]).show(ui, |ui| {
        test_card(ui, &r, &theme, &mono, &mut jump, w);
        ui.add_space(10.0);
        let coverage = app.tabs[i].editor.deco.marks.iter().any(|m| m.kind == MarkKind::Uncovered);
        if ui.button(if coverage { "Re-run with coverage" } else { "Run with test coverage" }).clicked() {
            jump = None;
            ui.ctx().data_mut(|d| d.insert_temp(Id::unique("run-coverage"), true));
        }
    });
    if ui.ctx().data_mut(|d| d.remove_temp::<bool>(Id::unique("run-coverage"))).is_some() {
        start_run(app, i, Instrument::Coverage);
    }
    if let Some(l) = jump {
        jump_to_srcloc(app, &l);
    }
}

// ---------------------------------------------------------------------------------- macro stepper

fn macros_panel(app: &mut App, ui: &mut Ui, rect: Rect) {
    if no_tab(app, ui, ic::AUTO_FIX_HIGH, "Macro Stepper", "") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let i = app.active;
    let mono = ui::mono((app.font_size() * 0.8).round());
    let Some(mv) = app.tabs[i].rk.macros.as_mut() else {
        empty_state(ui, &theme, ic::AUTO_FIX_HIGH, "Macro Stepper", "See every macro expansion step (⇧⌘M)");
        let r = ui.available_rect_before_wrap();
        let br = Rect::from_center_size(r.center() + vec2(0.0, 64.0), vec2(160.0, 32.0));
        let resp = ui.interact(br, Id::unique("start-macros"), Sense::click());
        ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { mix(theme.text_accent, Color32::WHITE, 0.1) } else { theme.text_accent });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, "Expand Program", ui::medium(13.5), Color32::WHITE);
        if resp.clicked() {
            run(app, Command::MacroStepper);
        }
        return;
    };
    if mv.loading {
        ui.spinner();
        return;
    }
    if mv.steps.is_empty() {
        empty_state(ui, &theme, ic::AUTO_FIX_HIGH, "No macro steps", "This program uses no (non-library) macros");
        return;
    }
    let n = mv.steps.len();
    let bar = Rect::from_min_size(rect.min + vec2(6.0, 4.0), vec2(rect.width() - 12.0, 34.0));
    let mut b = ui.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
    if icon_button_sized(&mut b, &theme, ic::CHEVRON_LEFT, "Previous", 30.0, 20.0, None, false).clicked() {
        mv.current = mv.current.saturating_sub(1);
    }
    if icon_button_sized(&mut b, &theme, ic::CHEVRON_RIGHT, "Next", 30.0, 20.0, None, false).clicked() {
        mv.current = (mv.current + 1).min(n - 1);
    }
    let s = &mv.steps[mv.current];
    b.label(egui::RichText::new(format!("Step {} of {n}  ·  {}", mv.current + 1, s.name)).font(ui::medium(13.0)).color(theme.text_muted));
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + 44.0), rect.max);
    ui.painter().hline(rect.left()..=rect.right(), body.top() - 2.0, Stroke::new(1.0, tk.island_border));
    let mut child = ui.new_child(UiBuilder::new().max_rect(body.shrink2(vec2(12.0, 8.0))));
    child.set_clip_rect(body.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical().id_salt("macro-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        let w = ui.available_width();
        ui.label(egui::RichText::new(&s.step_type).font(ui::ui_font(12.0)).color(theme.text_placeholder));
        for (title, text, foci, c) in [("BEFORE", &s.before, &s.foci_before, theme.syntax.function.color), ("AFTER", &s.after, &s.foci_after, theme.syntax.string.color)] {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(title).font(ui::semibold(11.5)).color(theme.text_placeholder));
            let mut job = racket_job(text, &theme, &mono, foci, with_alpha(c, 55));
            job.wrap.max_width = w - 20.0;
            egui::Frame::NONE.fill(mix(tk.island, theme.text, 0.035)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.set_width(w - 16.0);
                ui.label(job);
            });
        }
    });
}

// ---------------------------------------------------------------------------------- docs

fn docs_panel(app: &mut App, ui: &mut Ui) {
    let theme = app.theme.clone();
    let tk = app.tokens();
    let id = Id::unique("docs-search");
    let w = ui.available_width();
    let resp = search_field(ui, &theme, &mut app.racket.docs.query, "Search Racket documentation", id, w);
    if app.racket.docs.focus {
        resp.request_focus();
        app.racket.docs.focus = false;
    }
    let q = app.racket.docs.query.trim().to_string();
    if q != app.racket.docs.searched {
        app.racket.docs.searched = q.clone();
        if !q.is_empty() {
            app.racket.docs.searching = true;
            request(app, ServerRequest::DocsSearch { query: q.clone(), limit: 80 }, Pending::Docs);
        } else {
            app.racket.docs.results.clear();
        }
    }
    ui.add_space(8.0);
    if q.is_empty() {
        empty_state(ui, &theme, ic::MENU_BOOK, "Racket Documentation", "Search all installed manuals. F1 looks up the identifier at the cursor.");
        return;
    }
    let results = app.racket.docs.results.clone();
    egui::ScrollArea::vertical().id_salt("docs").auto_shrink([false, false]).show(ui, |ui| {
        for hit in &results {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(6), tk.hover);
            }
            let glyph = match hit.kind.as_str() {
                "procedure" => ic::FUNCTION,
                "syntax" | "form" => ic::AUTO_FIX_HIGH,
                "value" => ic::DATA_OBJECT,
                "struct" => ic::SCHEMA,
                _ => ic::DESCRIPTION,
            };
            ui.painter().text(pos2(r.left() + 16.0, r.top() + 14.0), Align2::CENTER_CENTER, glyph, ui::icon_font(16.0), theme.syntax.function.color);
            truncated_label(ui, pos2(r.left() + 32.0, r.top() + 14.0), &hit.name, ui::mono(13.5), theme.text, r.width() - 40.0);
            let libs = hit.libs.join(", ");
            truncated_label(ui, pos2(r.left() + 32.0, r.top() + 32.0), &format!("{}  {libs}", hit.kind), ui::ui_font(12.0), theme.text_placeholder, r.width() - 40.0);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                && let Some(u) = hit.url()
            {
                let _ = open::that(u);
            }
        }
    });
}

// ---------------------------------------------------------------------------------- debugger

fn debugger_panel(app: &mut App, ui: &mut Ui) {
    if no_tab(app, ui, ic::BUG_REPORT, "Debugger", "") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let i = app.active;
    let running = app.tabs[i].rk.session.is_some() && app.tabs[i].rk.instrument == Instrument::Debug;
    if !running {
        empty_state(ui, &theme, ic::BUG_REPORT, "Debugger", "Click the gutter to set breakpoints, then Debug (⇧⌘D)");
        let r = ui.available_rect_before_wrap();
        let br = Rect::from_center_size(r.center() + vec2(0.0, 64.0), vec2(150.0, 32.0));
        let resp = ui.interact(br, Id::unique("start-debug"), Sense::click());
        ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { mix(theme.text_accent, Color32::WHITE, 0.1) } else { theme.text_accent });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, "Start Debugging", ui::medium(13.5), Color32::WHITE);
        if resp.clicked() {
            start_run(app, i, Instrument::Debug);
        }
        return;
    }
    let paused = app.tabs[i].rk.paused.clone();
    let mut action = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let p = paused.is_some();
        for (glyph, tip, act, enabled) in [
            (ic::PLAY_ARROW, "Continue", DebugAction::Continue, p),
            (ic::PAUSE, "Pause", DebugAction::Pause, !p),
            (ic::STEP_OVER, "Step Over", DebugAction::StepOver, p),
            (ic::STEP_INTO, "Step Into", DebugAction::StepInto, p),
            (ic::STEP_OUT, "Step Out", DebugAction::StepOut, p),
        ] {
            let color = if enabled { None } else { Some(theme.text_disabled) };
            if icon_button_sized(ui, &theme, glyph, tip, 30.0, 20.0, color, false).clicked() && enabled {
                action = Some(act);
            }
        }
        ui.add_space(8.0);
        let (t, c) = if p { ("Paused", theme.warning) } else { ("Running", theme.success) };
        ui.label(egui::RichText::new(t).font(ui::medium(13.0)).color(c));
    });
    if let Some(a) = action
        && let Some(s) = app.tabs[i].rk.session.as_mut()
    {
        s.debug(a);
    }
    let Some(state) = paused else { return };
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("debugger").auto_shrink([false, false]).show(ui, |ui| {
        if let Some(r) = &state.result {
            ui.label(egui::RichText::new("RESULT").font(ui::semibold(11.5)).color(theme.text_placeholder));
            ui.label(egui::RichText::new(r).font(ui::mono(13.0)).color(theme.syntax.string.color));
            ui.add_space(8.0);
        }
        ui.label(egui::RichText::new("VARIABLES").font(ui::semibold(11.5)).color(theme.text_placeholder));
        if state.locals.is_empty() {
            ui.label(egui::RichText::new("No local bindings").color(theme.text_muted));
        }
        for l in &state.locals {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&l.name).font(ui::mono(13.0)).color(theme.syntax.variable.color));
                ui.label(egui::RichText::new("=").font(ui::mono(13.0)).color(theme.text_placeholder));
                ui.label(egui::RichText::new(&l.value).font(ui::mono(13.0)).color(theme.text));
            });
        }
        ui.add_space(10.0);
        ui.label(egui::RichText::new("CALL STACK").font(ui::semibold(11.5)).color(theme.text_placeholder));
        for f in &state.stack {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), tk.hover);
            }
            truncated_label(ui, pos2(r.left() + 6.0, r.center().y), &f.name, ui::mono(12.5), theme.text, r.width() - 70.0);
            if let Some(line) = f.srcloc.line {
                ui.painter().text(pos2(r.right() - 6.0, r.center().y), Align2::RIGHT_CENTER, format!("line {line}"), ui::ui_font(12.0), theme.text_placeholder);
            }
            if resp.clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(Id::unique("dbg-jump"), f.srcloc.pos.unwrap_or(0)));
            }
        }
    });
    if let Some(pos) = ui.ctx().data_mut(|d| d.remove_temp::<usize>(Id::unique("dbg-jump"))) {
        app.tabs[i].editor.reveal(pos..pos);
    }
}

// ---------------------------------------------------------------------------------- profiler

fn profiler_panel(app: &mut App, ui: &mut Ui) {
    if no_tab(app, ui, ic::SPEED, "Profiler", "") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let i = app.active;
    let Some(p) = app.tabs[i].rk.profile.clone() else {
        empty_state(ui, &theme, ic::SPEED, "Profiler", "Run with Profiling to see where time goes");
        let r = ui.available_rect_before_wrap();
        let br = Rect::from_center_size(r.center() + vec2(0.0, 64.0), vec2(160.0, 32.0));
        let resp = ui.interact(br, Id::unique("start-profile"), Sense::click());
        ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { mix(theme.text_accent, Color32::WHITE, 0.1) } else { theme.text_accent });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, "Profile Program", ui::medium(13.5), Color32::WHITE);
        if resp.clicked() {
            start_run(app, i, Instrument::Profile);
        }
        return;
    };
    let mut nodes = p.nodes.clone();
    nodes.sort_by(|a, b| b.self_ms.partial_cmp(&a.self_ms).unwrap_or(std::cmp::Ordering::Equal));
    ui.label(egui::RichText::new(format!("Total {:.0} ms", p.total_ms)).font(ui::medium(13.0)).color(theme.text_muted));
    ui.add_space(6.0);
    let mut jump = None;
    egui::ScrollArea::vertical().id_salt("profile").auto_shrink([false, false]).show(ui, |ui| {
        let w = ui.available_width();
        for n in nodes.iter().take(200) {
            let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
            let frac = if p.total_ms > 0.0 { (n.self_ms / p.total_ms).clamp(0.0, 1.0) as f32 } else { 0.0 };
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac, r.height())), CornerRadius::same(4), with_alpha(theme.text_accent, 40));
            if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(4), tk.hover);
            }
            truncated_label(ui, pos2(r.left() + 8.0, r.center().y), &n.name, ui::mono(12.5), theme.text, r.width() - 170.0);
            ui.painter().text(pos2(r.right() - 90.0, r.center().y), Align2::RIGHT_CENTER, format!("{:.1} ms", n.self_ms), ui::ui_font(12.0), theme.text);
            ui.painter().text(pos2(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, format!("{:.1} ms", n.total_ms), ui::ui_font(12.0), theme.text_placeholder);
            if resp.clicked() {
                jump = n.srcloc.clone();
            }
        }
    });
    if let Some(l) = jump {
        jump_to_srcloc(app, &l);
    }
}

// ---------------------------------------------------------------------------------- module browser

fn modules_panel(app: &mut App, ui: &mut Ui, rect: Rect) {
    if no_tab(app, ui, ic::ACCOUNT_TREE, "Module Browser", "") {
        return;
    }
    let theme = app.theme.clone();
    let tk = app.tokens();
    let i = app.active;
    let Some(mv) = app.tabs[i].rk.modules.as_ref() else {
        empty_state(ui, &theme, ic::ACCOUNT_TREE, "Module Browser", "See which modules this file requires");
        let r = ui.available_rect_before_wrap();
        let br = Rect::from_center_size(r.center() + vec2(0.0, 64.0), vec2(160.0, 32.0));
        let resp = ui.interact(br, Id::unique("start-modules"), Sense::click());
        ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { mix(theme.text_accent, Color32::WHITE, 0.1) } else { theme.text_accent });
        ui.painter().text(br.center(), Align2::CENTER_CENTER, "Show Modules", ui::medium(13.5), Color32::WHITE);
        if resp.clicked() {
            request_modules(app, i, false);
        }
        return;
    };
    // Toolbar.
    let include_libs = mv.include_libs;
    let mut toggle_libs = false;
    let bar = Rect::from_min_size(rect.min + vec2(10.0, 4.0), vec2(rect.width() - 20.0, 34.0));
    let mut b = ui.new_child(UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let mut on = include_libs;
    if crate::ui::settings_view::toggle(&mut b, &theme, &mut on).changed() {
        toggle_libs = true;
    }
    b.label(egui::RichText::new("Show library modules").color(theme.text_muted));
    if toggle_libs {
        request_modules(app, i, !include_libs);
        return;
    }
    let mv = app.tabs[i].rk.modules.as_ref().unwrap();
    if mv.loading {
        let mut c = ui.new_child(UiBuilder::new().max_rect(rect.shrink(40.0)));
        c.spinner();
        return;
    }
    if let Some(e) = &mv.error {
        let mut c = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 50.0))));
        c.label(egui::RichText::new(e).color(theme.error));
        return;
    }
    let Some(g) = mv.graph.clone() else { return };
    // Layer nodes by longest path from the root (node 0 is the file itself).
    let n = g.nodes.len();
    let mut layer = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(a, b) in &g.edges {
            if a < n && b < n && layer[b] < layer[a] + 1 && layer[a] + 1 < n {
                layer[b] = layer[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let layers = layer.iter().copied().max().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for (k, &l) in layer.iter().enumerate() {
        rows[l].push(k);
    }
    let node_h = 30.0;
    let gap_y = 56.0;
    let font = ui::medium(12.5);
    let widths: Vec<f32> = g.nodes.iter().map(|nd| ui.painter().layout_no_wrap(nd.label.clone(), font.clone(), theme.text).size().x + 40.0).collect();
    let row_w: Vec<f32> = rows.iter().map(|r| r.iter().map(|&k| widths[k] + 14.0).sum::<f32>()).collect();
    let canvas_w = row_w.iter().copied().fold(rect.width() - 30.0, f32::max);
    let canvas_h = layers as f32 * (node_h + gap_y) + 20.0;
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + 44.0), rect.max);
    ui.painter().hline(rect.left()..=rect.right(), body.top() - 2.0, Stroke::new(1.0, tk.island_border));
    let mut child = ui.new_child(UiBuilder::new().max_rect(body));
    child.set_clip_rect(body.intersect(ui.clip_rect()));
    let mut open_path = None;
    egui::ScrollArea::both().id_salt("modules").auto_shrink([false, false]).show(&mut child, |ui| {
        let (area, _) = ui.allocate_exact_size(vec2(canvas_w + 20.0, canvas_h), Sense::hover());
        let mut pos = vec![Rect::NOTHING; n];
        for (l, row) in rows.iter().enumerate() {
            let mut x = area.left() + 10.0 + (canvas_w - row_w[l]) / 2.0;
            let y = area.top() + 14.0 + l as f32 * (node_h + gap_y);
            for &k in row {
                pos[k] = Rect::from_min_size(pos2(x, y), vec2(widths[k], node_h));
                x += widths[k] + 14.0;
            }
        }
        let p = ui.painter();
        for &(a, b) in &g.edges {
            if a >= n || b >= n {
                continue;
            }
            let from = pos[a].center_bottom();
            let to = pos[b].center_top();
            let c = if g.nodes[b].lib { with_alpha(theme.text_muted, 70) } else { with_alpha(theme.text_accent, 140) };
            let mid = (to.y - from.y) * 0.5;
            p.add(egui::epaint::CubicBezierShape::from_points_stroke([from, from + vec2(0.0, mid), to - vec2(0.0, mid), to], false, Color32::TRANSPARENT, Stroke::new(1.3, c)));
            p.circle_filled(to, 2.5, c);
        }
        for (k, nd) in g.nodes.iter().enumerate() {
            let r = pos[k];
            let resp = ui.interact(r, Id::unique(("module-node", k)), Sense::click());
            let root = k == 0;
            let fill = if root { mix(tk.island, theme.text_accent, 0.22) } else if resp.hovered() { tk.pressed } else { tk.hover };
            let stroke = if root { Stroke::new(1.0, with_alpha(theme.text_accent, 180)) } else { Stroke::new(1.0, tk.island_border) };
            ui.painter().rect(r, CornerRadius::same(15), fill, stroke, StrokeKind::Inside);
            let glyph = if nd.lib { ic::INVENTORY_2 } else { ic::DESCRIPTION };
            let gc = if nd.lib { theme.text_placeholder } else { theme.syntax.function.color };
            ui.painter().text(pos2(r.left() + 16.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(15.0), gc);
            ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, &nd.label, font.clone(), if nd.lib { theme.text_muted } else { theme.text });
            if let Some(path) = &nd.path
                && !nd.lib
            {
                let resp = resp.on_hover_text(crate::config::tildify(Path::new(path))).on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    open_path = Some(PathBuf::from(path));
                }
            }
        }
    });
    if let Some(p) = open_path {
        app.open_file(&p);
    }
}

// =====================================================================================
// Blue box overlay (signature of the function at the cursor, like DrRacket's corner box)

pub fn call_box(app: &mut App, ui: &Ui, ed_rect: Rect) {
    let theme = app.theme.clone();
    let tk = app.tokens();
    let Some(tab) = app.active_tab() else { return };
    if !tab.editor.has_focus(ui.ctx()) {
        return;
    }
    let Some((name, lines)) = tab.rk.call_box.clone() else { return };
    let head = tab.editor.doc.buffer.selections.primary().head;
    let _ = head;
    let font = ui::mono((app.font_size() * 0.72).round().max(11.0));
    let max_w = (ed_rect.width() * 0.6).min(560.0);
    let galleys: Vec<_> = lines.iter().take(6).map(|l| {
        let mut job = racket_job(l, &theme, &font, &[], Color32::TRANSPARENT);
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_w - 24.0);
        ui.painter().layout_job(job)
    }).collect();
    let h: f32 = galleys.iter().map(|g| g.size().y + 2.0).sum::<f32>() + 18.0;
    let w = galleys.iter().map(|g| g.size().x).fold(0.0_f32, f32::max) + 24.0;
    let r = Rect::from_min_size(pos2(ed_rect.right() - w - 22.0, ed_rect.bottom() - h - 12.0), vec2(w, h));
    let bg = mix(tk.popup, theme.text_accent, if theme.dark { 0.07 } else { 0.05 });
    ui.painter().rect(r, CornerRadius::same(8), bg, Stroke::new(1.0, with_alpha(theme.text_accent, 60)), StrokeKind::Inside);
    let mut y = r.top() + 9.0;
    for g in galleys {
        let gh = g.size().y;
        ui.painter().galley(pos2(r.left() + 12.0, y), g, theme.text);
        y += gh + 2.0;
    }
    let _ = name;
    let _ = view::hover_card;
}
