//! The editor widget: custom-painted, laying out only visible lines.
//!
//! Features: multi-cursor editing, Racket-aware typing, rainbow brackets, structure-aware
//! indent guides, DrRacket-style shading of the s-expression at the cursor, an animated caret,
//! inline result chips, diagnostics squiggles, check-syntax binding arrows and hover cards,
//! occurrence highlighting, sticky header, scrollbar with markers, completion popup and a
//! find/replace bar.

use super::buffer::{Selection, Selections};
use super::document::{Document, Motion};
use super::lexer::{self, Class, Kind, Token};
use super::sexp;
use crate::config::{CursorShape, Settings};
use crate::theme::{Highlight, Theme, mix, with_alpha};
use crate::ui::{self, Tokens, ic};
use egui::text::{CCursor, LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, CursorIcon, Event, FontId, Galley, Id, Key, Modifiers, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorMode {
    /// A full code editor with gutter.
    Code,
    /// The REPL input: no gutter, grows with content, Enter submits balanced input.
    Repl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub range: Range<usize>,
    pub severity: Severity,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayKind {
    Pass,
    Fail,
}

/// A test result, anchored to the test's source range: a tick or cross in the gutter, and for
/// failures a chip after the end of the line. Editing inside the test removes it (stale).
#[derive(Clone, Debug)]
pub struct Inlay {
    pub range: Range<usize>,
    pub text: String,
    pub kind: InlayKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// Runtime error location (DrRacket's pink highlight).
    Error,
    /// Stepper: the expression about to be reduced.
    StepBefore,
    /// Code not covered by tests.
    Uncovered,
    /// Debugger: current expression.
    Debug,
    /// Visualizer: the code of the current step.
    Viz,
}

#[derive(Clone, Debug)]
pub struct Mark {
    pub range: Range<usize>,
    pub kind: MarkKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowKind {
    Lexical,
    Imported,
    Tail,
}

/// Check-syntax binding arrow: `from` is the binding occurrence, `to` a bound one.
#[derive(Clone, Debug)]
pub struct Arrow {
    pub from: Range<usize>,
    pub to: Range<usize>,
    pub kind: ArrowKind,
}

/// Semantic information from check syntax.
#[derive(Clone, Debug, Default)]
pub struct Semantic {
    pub arrows: Vec<Arrow>,
    pub tooltips: Vec<(Range<usize>, String)>,
    pub unused: Vec<Range<usize>>,
    pub docs: Vec<(Range<usize>, String)>,
}

impl Semantic {
    /// All occurrences (binder + uses) of the identifier at `pos`.
    pub fn occurrences(&self, pos: usize) -> Vec<Range<usize>> {
        let hit = |r: &Range<usize>| r.start <= pos && pos <= r.end;
        let binder = self
            .arrows
            .iter()
            .filter(|a| a.kind == ArrowKind::Lexical)
            .find(|a| hit(&a.from) || hit(&a.to))
            .map(|a| a.from.clone());
        let Some(b) = binder else { return Vec::new() };
        let mut out = vec![b.clone()];
        out.extend(self.arrows.iter().filter(|a| a.kind == ArrowKind::Lexical && a.from == b).map(|a| a.to.clone()));
        out.sort_by_key(|r| r.start);
        out.dedup();
        out
    }
}

/// Everything painted on top of the text that comes from outside the editor.
#[derive(Clone, Debug, Default)]
pub struct Decorations {
    pub diagnostics: Vec<Diagnostic>,
    pub inlays: Vec<Inlay>,
    pub marks: Vec<Mark>,
    /// Lines with breakpoints.
    pub breakpoints: BTreeSet<usize>,
    pub semantic: Option<Semantic>,
    /// Documentation lines for the identifier under the mouse (blue box), keyed by range.
    pub hover_doc: Option<(Range<usize>, Vec<String>)>,
    /// The visualizer's note for the code it is showing (`k is 12 → Yes`), written faintly
    /// after the end of that line.
    pub viz_note: Option<(usize, String)>,
}

impl Decorations {
    /// Shift all ranges through an edit (range in pre-edit coordinates, inserted length).
    pub fn shift(&mut self, edit: &Range<usize>, inserted: usize) {
        let map = |r: &mut Range<usize>| {
            let s = super::buffer::map_pos(r.start, edit, inserted, false);
            let e = super::buffer::map_pos(r.end, edit, inserted, true);
            *r = s..e.max(s);
        };
        self.diagnostics.iter_mut().for_each(|d| map(&mut d.range));
        // A result no longer describes a test that was edited (or deleted); edits elsewhere
        // just move it along.
        let touched = |r: &Range<usize>| edit.start < r.end && (edit.end > r.start || edit.start > r.start);
        self.inlays.retain(|i| !touched(&i.range));
        self.inlays.iter_mut().for_each(|i| map(&mut i.range));
        self.marks.iter_mut().for_each(|m| map(&mut m.range));
        if let Some(sem) = &mut self.semantic {
            for a in &mut sem.arrows {
                map(&mut a.from);
                map(&mut a.to);
            }
            sem.tooltips.iter_mut().for_each(|t| map(&mut t.0));
            sem.unused.iter_mut().for_each(map);
            sem.docs.iter_mut().for_each(|d| map(&mut d.0));
        }
        if let Some((r, _)) = &mut self.hover_doc {
            map(r);
        }
        if let Some((p, _)) = &mut self.viz_note {
            *p = super::buffer::map_pos(*p, edit, inserted, false);
        }
    }
}

/// Things the editor asks its host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorAction {
    GotoDefinition(usize),
    Rename(usize),
    OpenDocs(usize),
    ToggleBreakpoint(usize),
    /// The mouse rested on `pos`; the host may supply `hover_doc`.
    Hover(usize),
    /// The cursor is inside a call to this function (signature help / blue box).
    CallContext(String),
    /// REPL: Enter on balanced input.
    Submit,
    HistoryPrev,
    HistoryNext,
    RunSelection,
    /// Something Vim mode asks of the app (save, close, an app command).
    Vim(crate::editor::vim::Effect),
}

/// Trackpad scrolling locks to the axis a gesture starts on, so a slightly diagonal swipe
/// scrolls straight up/down. A clearly sideways motion still scrolls horizontally.
#[derive(Clone, Copy, Default)]
struct ScrollLock {
    axis: Option<ScrollAxis>,
    last: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum ScrollAxis {
    Vertical,
    Horizontal,
}

impl ScrollLock {
    /// A pause this long (seconds) ends a gesture.
    const GESTURE_GAP: f64 = 0.25;
    /// How strongly the other axis must dominate to break the lock.
    const BREAK_RATIO: f32 = 3.0;

    fn filter(&mut self, delta: Vec2, now: f64) -> Vec2 {
        if now - self.last > Self::GESTURE_GAP {
            self.axis = None;
        }
        self.last = now;
        let (ax, ay) = (delta.x.abs(), delta.y.abs());
        let axis = match self.axis {
            Some(ScrollAxis::Vertical) if ax > ay * Self::BREAK_RATIO => ScrollAxis::Horizontal,
            Some(ScrollAxis::Horizontal) if ay > ax * Self::BREAK_RATIO => ScrollAxis::Vertical,
            Some(a) => a,
            None if ax > ay * 1.5 => ScrollAxis::Horizontal,
            None => ScrollAxis::Vertical,
        };
        self.axis = Some(axis);
        match axis {
            ScrollAxis::Vertical => Vec2::new(0.0, delta.y),
            ScrollAxis::Horizontal => Vec2::new(delta.x, 0.0),
        }
    }
}

pub struct EditorStyle<'a> {
    pub theme: &'a Theme,
    pub settings: &'a Settings,
    pub font_size: f32,
    pub mode: EditorMode,
}

#[derive(Clone)]
struct Completion {
    anchor: usize,
    items: Vec<(String, bool)>,
    selected: usize,
}

#[derive(Default)]
pub struct FindBar {
    pub open: bool,
    pub replace_open: bool,
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub whole_word: bool,
    matches: Vec<Range<usize>>,
    matches_key: (u64, String, bool, bool, bool),
    current: usize,
    focus_query: bool,
    pub error: bool,
}

/// Geometry of the last frame, for hit testing.
#[derive(Clone, Copy)]
struct Geometry {
    rect: Rect,
    text_left: f32,
    top: f32,
    line_h: f32,
    char_w: f32,
}

impl Default for Geometry {
    fn default() -> Self {
        Self { rect: Rect::NOTHING, text_left: 0.0, top: 0.0, line_h: 20.0, char_w: 8.0 }
    }
}

pub struct EditorView {
    pub doc: Document,
    pub deco: Decorations,
    pub find: FindBar,
    /// Words offered by completion besides identifiers in the document.
    pub completion_words: Arc<Vec<String>>,
    scroll_lock: ScrollLock,
    pub read_only: bool,
    id: Id,
    scroll: Vec2,
    max_line_chars: usize,
    goal_col: Option<usize>,
    last_activity: f64,
    caret_anim: Option<Pos2>,
    /// The current mouse press already placed the caret (so its click on release doesn't).
    press_placed: bool,
    geom: Geometry,
    galleys: HashMap<usize, Arc<Galley>>,
    drag_anchor: Option<(usize, usize)>,
    drag_unit_word: bool,
    hover: Option<(Pos2, f64)>,
    hover_sent: Option<usize>,
    completion: Option<Completion>,
    scroll_to_cursor: bool,
    center_on_cursor: bool,
    last_call_ctx: Option<String>,
    focus_request: bool,
    pub actions: Vec<EditorAction>,
    /// Lines the host wants revealed (e.g. error locations).
    pending_reveal: Option<usize>,
    /// Vim mode's state, while the setting is on.
    pub vim: Option<crate::editor::vim::Vim>,
    /// A scroll Vim asked for (zz, ^D...), applied once the viewport is known.
    vim_scroll: Option<crate::editor::vim::ScrollTo>,
}

const SCROLLBAR_W: f32 = 12.0;

impl EditorView {
    pub fn new(text: &str, id: Id) -> Self {
        Self {
            doc: Document::new(text),
            deco: Decorations::default(),
            find: FindBar::default(),
            completion_words: Arc::new(Vec::new()),
            scroll_lock: ScrollLock::default(),
            read_only: false,
            id,
            scroll: Vec2::ZERO,
            max_line_chars: 0,
            goal_col: None,
            last_activity: 0.0,
            caret_anim: None,
            press_placed: false,
            geom: Geometry::default(),
            galleys: HashMap::new(),
            drag_anchor: None,
            drag_unit_word: false,
            hover: None,
            hover_sent: None,
            completion: None,
            scroll_to_cursor: false,
            center_on_cursor: false,
            last_call_ctx: None,
            focus_request: false,
            actions: Vec::new(),
            pending_reveal: None,
            vim: None,
            vim_scroll: None,
        }
    }

    /// Insert an image at the cursor(s).
    pub fn insert_image(&mut self, image: crate::editor::images::EmbeddedImage) {
        let c = self.doc.images.add(image);
        self.doc.paste(&c.to_string());
        self.completion = None;
    }

    pub fn request_focus(&mut self) {
        self.focus_request = true;
    }

    pub fn has_focus(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(self.id))
    }

    /// Put the cursor at `pos` (selecting `pos..end`) and scroll it into view, centered.
    pub fn reveal(&mut self, range: Range<usize>) {
        let n = self.doc.buffer.len();
        let (s, e) = (range.start.min(n), range.end.min(n));
        self.doc.buffer.selections = Selections::single(Selection::new(s, e));
        self.center_on_cursor = true;
        self.pending_reveal = Some(self.doc.buffer.line_of(s));
    }

    /// Scroll so `pos` is visible without moving the cursor.
    pub fn reveal_soft(&mut self, pos: usize) {
        let line = self.doc.buffer.line_of(pos.min(self.doc.buffer.len()));
        let top = (self.scroll.y / self.geom.line_h).floor() as usize;
        let visible = ((self.geom.rect.height() / self.geom.line_h) as usize).saturating_sub(2);
        if line < top || line > top + visible {
            self.scroll.y = (line as f32 * self.geom.line_h - self.geom.rect.height() * 0.35).max(0.0);
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.doc.buffer.set_text(text);
        self.doc.buffer.selections = Selections::single(Selection::caret(self.doc.buffer.len()));
        self.scroll_to_cursor = true;
    }

    /// Keep decorations aligned with edits made since the last frame.
    fn shift_decorations(&mut self) {
        for (r, n) in self.doc.take_edit_log() {
            self.deco.shift(&r, n);
        }
    }

    fn font(&self, style: &EditorStyle) -> FontId {
        ui::mono(style.font_size)
    }

    // ================================================================= layout helpers

    fn line_y(&self, line: usize) -> f32 {
        self.geom.top + line as f32 * self.geom.line_h - self.scroll.y
    }

    /// Screen x of a char position on its line (uses this frame's galley if available).
    /// Thumbnail size of an embedded image: as tall as a line, keeping its aspect ratio.
    fn image_thumb_size(&self, img: &crate::editor::images::EmbeddedImage) -> Vec2 {
        let h = (self.geom.line_h - 4.0).max(8.0);
        let w = (h * img.width as f32 / img.height.max(1) as f32).clamp(h * 0.4, self.geom.char_w * 16.0);
        vec2(w, h)
    }

    /// Paint embedded images over their placeholder glyphs; hovering shows one full size.
    fn paint_images(&self, ui: &Ui, first: usize, last: usize) {
        if self.doc.images.is_empty() {
            return;
        }
        let chars = self.doc.chars();
        let b = &self.doc.buffer;
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut preview = None;
        for line in first..=last {
            let (ls, le) = (b.line_start(line), b.line_end(line));
            for (k, &c) in chars[ls..le].iter().enumerate() {
                if !crate::editor::images::is_image_char(c) {
                    continue;
                }
                let Some(img) = self.doc.images.get(c) else { continue };
                let size = self.image_thumb_size(img);
                // egui puts the extra width before the placeholder glyph: end the thumbnail
                // where the next character starts.
                let x = self.x_of(line, k + 1) - size.x - 2.0;
                let r = Rect::from_min_size(pos2(x, self.line_y(line) + (self.geom.line_h - size.y) / 2.0), size);
                if !ui.clip_rect().intersects(r) {
                    continue;
                }
                let uri = format!("bytes://embedded/{:p}.png", std::sync::Arc::as_ptr(&img.png));
                egui::Image::from_bytes(uri.clone(), egui::load::Bytes::Shared(img.png.clone())).corner_radius(2).paint_at(ui, r);
                if pointer.is_some_and(|p| r.contains(p)) {
                    preview = Some((uri, img.clone(), r));
                }
            }
        }
        if let Some((uri, img, r)) = preview {
            let scale = (320.0 / img.width.max(img.height).max(1) as f32).min(2.0);
            let size = vec2(img.width as f32, img.height as f32) * scale;
            egui::Area::new(self.id.with("image-preview")).order(egui::Order::Tooltip).fixed_pos(r.left_bottom() + vec2(0.0, 6.0)).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    ui.add(egui::Image::from_bytes(uri, egui::load::Bytes::Shared(img.png.clone())).fit_to_exact_size(size));
                    ui.label(egui::RichText::new(format!("{} × {}", img.width, img.height)).size(11.0).weak());
                });
            });
        }
    }

    fn x_of(&self, line: usize, col: usize) -> f32 {
        if let Some(g) = self.galleys.get(&line) {
            return self.geom.text_left - self.scroll.x + g.pos_from_cursor(CCursor::new(col)).min.x;
        }
        self.geom.text_left - self.scroll.x + col as f32 * self.geom.char_w
    }

    fn pos_at(&self, p: Pos2) -> usize {
        let b = &self.doc.buffer;
        let rel_y = p.y - self.geom.top + self.scroll.y;
        let line = if rel_y < 0.0 { 0 } else { ((rel_y / self.geom.line_h) as usize).min(b.line_count().saturating_sub(1)) };
        let rel_x = p.x - self.geom.text_left + self.scroll.x;
        let col = if let Some(g) = self.galleys.get(&line) {
            g.cursor_from_pos(vec2(rel_x.max(0.0), g.size().y / 2.0)).index.0
        } else {
            ((rel_x / self.geom.char_w).round().max(0.0)) as usize
        };
        b.line_start(line) + col.min(b.line_len(line))
    }

    fn highlight_for(&self, class: Class, depth: u16, theme: &Theme, rainbow: bool) -> Highlight {
        let s = &theme.syntax;
        match class {
            Class::Plain => Highlight { color: theme.editor_fg, italic: false, bold: false },
            Class::Comment => s.comment,
            Class::String => s.string,
            Class::Escape => s.string_escape,
            Class::Regex => s.regex,
            Class::Number => s.number,
            Class::Constant => s.constant,
            Class::HashKeyword => s.hash_keyword,
            Class::Symbol => s.symbol,
            Class::Keyword => s.keyword,
            Class::Function => s.function,
            Class::Operator => s.operator,
            Class::Variable => s.variable,
            Class::Bracket => {
                if rainbow {
                    Highlight { color: theme.rainbow(depth as usize), italic: false, bold: false }
                } else {
                    s.punctuation
                }
            }
            Class::Punctuation => s.punctuation,
            Class::Lang => s.keyword,
            Class::LangName => s.lang,
            Class::Error => Highlight { color: theme.error, italic: false, bold: false },
        }
    }

    /// Build the colored layout job for one line.
    fn line_job(&self, line: usize, style: &EditorStyle, toks: &[Token]) -> LayoutJob {
        let b = &self.doc.buffer;
        let ls = b.line_start(line);
        let le = b.line_end(line);
        let chars = self.doc.chars();
        let font = self.font(style);
        let theme = style.theme;
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        let unused = self.deco.semantic.as_ref().map(|s| &s.unused);
        let images = &self.doc.images;
        let space_w = self.geom.char_w;
        let push = |job: &mut LayoutJob, from: usize, to: usize, h: Highlight, dim: bool| {
            if from >= to {
                return;
            }
            let color = if dim { with_alpha(h.color, 120) } else { h.color };
            let format = TextFormat { font_id: font.clone(), color, italics: h.italic, ..Default::default() };
            // An embedded image shows as a space as wide as its thumbnail (one char, one
            // glyph, so carets and selections line up); the thumbnail is painted over it.
            let mut run = String::new();
            for &c in &chars[from..to] {
                match images.get(c).filter(|_| crate::editor::images::is_image_char(c)) {
                    Some(img) => {
                        if !run.is_empty() {
                            job.append(&std::mem::take(&mut run), 0.0, format.clone());
                        }
                        let w = self.image_thumb_size(img).x + 4.0;
                        job.append(" ", 0.0, TextFormat { extra_letter_spacing: (w - space_w).max(0.0), ..format.clone() });
                    }
                    None => run.push(if c == '\t' { ' ' } else { c }),
                }
            }
            if !run.is_empty() {
                job.append(&run, 0.0, format);
            }
        };
        let plain = Highlight { color: theme.editor_fg, italic: false, bold: false };
        let mut i = lexer::token_index_at(toks, ls);
        let mut at = ls;
        while i < toks.len() && toks[i].start < le {
            let t = toks[i];
            let s = t.start.max(ls);
            let e = t.end.min(le);
            if s > at {
                push(&mut job, at, s, plain, false);
            }
            let name = t.kind == Kind::Symbol && matches!(t.class, Class::Function | Class::Variable);
            // Anything in call position is a function (blue), the program's own included;
            // variables are red.
            let class = t.class;
            let h = if t.mismatched { Highlight { color: theme.error, italic: false, bold: true } } else { self.highlight_for(class, t.depth, theme, style.settings.rainbow_brackets) };
            let dim = name && unused.is_some_and(|u| u.iter().any(|r| r.start == t.start));
            push(&mut job, s, e, h, dim);
            at = e;
            i += 1;
        }
        if le > at {
            push(&mut job, at, le, plain, false);
        }
        if job.sections.is_empty() {
            job.append("", 0.0, TextFormat { font_id: font, color: theme.editor_fg, ..Default::default() });
        }
        job
    }

    // ================================================================= main entry

    /// Show the editor filling `rect`. In REPL mode the height adapts to content.
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, style: &EditorStyle) -> Response {
        let ctx = ui.ctx().clone();
        let theme = style.theme;
        let tk = Tokens::of(theme);
        let font = self.font(style);
        let char_w = ctx.fonts_mut(|f| f.glyph_width(&font, 'M'));
        let line_h = (style.font_size * style.settings.line_height).round();
        let code = style.mode == EditorMode::Code;

        self.doc.sync();
        self.shift_decorations();

        // ---- geometry
        let line_count = self.doc.buffer.line_count();
        let digits = format!("{}", line_count.max(99)).len() as f32;
        let gutter_w = if code && style.settings.show_line_numbers { 22.0 + digits * char_w + 18.0 } else if code { 26.0 } else { 0.0 };
        let pad_top = if code { 6.0 } else { 0.0 };
        let text_left = rect.left() + gutter_w + if code { 6.0 } else { 0.0 };
        self.geom = Geometry { rect, text_left, top: rect.top() + pad_top, line_h, char_w };

        // ---- interaction
        let resp = ui.interact(rect, self.id, Sense::click_and_drag());
        if resp.clicked() || resp.drag_started() {
            resp.request_focus();
        }
        if self.focus_request {
            resp.request_focus();
            self.focus_request = false;
            if std::env::var_os("DRROCKET_TRACE").is_some() {
                eprintln!("focus requested: {:?}", self.id);
            }
        }
        if resp.hovered() {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        let focused = resp.has_focus();
        if std::env::var_os("DRROCKET_TRACE").is_some() && focused != ui.ctx().data(|d| d.get_temp::<bool>(self.id.with("was-focused")).unwrap_or(false)) {
            eprintln!("focus {:?} -> {focused}", self.id);
            ui.ctx().data_mut(|d| d.insert_temp(self.id.with("was-focused"), focused));
        }
        if std::env::var_os("DRROCKET_TRACE").is_some() && ui.input(|i| i.events.iter().any(|e| matches!(e, Event::Text(_)))) {
            eprintln!("text event: editor {:?} focused={focused} focus_request={} mode={:?}", self.id, self.focus_request, style.mode);
        }
        // Vim mode follows the setting (code editors only).
        let vim_on = style.settings.vim_mode && code && !self.read_only;
        if vim_on != self.vim.is_some() {
            self.vim = vim_on.then(crate::editor::vim::Vim::new);
        }
        if focused {
            ui.memory_mut(|m| m.set_focus_lock_filter(self.id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: self.find.open || self.completion.is_some() || self.vim.is_some() }));
        }
        let now = ctx.input(|i| i.time);
        let version_before = self.doc.version();
        let sel_before = self.doc.buffer.selections.clone();

        self.handle_mouse(ui, &resp, style, now);
        // A mouse selection continues in visual mode; a click leaves it.
        if let Some(v) = self.vim.as_mut() {
            let sel = self.doc.buffer.selections.primary();
            if self.doc.buffer.selections != sel_before {
                if sel.is_empty() && matches!(v.mode, crate::editor::vim::Mode::Visual | crate::editor::vim::Mode::VisualLine) {
                    v.mode = crate::editor::vim::Mode::Normal;
                } else {
                    v.select_from_mouse(sel);
                }
            }
        }
        if focused && !self.read_only {
            self.handle_keys(ui, style);
        } else if focused {
            self.handle_readonly_keys(ui);
        }
        self.doc.sync();
        self.shift_decorations();
        // Input may have added or removed lines; everything below lays out the edited text.
        let line_count = self.doc.buffer.line_count();
        if self.doc.version() != version_before || self.doc.buffer.selections != sel_before {
            self.last_activity = now;
            self.scroll_to_cursor = true;
            if self.doc.version() != version_before {
                self.hover = None;
            }
        }

        // ---- scrolling
        let viewport_h = rect.height() - pad_top;
        let content_h = line_count as f32 * line_h;
        let overscroll = if code { (viewport_h - line_h * 3.0).max(0.0) } else { 0.0 };
        let max_scroll_y = (content_h + overscroll - viewport_h).max(0.0);
        if resp.hovered() || ui.rect_contains_pointer(rect) {
            let (delta, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
            if delta != Vec2::ZERO && !ui.ctx().is_being_dragged(self.id.with("vscroll")) {
                if shift && delta.x == 0.0 {
                    self.scroll.x -= delta.y;
                } else {
                    let d = self.scroll_lock.filter(delta, now);
                    self.scroll.y -= d.y;
                    self.scroll.x -= d.x;
                }
                self.hover = None;
            }
        }
        if let Some(s) = self.vim_scroll.take() {
            use crate::editor::vim::ScrollTo;
            let cur = self.doc.buffer.line_of(self.doc.buffer.selections.primary().head) as f32 * line_h;
            match s {
                ScrollTo::Center => self.scroll.y = cur - (viewport_h - line_h) / 2.0,
                ScrollTo::Top => self.scroll.y = cur,
                ScrollTo::Bottom => self.scroll.y = cur + line_h - viewport_h,
                ScrollTo::Lines(n) => self.scroll.y += n as f32 * line_h,
            }
            self.scroll_to_cursor = !matches!(s, ScrollTo::Center | ScrollTo::Top | ScrollTo::Bottom);
        }
        if let Some(line) = self.pending_reveal.take() {
            let y = line as f32 * line_h;
            if self.center_on_cursor || y < self.scroll.y || y + line_h > self.scroll.y + viewport_h {
                self.scroll.y = y - viewport_h * 0.35;
            }
            self.center_on_cursor = false;
            self.scroll_to_cursor = false;
        }
        if self.scroll_to_cursor && code {
            let head = self.doc.buffer.selections.primary().head;
            let line = self.doc.buffer.line_of(head);
            let y = line as f32 * line_h;
            let margin = line_h * 2.0;
            if y < self.scroll.y + margin {
                self.scroll.y = y - margin;
            } else if y + line_h > self.scroll.y + viewport_h - margin {
                self.scroll.y = y + line_h - viewport_h + margin;
            }
            let col = self.doc.buffer.col_of(head) as f32 * char_w;
            let text_w = rect.right() - text_left - SCROLLBAR_W - 8.0;
            if col < self.scroll.x {
                self.scroll.x = (col - 4.0 * char_w).max(0.0);
            } else if col > self.scroll.x + text_w - 2.0 * char_w {
                self.scroll.x = col - text_w + 4.0 * char_w;
            }
            self.scroll_to_cursor = false;
        }
        self.scroll.y = self.scroll.y.clamp(0.0, max_scroll_y);
        let max_scroll_x = (self.max_line_chars as f32 * char_w - (rect.right() - text_left - SCROLLBAR_W - 40.0)).max(0.0);
        self.scroll.x = self.scroll.x.clamp(0.0, max_scroll_x);

        // ---- visible lines
        let first = (self.scroll.y / line_h).floor() as usize;
        let visible = (viewport_h / line_h).ceil() as usize + 1;
        let last = (first + visible).min(line_count.saturating_sub(1));
        let painter = ui.painter_at(rect);
        // Signature context for the blue box (needs the document's own token access).
        let primary0 = self.doc.buffer.selections.primary();
        if focused && primary0.is_empty() {
            let call = self.doc.enclosing_call(primary0.head).map(|c| c.0);
            if call != self.last_call_ctx {
                self.last_call_ctx = call.clone();
                if let Some(c) = call {
                    self.actions.push(EditorAction::CallContext(c));
                }
            }
        }
        // Shared handle: a refcount bump, not a copy.
        let toks_arc = self.doc.tokens_shared();
        let toks: &[Token] = &toks_arc;

        // Lay out visible lines.
        let mut galleys = HashMap::with_capacity(visible + 1);
        for line in first..=last {
            let job = self.line_job(line, style, toks);
            let g = painter.layout_job(job);
            galleys.insert(line, g);
            self.max_line_chars = self.max_line_chars.max(self.doc.buffer.line_len(line));
        }
        self.galleys = galleys;

        let sels = self.doc.buffer.selections.clone();
        let primary = sels.primary();
        let cursor_line = self.doc.buffer.line_of(primary.head);
        let text_clip = Rect::from_min_max(pos2(rect.left() + gutter_w, rect.top()), rect.max);
        let tp = painter.with_clip_rect(text_clip);

        // Active line.
        if style.settings.highlight_active_line && code && sels.list.iter().all(|s| s.is_empty()) && (first..=last).contains(&cursor_line) {
            let y = self.line_y(cursor_line);
            tp.rect_filled(Rect::from_min_max(pos2(text_clip.left(), y), pos2(rect.right(), y + line_h)), CornerRadius::ZERO, with_alpha(theme.active_line, if focused { 255 } else { 140 }));
        }

        // Shading of the s-expression adjacent to the cursor (DrRacket) + bracket match.
        let bracket = if sels.list.len() == 1 && primary.is_empty() { sexp::bracket_pair_at(toks, primary.head) } else { None };
        if let Some((o, c)) = bracket {
            let (s, e) = (toks[o].start, toks[c].end);
            // The pair's own rainbow color makes it easy to see which brackets belong together.
            let pair_color = if style.settings.bracket_pair_highlight && style.settings.rainbow_brackets { Some(theme.rainbow(toks[o].depth as usize)) } else { None };
            let shade = match pair_color {
                Some(pc) => with_alpha(pc, if theme.dark { 12 } else { 18 }),
                None => with_alpha(theme.text, if theme.dark { 9 } else { 12 }),
            };
            self.paint_range_bg(&tp, s, e, first, last, shade);
            for k in [o, c] {
                let t = toks[k];
                let line = self.doc.buffer.line_of(t.start);
                if (first..=last).contains(&line) {
                    let x0 = self.x_of(line, t.start - self.doc.buffer.line_start(line));
                    let x1 = self.x_of(line, t.end - self.doc.buffer.line_start(line));
                    let y = self.line_y(line);
                    let r = Rect::from_min_max(pos2(x0 - 1.0, y + 3.0), pos2(x1 + 1.0, y + line_h - 3.0));
                    let (fill, stroke) = match pair_color {
                        Some(pc) => (with_alpha(pc, 45), Stroke::new(1.0, with_alpha(pc, 200))),
                        None => (with_alpha(theme.bracket_match, 150), Stroke::new(1.0, with_alpha(theme.text_muted, 90))),
                    };
                    tp.rect(r, CornerRadius::same(3), fill, stroke, StrokeKind::Inside);
                }
            }
        }

        // Marks (error / stepper / coverage / debugger).
        for m in &self.deco.marks {
            let (fill, border) = match m.kind {
                MarkKind::Error => (with_alpha(theme.error, 46), Some(theme.error)),
                MarkKind::StepBefore => (with_alpha(theme.syntax.function.color, 50), Some(theme.syntax.function.color)),
                // Uncovered code stands out like DrRacket's orange-on-black coverage display.
                MarkKind::Uncovered => (with_alpha(theme.warning, if theme.dark { 72 } else { 90 }), Some(theme.warning)),
                MarkKind::Debug => (with_alpha(theme.success, 50), Some(theme.success)),
                MarkKind::Viz => (with_alpha(theme.text_accent, 46), Some(theme.text_accent)),
            };
            self.paint_range_bg(&tp, m.range.start, m.range.end, first, last, fill);
            if let Some(b) = border {
                self.paint_range_underline(&tp, m.range.start, m.range.end, first, last, with_alpha(b, 200));
            }
            // The visualizer's current line gets an arrow in the gutter.
            if m.kind == MarkKind::Viz {
                let line = self.doc.buffer.line_of(m.range.start);
                if (first..=last).contains(&line) {
                    let cy = self.line_y(line) + line_h / 2.0;
                    let x = text_left - 6.0;
                    ui.painter_at(rect).add(egui::Shape::convex_polygon(vec![pos2(x, cy), pos2(x - 7.0, cy - 5.0), pos2(x - 7.0, cy + 5.0)], theme.text_accent, Stroke::NONE));
                }
            }
        }

        // Occurrences of the identifier at the cursor.
        if primary.is_empty() {
            let occ = match &self.deco.semantic {
                Some(sem) => sem.occurrences(primary.head),
                None => Vec::new(),
            };
            let occ = if occ.is_empty() { self.textual_occurrences(primary.head, first, last, toks) } else { occ };
            if occ.len() > 1 {
                for r in &occ {
                    self.paint_range_bg(&tp, r.start, r.end, first, last, with_alpha(theme.highlight_read, 120));
                }
            }
        }

        // Search matches.
        if self.find.open {
            self.update_find_matches();
            let lo = self.doc.buffer.line_start(first);
            let hi = self.doc.buffer.line_end(last);
            let start = self.find.matches.partition_point(|r| r.end < lo);
            for (k, r) in self.find.matches.iter().enumerate().skip(start) {
                if r.start > hi {
                    break;
                }
                let c = if k == self.find.current { theme.search_active_match } else { theme.search_match };
                self.paint_range_bg(&tp, r.start, r.end, first, last, c);
            }
        }

        // Selections.
        for s in &sels.list {
            if !s.is_empty() {
                let c = if focused { theme.selection } else { with_alpha(theme.selection, (theme.selection.a() as f32 * 0.6) as u8) };
                self.paint_selection(&tp, s.start(), s.end(), first, last, c);
            }
        }

        // Indent guides (structure-aware: one guide per enclosing multi-line list).
        if style.settings.indent_style != crate::config::IndentStyle::Off && code {
            self.paint_indent_guides(&tp, toks, first, last, style, primary.head);
        }

        // Text.
        for line in first..=last {
            if let Some(g) = self.galleys.get(&line) {
                let y = self.line_y(line) + (line_h - g.size().y) / 2.0;
                tp.galley(pos2(text_left - self.scroll.x, y), g.clone(), theme.editor_fg);
            }
        }
        {
            let mut iui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            iui.set_clip_rect(tp.clip_rect());
            self.paint_images(&iui, first, last);
        }

        // Diagnostics squiggles.
        for d in &self.deco.diagnostics {
            let c = match d.severity {
                Severity::Error => theme.error,
                Severity::Warning => theme.warning,
            };
            self.paint_squiggle(&tp, d.range.start, d.range.end.max(d.range.start + 1), first, last, c);
        }

        // The visualizer's note, after the line it is about.
        if let Some((pos, note)) = &self.deco.viz_note {
            let line = self.doc.buffer.line_of(*pos);
            if (first..=last).contains(&line) {
                let after = self.x_of(line, self.doc.buffer.line_len(line)) + self.geom.char_w * 2.0;
                let y = self.line_y(line) + self.geom.line_h / 2.0;
                let right = self.geom.rect.right() - SCROLLBAR_W - 10.0;
                let mut job = LayoutJob::simple_singleline(note.clone(), ui::mono((style.font_size * 0.86).round()), with_alpha(theme.text_accent, 225));
                job.wrap = egui::text::TextWrapping::truncate_at_width((self.geom.rect.width() * 0.6).max(80.0));
                let g = tp.layout_job(job);
                if after + g.size().x <= right {
                    tp.galley(pos2(after, y - g.size().y / 2.0), g, theme.text_accent);
                } else {
                    // The line runs past the edge: pin the note to the right edge, on a chip so
                    // it reads over the code beneath.
                    let r = Rect::from_min_max(pos2(right - g.size().x - 16.0, y - g.size().y / 2.0 - 3.0), pos2(right, y + g.size().y / 2.0 + 3.0));
                    tp.rect(r, CornerRadius::same(6), style.theme.editor_bg, Stroke::new(1.0, with_alpha(theme.text_accent, 120)), egui::StrokeKind::Inside);
                    tp.galley(pos2(r.left() + 8.0, y - g.size().y / 2.0), g, theme.text_accent);
                }
            }
        }

        // Failed tests: a chip after the line (passes only get a gutter tick).
        if style.settings.test_results_in_editor {
            for inlay in self.deco.inlays.iter().filter(|i| i.kind == InlayKind::Fail) {
                let line = self.doc.buffer.line_of(inlay.range.start);
                if (first..=last).contains(&line) {
                    self.paint_inlay(&tp, line, inlay, theme, style.font_size);
                }
            }
        }

        // Cursors.
        if focused || style.mode == EditorMode::Repl {
            self.paint_cursors(&tp, &ctx, style, now, focused);
        }

        // Gutter.
        if code {
            // In Vim's visual modes the selection ends past the cursor: number from the cursor.
            let number_line = self.vim.as_ref().and_then(|v| v.cursor_pos()).map_or(cursor_line, |c| self.doc.buffer.line_of(c));
            self.paint_gutter(ui, &painter, rect, gutter_w, first, last, number_line, style, &resp);
        }

        // Sticky header: the top-level form enclosing the first visible line.
        if code && self.scroll.y > 0.0 {
            self.paint_sticky(ui, &painter, rect, gutter_w, first, toks, style);
        }

        // Binding arrows + hover card.
        self.handle_hover(ui, &resp, style, now, toks);

        // Scrollbar.
        if code || content_h > viewport_h {
            self.paint_scrollbar(ui, rect, content_h + overscroll, viewport_h, style);
        }

        // Find bar and completion popup.
        if self.find.open && code {
            self.show_find_bar(ui, rect, style);
        }
        if focused {
            self.show_completion(ui, style);
        } else {
            self.completion = None;
        }

        // Vim: the mode, the which-key list after the leader, and the `:` menu.
        if self.vim.is_some() {
            self.paint_vim(ui, rect, style, gutter_w, focused);
        }

        // IME position.
        if focused {
            let head = primary.head;
            let line = self.doc.buffer.line_of(head);
            let x = self.x_of(line, head - self.doc.buffer.line_start(line));
            let y = self.line_y(line);
            let cr = Rect::from_min_size(pos2(x, y), vec2(2.0, line_h));
            ctx.output_mut(|o| o.ime = Some(egui::output::IMEOutput { purpose: egui::IMEPurpose::Normal, rect, cursor_rect: cr, should_interrupt_composition: false }));
        }

        let _ = tk;
        resp
    }

    // ================================================================= painting helpers

    fn line_range_cols(&self, line: usize, s: usize, e: usize) -> (usize, usize) {
        let ls = self.doc.buffer.line_start(line);
        let le = self.doc.buffer.line_end(line);
        (s.clamp(ls, le) - ls, e.clamp(ls, le) - ls)
    }

    fn paint_range_bg(&self, p: &egui::Painter, s: usize, e: usize, first: usize, last: usize, color: Color32) {
        if e <= s && s != e {
            return;
        }
        let b = &self.doc.buffer;
        let (l0, l1) = (b.line_of(s), b.line_of(e));
        for line in l0.max(first)..=l1.min(last) {
            let (c0, c1) = self.line_range_cols(line, s, e);
            let x0 = self.x_of(line, c0);
            let mut x1 = self.x_of(line, c1);
            if line < l1 {
                x1 += self.geom.char_w * 0.5;
            }
            if x1 - x0 < 1.0 && s != e {
                continue;
            }
            let y = self.line_y(line);
            p.rect_filled(Rect::from_min_max(pos2(x0, y + 1.0), pos2(x1, y + self.geom.line_h - 1.0)), CornerRadius::same(3), color);
        }
    }

    fn paint_range_underline(&self, p: &egui::Painter, s: usize, e: usize, first: usize, last: usize, color: Color32) {
        let b = &self.doc.buffer;
        let (l0, l1) = (b.line_of(s), b.line_of(e));
        for line in l0.max(first)..=l1.min(last) {
            let (c0, c1) = self.line_range_cols(line, s, e);
            let (x0, x1) = (self.x_of(line, c0), self.x_of(line, c1));
            let y = self.line_y(line) + self.geom.line_h - 2.0;
            p.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(1.5, color));
        }
    }

    /// Selection painted as one shape per line with rounded corners.
    fn paint_selection(&self, p: &egui::Painter, s: usize, e: usize, first: usize, last: usize, color: Color32) {
        let b = &self.doc.buffer;
        let (l0, l1) = (b.line_of(s), b.line_of(e));
        for line in l0.max(first)..=l1.min(last) {
            let (c0, c1) = self.line_range_cols(line, s, e);
            let x0 = self.x_of(line, c0);
            let mut x1 = self.x_of(line, c1);
            if line < l1 {
                x1 += self.geom.char_w * 0.6;
            }
            let y = self.line_y(line);
            p.rect_filled(Rect::from_min_max(pos2(x0, y), pos2(x1, y + self.geom.line_h)), CornerRadius::same(3), color);
        }
    }

    fn paint_squiggle(&self, p: &egui::Painter, s: usize, e: usize, first: usize, last: usize, color: Color32) {
        let b = &self.doc.buffer;
        let e = e.min(b.len());
        let (l0, l1) = (b.line_of(s), b.line_of(e));
        for line in l0.max(first)..=l1.min(last) {
            let (c0, mut c1) = self.line_range_cols(line, s, e);
            if c1 == c0 {
                c1 = c0 + 1;
            }
            let x0 = self.x_of(line, c0);
            let x1 = self.x_of(line, c1).max(x0 + self.geom.char_w);
            let y = self.line_y(line) + self.geom.line_h * 0.5 + 9.0_f32.min(self.geom.line_h * 0.36);
            let mut pts = Vec::new();
            let mut x = x0;
            let mut up = true;
            while x <= x1 {
                pts.push(pos2(x, if up { y - 1.5 } else { y + 1.5 }));
                x += 2.5;
                up = !up;
            }
            p.add(Shape::line(pts, Stroke::new(1.3, color)));
        }
    }

    fn paint_inlay(&self, p: &egui::Painter, line: usize, inlay: &Inlay, theme: &Theme, font_size: f32) {
        let len = self.doc.buffer.line_len(line);
        let x = self.x_of(line, len) + self.geom.char_w * 2.0;
        let y = self.line_y(line);
        let (fg, bg, glyph) = match inlay.kind {
            InlayKind::Pass => (theme.success, with_alpha(theme.success, 26), Some(ic::CHECK)),
            InlayKind::Fail => (theme.error, with_alpha(theme.error, 30), Some(ic::CLOSE)),
        };
        let font = ui::mono((font_size * 0.78).round());
        let icon_w = if glyph.is_some() { font_size * 0.85 } else { 0.0 };
        // Keep the chip inside the editor: truncate long messages, and fall back to an
        // icon-only chip when there is no room for text at all.
        let room = self.geom.rect.right() - SCROLLBAR_W - 10.0 - x - icon_w - 14.0;
        let text = if room >= 40.0 { inlay.text.clone() } else { String::new() };
        if room < 0.0 || (text.is_empty() && glyph.is_none()) {
            return;
        }
        let mut job = LayoutJob::simple_singleline(text, font, fg);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room.max(1.0));
        let g = p.layout_job(job);
        let h = (self.geom.line_h * 0.72).min(font_size * 1.35);
        let text_w = if g.size().x > 0.5 { g.size().x + 14.0 } else { 10.0 };
        let r = Rect::from_min_size(pos2(x, y + (self.geom.line_h - h) / 2.0), vec2(text_w + icon_w, h));
        p.rect_filled(r, CornerRadius::same(5), bg);
        if let Some(gl) = glyph {
            p.text(pos2(r.left() + 6.0 + icon_w / 2.0, r.center().y), Align2::CENTER_CENTER, gl, ui::icon_font(font_size * 0.8), fg);
        }
        p.galley(pos2(r.left() + 7.0 + icon_w, r.center().y - g.size().y / 2.0), g, fg);
    }

    fn paint_cursors(&mut self, p: &egui::Painter, ctx: &egui::Context, style: &EditorStyle, now: f64, focused: bool) {
        let theme = style.theme;
        let blink_on = if style.settings.cursor_blink && focused {
            let t = now - self.last_activity;
            if t < 0.6 {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.6 - t));
                true
            } else {
                let phase = ((t - 0.6) / 0.53) as i64;
                let next = 0.6 + (phase + 1) as f64 * 0.53 - t;
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(next.max(0.01)));
                phase % 2 == 1
            }
        } else {
            true
        };
        let sels = self.doc.buffer.selections.clone();
        let line_h = self.geom.line_h;
        // Vim: a block in normal and visual mode (on the visual cursor), a bar in insert mode,
        // an underline in replace mode.
        let vim_shape = self.vim.as_ref().map(|v| match v.mode {
            crate::editor::vim::Mode::Insert => CursorShape::Bar,
            crate::editor::vim::Mode::Replace => CursorShape::Underline,
            _ => CursorShape::Block,
        });
        let vim_cursor = self.vim.as_ref().and_then(|v| v.cursor_pos());
        for (k, s) in sels.list.iter().enumerate() {
            let head = if k == sels.primary { vim_cursor.unwrap_or(s.head) } else { s.head };
            let s = &Selection::new(s.anchor, head);
            let line = self.doc.buffer.line_of(s.head);
            let col = s.head - self.doc.buffer.line_start(line);
            let target = pos2(self.x_of(line, col), self.line_y(line));
            // Animate the primary caret (smooth caret movement).
            let pos = if k == sels.primary && style.settings.smooth_caret {
                let prev = self.caret_anim.unwrap_or(target);
                let d = target - prev;
                let next = if d.length() < 0.5 || d.length() > 400.0 { target } else { prev + d * 0.45 };
                if next != target {
                    ctx.request_repaint();
                }
                self.caret_anim = Some(next);
                next
            } else {
                target
            };
            if !blink_on {
                continue;
            }
            let shape = if style.mode == EditorMode::Repl && !focused { CursorShape::Bar } else { vim_shape.unwrap_or(style.settings.cursor_shape) };
            let color = if focused { theme.cursor } else { with_alpha(theme.cursor, 110) };
            match shape {
                CursorShape::Bar => {
                    let w = style.settings.cursor_width.clamp(1.0, 8.0);
                    p.rect_filled(Rect::from_min_size(pos2(pos.x - w / 2.0, pos.y + 3.0), vec2(w, line_h - 6.0)), CornerRadius::same((w / 2.0).min(2.0) as u8), color);
                }
                CursorShape::Block => {
                    let w = self.geom.char_w;
                    let r = Rect::from_min_size(pos2(pos.x, pos.y + 2.0), vec2(w, line_h - 4.0));
                    if focused {
                        p.rect_filled(r, CornerRadius::same(2), color);
                        if let Some(c) = self.doc.buffer.char_at(s.head).filter(|c| !c.is_whitespace()) {
                            p.text(r.center(), Align2::CENTER_CENTER, c.to_string(), ui::mono(style.font_size), theme.editor_bg);
                        }
                    } else {
                        p.rect_stroke(r, CornerRadius::same(2), Stroke::new(1.0, color), StrokeKind::Inside);
                    }
                }
                CursorShape::Underline => {
                    let w = style.settings.cursor_width.clamp(1.0, 6.0);
                    p.rect_filled(Rect::from_min_size(pos2(pos.x, pos.y + line_h - 2.0 - w), vec2(self.geom.char_w, w)), CornerRadius::ZERO, color);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_gutter(&mut self, ui: &Ui, p: &egui::Painter, rect: Rect, gutter_w: f32, first: usize, last: usize, cursor_line: usize, style: &EditorStyle, resp: &Response) {
        let theme = style.theme;
        let gr = Rect::from_min_max(rect.min, pos2(rect.left() + gutter_w, rect.bottom()));
        let font = ui::mono(style.font_size * 0.86);
        let hover_line = resp.hover_pos().filter(|h| gr.contains(*h)).map(|h| ((h.y - self.geom.top + self.scroll.y) / self.geom.line_h) as usize);
        // Diagnostics per line for the gutter dot.
        let mut diag_lines: HashMap<usize, Severity> = HashMap::new();
        for d in &self.deco.diagnostics {
            let l = self.doc.buffer.line_of(d.range.start);
            let e = diag_lines.entry(l).or_insert(d.severity);
            if d.severity == Severity::Error {
                *e = Severity::Error;
            }
        }
        // Test results per line (a failure wins over a pass on the same line).
        let mut test_lines: HashMap<usize, InlayKind> = HashMap::new();
        if style.settings.test_results_in_editor {
            for i in &self.deco.inlays {
                let e = test_lines.entry(self.doc.buffer.line_of(i.range.start)).or_insert(i.kind);
                if i.kind == InlayKind::Fail {
                    *e = InlayKind::Fail;
                }
            }
        }
        // Vim's relative numbers: distances from the cursor, and its own line's number set left.
        let relative = self.vim.is_some() && style.settings.relative_line_numbers;
        let digits_w = {
            let digits = self.doc.buffer.line_count().max(1).ilog10() as usize + 1;
            p.layout_no_wrap("0".repeat(digits), font.clone(), theme.line_number).size().x
        };
        for line in first..=last {
            let y = self.line_y(line);
            let row = Rect::from_min_max(pos2(gr.left(), y), pos2(gr.right(), y + self.geom.line_h));
            let is_cur = line == cursor_line;
            if style.settings.show_line_numbers {
                let color = if is_cur { theme.active_line_number } else { theme.line_number };
                if relative && is_cur {
                    p.text(pos2(gr.right() - 14.0 - digits_w, row.center().y), Align2::LEFT_CENTER, format!("{}", line + 1), font.clone(), color);
                } else {
                    let n = if relative { line.abs_diff(cursor_line) } else { line + 1 };
                    p.text(pos2(gr.right() - 14.0, row.center().y), Align2::RIGHT_CENTER, format!("{n}"), font.clone(), color);
                }
            }
            let bp_center = pos2(gr.left() + 12.0, row.center().y);
            if self.deco.breakpoints.contains(&line) {
                p.circle_filled(bp_center, 5.0, theme.error);
            } else if hover_line == Some(line) {
                p.circle_filled(bp_center, 5.0, with_alpha(theme.error, 70));
            } else if let Some(kind) = test_lines.get(&line) {
                let (glyph, c) = if *kind == InlayKind::Pass { (ic::CHECK, theme.success) } else { (ic::CLOSE, theme.error) };
                p.text(bp_center, Align2::CENTER_CENTER, glyph, ui::icon_font((style.font_size * 0.82).round()), c);
            } else if let Some(sev) = diag_lines.get(&line) {
                let c = if *sev == Severity::Error { theme.error } else { theme.warning };
                p.circle_filled(bp_center, 3.0, c);
            }
        }
        // Click in the left margin toggles a breakpoint.
        if resp.clicked()
            && let Some(h) = resp.interact_pointer_pos()
            && h.x < gr.left() + 24.0
        {
            let line = ((h.y - self.geom.top + self.scroll.y) / self.geom.line_h) as usize;
            if line < self.doc.buffer.line_count() {
                self.actions.push(EditorAction::ToggleBreakpoint(line));
            }
        }
        let _ = ui;
    }

    /// Structure-aware indent guides: one per enclosing multi-line list, at its bracket's
    /// column. Optionally colored by depth (rainbow) and with a translucent depth fill.
    /// indent-rainbow style: each 2-column indentation step gets the next rainbow colour as a
    /// faint band, with a dot on every leading space.
    fn paint_indent_bands(&self, p: &egui::Painter, line: usize, indent: usize, theme: &Theme, col_x: &dyn Fn(usize, usize) -> f32) {
        const STEP: usize = 2;
        let y = self.line_y(line);
        let h = self.geom.line_h;
        let alpha = if theme.dark { 22 } else { 28 };
        for (k, c0) in (0..indent).step_by(STEP).enumerate() {
            let x0 = col_x(line, c0);
            let x1 = col_x(line, (c0 + STEP).min(indent));
            p.rect_filled(Rect::from_min_max(pos2(x0, y), pos2(x1, y + h)), CornerRadius::ZERO, with_alpha(theme.rainbow(k), alpha));
        }
        let dot = with_alpha(theme.text_placeholder, if theme.dark { 90 } else { 110 });
        let cy = y + h / 2.0;
        for col in 0..indent {
            p.circle_filled(pos2((col_x(line, col) + col_x(line, col + 1)) / 2.0, cy), 1.0, dot);
        }
    }

    fn paint_indent_guides(&self, p: &egui::Painter, toks: &[Token], first: usize, last: usize, style: &EditorStyle, cursor: usize) {
        let theme = style.theme;
        let rainbow = style.settings.rainbow_indent_guides;
        let fill = style.settings.indent_style == crate::config::IndentStyle::Highlights;
        let b = &self.doc.buffer;
        let active_open = sexp::enclosing_open(toks, cursor);
        let col_x = |line: usize, col: usize| -> f32 {
            if self.galleys.contains_key(&line) && col <= b.line_len(line) {
                self.x_of(line, col)
            } else {
                self.geom.text_left - self.scroll.x + col as f32 * self.geom.char_w
            }
        };
        let mut guides: Vec<(usize, u16, bool)> = Vec::with_capacity(16);
        for line in first..=last {
            let ls = b.line_start(line);
            let indent = b.indentation(line);
            let blank = b.line_len(line) == indent;
            if fill {
                if !blank {
                    self.paint_indent_bands(p, line, indent, theme, &col_x);
                }
                continue;
            }
            // Opens enclosing this line's start that began on an earlier line.
            guides.clear();
            let mut probe = ls;
            let mut n = 0;
            while let Some(o) = sexp::enclosing_open(toks, probe) {
                if b.line_of(toks[o].start) < line {
                    let col = b.col_of(toks[o].end - 1);
                    if col < indent || (blank && col < 200) {
                        guides.push((col, toks[o].depth, Some(o) == active_open));
                    }
                }
                probe = toks[o].start;
                n += 1;
                if n > 32 {
                    break;
                }
            }
            if guides.is_empty() {
                continue;
            }
            guides.reverse(); // outermost first
            let y = self.line_y(line);
            let h = self.geom.line_h;
            for &(col, depth, active) in &guides {
                let x = col_x(line, col).round() + 0.5;
                let c = if rainbow {
                    with_alpha(theme.rainbow(depth as usize), if active { 210 } else { 80 })
                } else if active {
                    theme.indent_guide_active
                } else {
                    theme.indent_guide
                };
                p.line_segment([pos2(x, y), pos2(x, y + h)], Stroke::new(if active && rainbow { 1.5 } else { 1.0 }, c));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_sticky(&self, ui: &Ui, p: &egui::Painter, rect: Rect, gutter_w: f32, first: usize, toks: &[Token], style: &EditorStyle) {
        let b = &self.doc.buffer;
        let probe = b.line_start(first.min(b.line_count().saturating_sub(1)));
        let mut top = None;
        let mut q = probe;
        while let Some(o) = sexp::enclosing_open(toks, q) {
            top = Some(o);
            q = toks[o].start;
        }
        let Some(o) = top else { return };
        let line = b.line_of(toks[o].start);
        if line >= first {
            return;
        }
        let theme = style.theme;
        let h = self.geom.line_h;
        let r = Rect::from_min_size(rect.min, vec2(rect.width() - SCROLLBAR_W, h + 2.0));
        p.rect_filled(r, CornerRadius::ZERO, theme.editor_bg);
        p.hline(r.x_range(), r.bottom(), Stroke::new(1.0, Tokens::of(theme).island_border));
        // Soft shadow under the sticky line.
        for k in 0..6 {
            let a = (24 - k * 4) as u8;
            p.hline(r.x_range(), r.bottom() + 1.0 + k as f32, Stroke::new(1.0, Color32::from_black_alpha(if theme.dark { a } else { a / 3 })));
        }
        if let Some(g) = self.galleys.get(&line).cloned().or_else(|| Some(p.layout_job(self.line_job(line, style, toks)))) {
            let tp = p.with_clip_rect(Rect::from_min_max(pos2(rect.left() + gutter_w, r.top()), r.max));
            tp.galley(pos2(self.geom.text_left - self.scroll.x, r.top() + (h - g.size().y) / 2.0 + 1.0), g, theme.editor_fg);
        }
        if style.settings.show_line_numbers {
            p.text(pos2(rect.left() + gutter_w - 14.0, r.top() + h / 2.0 + 1.0), Align2::RIGHT_CENTER, format!("{}", line + 1), ui::mono(style.font_size * 0.86), theme.line_number);
        }
        let click = ui.interact(r, self.id.with("sticky"), Sense::click());
        if click.clicked() {
            // Jumping happens next frame through reveal.
            ui.ctx().data_mut(|d| d.insert_temp(self.id.with("sticky-jump"), toks[o].start));
        }
    }

    fn paint_scrollbar(&mut self, ui: &Ui, rect: Rect, total_h: f32, viewport_h: f32, style: &EditorStyle) {
        let theme = style.theme;
        if total_h <= viewport_h + 1.0 {
            return;
        }
        let track = Rect::from_min_max(pos2(rect.right() - SCROLLBAR_W, rect.top()), rect.max);
        let id = self.id.with("vscroll");
        let resp = ui.interact(track, id, Sense::click_and_drag());
        let hovered = resp.hovered() || resp.dragged();
        let ratio = viewport_h / total_h;
        let thumb_h = (track.height() * ratio).max(28.0);
        let max_scroll = total_h - viewport_h;
        let thumb_y = track.top() + (track.height() - thumb_h) * (self.scroll.y / max_scroll).clamp(0.0, 1.0);
        if resp.dragged() {
            let dy = resp.drag_delta().y;
            self.scroll.y = (self.scroll.y + dy / (track.height() - thumb_h) * max_scroll).clamp(0.0, max_scroll);
        } else if resp.clicked()
            && let Some(pp) = resp.interact_pointer_pos()
        {
            let f = ((pp.y - track.top() - thumb_h / 2.0) / (track.height() - thumb_h)).clamp(0.0, 1.0);
            self.scroll.y = f * max_scroll;
        }
        let p = ui.painter_at(rect);
        // Markers: diagnostics, search matches, cursor.
        let lines = self.doc.buffer.line_count().max(1) as f32;
        let content_lines_h = lines * self.geom.line_h;
        let to_y = |line: usize| track.top() + track.height() * (line as f32 * self.geom.line_h / content_lines_h.max(total_h));
        if self.find.open {
            for r in self.find.matches.iter().take(2000) {
                let y = to_y(self.doc.buffer.line_of(r.start));
                p.rect_filled(Rect::from_min_size(pos2(track.left() + 2.0, y), vec2(4.0, 2.0)), CornerRadius::ZERO, with_alpha(theme.text_accent, 180));
            }
        }
        for d in &self.deco.diagnostics {
            let y = to_y(self.doc.buffer.line_of(d.range.start));
            let c = if d.severity == Severity::Error { theme.error } else { theme.warning };
            p.rect_filled(Rect::from_min_size(pos2(track.right() - 6.0, y), vec2(4.0, 3.0)), CornerRadius::same(1), c);
        }
        let cy = to_y(self.doc.buffer.line_of(self.doc.buffer.selections.primary().head));
        p.hline(track.left() + 2.0..=track.right() - 2.0, cy, Stroke::new(1.5, with_alpha(theme.text, 70)));
        let thumb = Rect::from_min_size(pos2(track.left() + 3.0, thumb_y), vec2(SCROLLBAR_W - 6.0, thumb_h));
        let c = if hovered { theme.scrollbar_thumb_hover } else { theme.scrollbar_thumb };
        p.rect_filled(thumb, CornerRadius::same(4), c);
    }

    fn textual_occurrences(&mut self, pos: usize, first: usize, last: usize, toks: &[Token]) -> Vec<Range<usize>> {
        let Some((r, name)) = ({
            let i = lexer::token_index_at(toks, pos);
            let cand = [toks.get(i).filter(|t| t.start <= pos), i.checked_sub(1).and_then(|j| toks.get(j)).filter(|t| t.end == pos)];
            cand.into_iter().flatten().find(|t| t.kind == Kind::Symbol && t.class != Class::Keyword).map(|t| (t.start..t.end, self.doc.chars()[t.start..t.end].to_vec()))
        }) else {
            return Vec::new();
        };
        let lo = self.doc.buffer.line_start(first);
        let hi = self.doc.buffer.line_end(last);
        let chars = self.doc.chars();
        let mut out: Vec<Range<usize>> = toks[lexer::token_index_at(toks, lo)..]
            .iter()
            .take_while(|t| t.start <= hi)
            .filter(|t| t.kind == Kind::Symbol && chars[t.start..t.end] == name[..])
            .map(|t| t.start..t.end)
            .collect();
        if !out.contains(&r) {
            out.push(r);
        }
        out
    }

    // ================================================================= hover

    fn handle_hover(&mut self, ui: &Ui, resp: &Response, style: &EditorStyle, now: f64, toks: &[Token]) {
        let theme = style.theme;
        // Vim's popups sit over the text: no hover card on top of them.
        let vim_popup = self.vim.as_ref().is_some_and(|v| v.cmdline.is_some() || v.which_key().is_some());
        let Some(hp) = resp.hover_pos().filter(|_| !vim_popup) else {
            self.hover = None;
            return;
        };
        if hp.x < self.geom.text_left || hp.x > self.geom.rect.right() - SCROLLBAR_W || ui.ctx().is_being_dragged(self.id) {
            self.hover = None;
            return;
        }
        match self.hover {
            Some((p, _)) if (p - hp).length() < 3.0 => {}
            _ => self.hover = Some((hp, now)),
        }
        let (_, since) = self.hover.unwrap();
        let pos = self.pos_at(hp);
        // Is the pointer actually over a token?
        let tok = lexer::token_at(toks, pos).map(|(_, t)| *t).filter(|t| {
            let line = self.doc.buffer.line_of(t.start);
            let x0 = self.x_of(line, t.start - self.doc.buffer.line_start(line));
            let x1 = self.x_of(line, t.end.min(self.doc.buffer.line_end(line)) - self.doc.buffer.line_start(line));
            hp.x >= x0 - 1.0 && hp.x <= x1 + 1.0
        });

        // Cmd/Ctrl-hover: underline the symbol like a link.
        let cmd = ui.input(|i| i.modifiers.command);
        if cmd
            && let Some(t) = tok.filter(|t| t.kind == Kind::Symbol)
        {
            let line = self.doc.buffer.line_of(t.start);
            let ls = self.doc.buffer.line_start(line);
            let y = self.line_y(line) + self.geom.line_h * 0.5 + style.font_size * 0.55;
            ui.painter().hline(self.x_of(line, t.start - ls)..=self.x_of(line, t.end - ls), y, Stroke::new(1.0, theme.text_accent));
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        // DrRacket binding arrows.
        if style.settings.show_binding_arrows
            && let (Some(sem), Some(t)) = (&self.deco.semantic, tok)
        {
            let hit = |r: &Range<usize>| r.start <= t.start && t.end <= r.end.max(r.start + 1);
            let arrows: Vec<&Arrow> = sem.arrows.iter().filter(|a| hit(&a.from) || hit(&a.to)).take(64).collect();
            for a in arrows {
                self.paint_arrow(ui, a, theme);
            }
        }

        let dwell = now - since;
        if dwell < 0.45 {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(0.46 - dwell));
            return;
        }
        let Some(t) = tok else { return };
        if self.hover_sent != Some(t.start) && t.kind == Kind::Symbol {
            self.hover_sent = Some(t.start);
            self.actions.push(EditorAction::Hover(t.start));
        }
        // Gather card contents.
        let mut sections: Vec<(Color32, String)> = Vec::new();
        for d in &self.deco.diagnostics {
            if d.range.start <= t.end && t.start <= d.range.end.max(d.range.start + 1) {
                let c = if d.severity == Severity::Error { theme.error } else { theme.warning };
                sections.push((c, d.message.clone()));
            }
        }
        let mut doc_lines: Vec<String> = Vec::new();
        if let Some((r, lines)) = &self.deco.hover_doc
            && r.start == t.start
        {
            doc_lines = lines.clone();
        }
        if let Some(sem) = &self.deco.semantic {
            for (r, text) in &sem.tooltips {
                if r.start <= t.start && t.end <= r.end && !sections.iter().any(|s| &s.1 == text) {
                    sections.push((theme.text_muted, text.clone()));
                }
            }
        }
        if sections.is_empty() && doc_lines.is_empty() {
            return;
        }
        let line = self.doc.buffer.line_of(t.start);
        let anchor = pos2(self.x_of(line, t.start - self.doc.buffer.line_start(line)), self.line_y(line) + self.geom.line_h + 4.0);
        hover_card(ui, self.id.with("hover"), anchor, theme, &doc_lines, &sections, style.font_size);
    }

    fn paint_arrow(&self, ui: &Ui, a: &Arrow, theme: &Theme) {
        let center = |r: &Range<usize>| -> Option<Pos2> {
            let line = self.doc.buffer.line_of(r.start);
            let ls = self.doc.buffer.line_start(line);
            let x0 = self.x_of(line, r.start - ls);
            let x1 = self.x_of(line, r.end.min(self.doc.buffer.line_end(line)) - ls);
            Some(pos2((x0 + x1) / 2.0, self.line_y(line) + self.geom.line_h / 2.0))
        };
        let (Some(from), Some(to)) = (center(&a.from), center(&a.to)) else { return };
        let color = match a.kind {
            ArrowKind::Lexical => theme.text_accent,
            ArrowKind::Imported => theme.syntax.keyword.color,
            ArrowKind::Tail => theme.syntax.string.color,
        };
        let color = with_alpha(color, 200);
        let p = ui.painter();
        let dist = (to - from).length();
        let bend = (dist * 0.25).clamp(12.0, 60.0);
        let c1 = from + vec2(0.0, -bend);
        let c2 = to + vec2(0.0, -bend);
        let curve = egui::epaint::CubicBezierShape::from_points_stroke([from, c1, c2, to], false, Color32::TRANSPARENT, Stroke::new(1.5, color));
        p.add(curve);
        p.circle_filled(from, 3.0, color);
        // Arrowhead along the final tangent.
        let dir = (to - c2).normalized();
        let n = vec2(-dir.y, dir.x);
        let tip = to;
        p.add(Shape::convex_polygon(vec![tip, tip - dir * 7.0 + n * 3.5, tip - dir * 7.0 - n * 3.5], color, Stroke::NONE));
    }

    // ================================================================= input

    fn handle_mouse(&mut self, ui: &Ui, resp: &Response, style: &EditorStyle, _now: f64) {
        if let Some(pos) = ui.ctx().data_mut(|d| d.remove_temp::<usize>(self.id.with("sticky-jump"))) {
            self.doc.buffer.selections = Selections::single(Selection::caret(pos));
            self.pending_reveal = Some(self.doc.buffer.line_of(pos));
        }
        let mods = ui.input(|i| i.modifiers);
        let gutter_hit = |p: Pos2| p.x < self.geom.text_left - 4.0 && style.mode == EditorMode::Code;
        if resp.triple_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let pos = self.pos_at(p);
            let line = self.doc.buffer.line_of(pos);
            let end = if line + 1 < self.doc.buffer.line_count() { self.doc.buffer.line_start(line + 1) } else { self.doc.buffer.len() };
            self.doc.buffer.selections.set_primary(Selection::new(self.doc.buffer.line_start(line), end));
            return;
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && !gutter_hit(p)
        {
            let pos = self.pos_at(p);
            // Double-click a bracket selects the whole s-expression.
            let toks = self.doc.tokens().to_vec();
            if let Some((o, c)) = sexp::bracket_pair_at(&toks, pos) {
                self.doc.buffer.selections.set_primary(Selection::new(toks[o].start, toks[c].end));
            } else if let Some(r) = self.doc.word_at(pos) {
                self.doc.buffer.selections.set_primary(Selection::new(r.start, r.end));
                self.drag_anchor = Some((r.start, r.end));
                self.drag_unit_word = true;
            }
            return;
        }
        // Place the caret the moment the button goes down (a click only completes on release),
        // and snap it there: the smooth-caret glide is for typing, not for clicking.
        let pressed = resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed());
        // A press and release within one frame (a quick click on a slow frame) only shows up
        // as a click.
        let quick_click = resp.clicked() && !self.press_placed;
        if !pressed && !ui.input(|i| i.pointer.primary_down()) {
            self.press_placed = false;
        }
        if (pressed || quick_click)
            && let Some(p) = resp.interact_pointer_pos() {
                self.press_placed = pressed;
                if gutter_hit(p) {
                    return;
                }
                let pos = self.pos_at(p);
                self.completion = None;
                self.caret_anim = None;
                if mods.command {
                    self.doc.buffer.selections = Selections::single(Selection::caret(pos));
                    self.actions.push(EditorAction::GotoDefinition(pos));
                } else if mods.alt {
                    self.doc.buffer.selections.add(Selection::caret(pos));
                } else if mods.shift {
                    let a = self.doc.buffer.selections.primary().anchor;
                    self.doc.buffer.selections = Selections::single(Selection::new(a, pos));
                } else {
                    self.doc.buffer.selections = Selections::single(Selection::caret(pos));
                }
                self.drag_anchor = Some((pos, pos));
                self.drag_unit_word = false;
                self.goal_col = None;
                self.doc.buffer.break_undo_group();
            }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let (Some(p), Some((a0, a1))) = (resp.interact_pointer_pos(), self.drag_anchor)
        {
            let pos = self.pos_at(p);
            let (anchor, head) = if self.drag_unit_word {
                let w = self.doc.word_at(pos).unwrap_or(pos..pos);
                if pos < a0 { (a1, w.start) } else { (a0, w.end) }
            } else {
                (a0, pos)
            };
            let mut sels = self.doc.buffer.selections.clone();
            sels.list[sels.primary] = Selection::new(anchor, head);
            sels.normalize();
            self.doc.buffer.selections = sels;
            self.caret_anim = None;
            // Auto-scroll when dragging outside.
            let r = self.geom.rect;
            if p.y < r.top() {
                self.scroll.y -= (r.top() - p.y).min(60.0) * 0.5;
                ui.ctx().request_repaint();
            } else if p.y > r.bottom() {
                self.scroll.y += (p.y - r.bottom()).min(60.0) * 0.5;
                ui.ctx().request_repaint();
            }
        }
        if resp.drag_stopped() {
            self.drag_anchor = None;
        }
        // Context menu.
        let ro = self.read_only;
        resp.context_menu(|ui| {
            ui.set_min_width(220.0);
            let pos = self.doc.buffer.selections.primary().head;
            if style.mode == EditorMode::Code {
                if ui.button("Go to Definition").clicked() {
                    self.actions.push(EditorAction::GotoDefinition(pos));
                    ui.close();
                }
                if !ro && ui.button("Rename Symbol").clicked() {
                    self.actions.push(EditorAction::Rename(pos));
                    ui.close();
                }
                if ui.button("Open Documentation").clicked() {
                    self.actions.push(EditorAction::OpenDocs(pos));
                    ui.close();
                }
                if ui.button("Run Selection in REPL").clicked() {
                    self.actions.push(EditorAction::RunSelection);
                    ui.close();
                }
                ui.separator();
            }
            if !ro && ui.button("Cut").clicked() {
                let t = self.doc.cut();
                ui.ctx().copy_text(t);
                ui.close();
            }
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(self.doc.copy_text().0);
                ui.close();
            }
            if !ro && ui.button("Toggle Comment").clicked() {
                self.doc.toggle_comment();
                ui.close();
            }
            if !ro && ui.button("Reindent").clicked() {
                self.doc.indent_selection_lines();
                ui.close();
            }
        });
    }

    fn handle_readonly_keys(&mut self, ui: &Ui) {
        let events = ui.input(|i| i.events.clone());
        for e in events {
            match e {
                Event::Copy => ui.ctx().copy_text(self.doc.copy_text().0),
                Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.command => self.doc.select_all(),
                _ => {}
            }
        }
    }

    fn handle_keys(&mut self, ui: &Ui, style: &EditorStyle) {
        let events = ui.input(|i| i.events.clone());
        let auto_close = style.settings.auto_close_brackets;
        let repl = style.mode == EditorMode::Repl;
        for e in events {
            if self.vim.is_some() && self.vim_event(ui, style, &e) {
                continue;
            }
            match e {
                Event::Text(t) => {
                    if t.chars().all(|c| c.is_control()) {
                        continue;
                    }
                    self.doc.type_text(&t, auto_close);
                    self.goal_col = None;
                    self.update_completion_after_typing(&t, style.settings.auto_complete);
                }
                Event::Ime(egui::ImeEvent::Commit(t)) => {
                    self.doc.type_text(&t, auto_close);
                }
                Event::Paste(t) => {
                    let t = self.doc.images.adopt_from_clipboard(&t);
                    self.doc.paste(&t);
                    self.completion = None;
                }
                // An image on the clipboard (a screenshot, an image copied from a browser):
                // paste it into the program, like DrRacket.
                Event::PasteImage(img) if style.mode == EditorMode::Code => {
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    if let Some(image) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba).and_then(crate::editor::images::EmbeddedImage::from_rgba) {
                        self.insert_image(image);
                    }
                }
                Event::Copy => {
                    let t = self.doc.copy_text().0;
                    self.doc.images.shelve_for_clipboard(&t);
                    ui.ctx().copy_text(t);
                }
                Event::Cut => {
                    let t = self.doc.copy_text().0;
                    self.doc.images.shelve_for_clipboard(&t);
                    let t = self.doc.cut();
                    ui.ctx().copy_text(t);
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if self.completion_key(key, modifiers) {
                        continue;
                    }
                    self.key(key, modifiers, repl, style);
                }
                _ => {}
            }
        }
    }

    /// Vim mode's overlays: a mode pill (with pending keys, macro recording and messages) at the
    /// bottom left, the which-key list above it after the leader, and the command menu.
    fn paint_vim(&self, ui: &Ui, rect: Rect, style: &EditorStyle, gutter_w: f32, focused: bool) {
        use crate::editor::vim::Mode;
        let Some(v) = self.vim.as_ref() else { return };
        let theme = style.theme;
        let tk = Tokens::of(theme);
        let p = ui.painter_at(rect);
        let shadow = egui::epaint::Shadow { offset: [0, 6], blur: 18, spread: 0, color: with_alpha(Color32::BLACK, if theme.dark { 110 } else { 40 }) };

        // Mode pill.
        let (label, color) = match v.mode {
            Mode::Normal if v.one_shot() => ("(INSERT)", theme.success),
            Mode::Normal => ("NORMAL", theme.text_accent),
            Mode::Insert => ("INSERT", theme.success),
            Mode::Replace => ("REPLACE", theme.error),
            Mode::Visual => ("VISUAL", theme.syntax.keyword.color),
            Mode::VisualLine => ("V-LINE", theme.syntax.keyword.color),
        };
        let pill_font = ui::semibold(11.0);
        let g = p.layout_no_wrap(label.to_string(), pill_font, if theme.dark { theme.editor_bg } else { Color32::WHITE });
        let base = pos2(rect.left() + gutter_w.max(10.0) + 6.0, rect.bottom() - 12.0);
        let pill = Rect::from_min_size(pos2(base.x, base.y - 20.0), vec2(g.size().x + 16.0, 20.0));
        let alpha = if focused { 255 } else { 150 };
        p.rect_filled(pill, CornerRadius::same(10), with_alpha(color, alpha));
        p.galley(pos2(pill.left() + 8.0, pill.center().y - g.size().y / 2.0), g, theme.editor_bg);
        let mut x = pill.right() + 8.0;
        let extra = |p: &egui::Painter, x: &mut f32, text: &str, c: Color32, mono: bool| {
            if text.is_empty() {
                return;
            }
            let font = if mono { ui::mono(12.5) } else { ui::ui_font(12.5) };
            let g = p.layout_no_wrap(text.to_string(), font, c);
            let bg = Rect::from_min_size(pos2(*x, pill.top()), vec2(g.size().x + 14.0, pill.height()));
            p.rect_filled(bg, CornerRadius::same(10), with_alpha(tk.popup, 235));
            p.galley(pos2(bg.left() + 7.0, bg.center().y - g.size().y / 2.0), g, c);
            *x = bg.right() + 6.0;
        };
        if let Some(r) = v.recording_into() {
            extra(&p, &mut x, &format!("recording @{r}"), theme.error, false);
        }
        extra(&p, &mut x, &v.pending(), theme.text, true);
        if let Some(m) = &v.message {
            let c = if m.starts_with('E') { theme.error } else { theme.text_muted };
            extra(&p, &mut x, m, c, false);
        }

        // Which-key: what can follow the leader.
        if let Some((typed, items)) = v.which_key().filter(|(_, i)| !i.is_empty()) {
            let w = (rect.width() - 24.0).min(760.0);
            let col_w = 190.0f32;
            let cols = ((w - 24.0) / col_w).floor().max(1.0) as usize;
            let rows = items.len().div_ceil(cols);
            let h = 40.0 + rows as f32 * 24.0 + 10.0;
            let panel = Rect::from_min_size(pos2(rect.center().x - w / 2.0, pill.top() - 10.0 - h), vec2(w, h));
            p.add(shadow.as_shape(panel, CornerRadius::same(12)));
            p.rect(panel, CornerRadius::same(12), tk.popup, Stroke::new(1.0, tk.subtle_border), StrokeKind::Inside);
            let title = if typed.is_empty() { "Space".to_string() } else { format!("Space {}", typed.chars().map(String::from).collect::<Vec<_>>().join(" ")) };
            p.text(pos2(panel.left() + 16.0, panel.top() + 20.0), Align2::LEFT_CENTER, title, ui::semibold(12.5), theme.text_muted);
            for (k, (key, desc, group)) in items.iter().enumerate() {
                let (c, r) = (k / rows, k % rows);
                let at = pos2(panel.left() + 16.0 + c as f32 * col_w, panel.top() + 44.0 + r as f32 * 24.0);
                let shown = if key == " " { "␣" } else { key.as_str() };
                let kr = Rect::from_min_size(pos2(at.x, at.y - 9.0), vec2(22.0, 18.0));
                p.rect_filled(kr, CornerRadius::same(5), tk.hover);
                p.text(kr.center(), Align2::CENTER_CENTER, shown, ui::mono(12.5), theme.text);
                p.text(pos2(kr.right() + 8.0, at.y), Align2::LEFT_CENTER, *desc, ui::ui_font(13.0), if *group { theme.text_accent } else { theme.text });
            }
        }

        // The command line: `:` as a menu of commands, `/` and `?` as a search box.
        if let Some((kind, text)) = &v.cmdline {
            let w = (rect.width() - 40.0).min(560.0);
            let suggestions = v.ex_suggestions();
            let shown = suggestions.len().min(8);
            let row_h = 30.0;
            let h = 44.0 + if shown > 0 { 8.0 + shown as f32 * row_h + 6.0 } else { 0.0 };
            let panel = Rect::from_min_size(pos2(rect.center().x - w / 2.0, rect.top() + (rect.height() * 0.12).max(24.0)), vec2(w, h));
            p.add(shadow.as_shape(panel, CornerRadius::same(12)));
            p.rect(panel, CornerRadius::same(12), tk.popup, Stroke::new(1.0, with_alpha(theme.text_accent, 140)), StrokeKind::Inside);
            let input = Rect::from_min_size(panel.min, vec2(w, 44.0));
            let icon = match kind {
                ':' => ic::TERMINAL,
                _ => ic::SEARCH,
            };
            p.text(pos2(input.left() + 18.0, input.center().y), Align2::CENTER_CENTER, icon, ui::icon_font(17.0), theme.text_accent);
            let mut job = LayoutJob::default();
            job.append(&kind.to_string(), 0.0, TextFormat { font_id: ui::mono(14.5), color: theme.text_accent, ..Default::default() });
            job.append(text, 0.0, TextFormat { font_id: ui::mono(14.5), color: theme.text, ..Default::default() });
            let g = p.layout_job(job);
            let tx = input.left() + 36.0;
            p.galley(pos2(tx, input.center().y - g.size().y / 2.0), g.clone(), theme.text);
            let caret_x = tx + g.size().x + 1.0;
            p.rect_filled(Rect::from_min_size(pos2(caret_x, input.center().y - 9.0), vec2(2.0, 18.0)), CornerRadius::same(1), theme.cursor);
            if text.is_empty() {
                let hint = match kind {
                    ':' => "w saves, q closes, %s/old/new/g, a line number, or any command",
                    '/' => "search forward",
                    _ => "search backward",
                };
                p.text(pos2(caret_x + 8.0, input.center().y), Align2::LEFT_CENTER, hint, ui::ui_font(13.0), theme.text_placeholder);
            }
            if shown > 0 {
                p.hline(panel.x_range().shrink(1.0), input.bottom(), Stroke::new(1.0, tk.subtle_border));
                for (k, (cmd, desc)) in suggestions.iter().take(shown).enumerate() {
                    let r = Rect::from_min_size(pos2(panel.left() + 6.0, input.bottom() + 6.0 + k as f32 * row_h), vec2(w - 12.0, row_h));
                    if v.ex_selected == Some(k) {
                        p.rect_filled(r, CornerRadius::same(7), with_alpha(theme.text_accent, 40));
                    }
                    p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, format!(":{cmd}"), ui::mono(13.5), theme.text);
                    p.text(pos2(r.left() + 140.0, r.center().y), Align2::LEFT_CENTER, *desc, ui::ui_font(13.0), theme.text_muted);
                }
            }
        }
    }

    /// Vim mode's handling of one input event; false leaves it to the normal editor (typing
    /// in insert mode, ⌘ shortcuts, clipboard events).
    fn vim_event(&mut self, ui: &Ui, style: &EditorStyle, e: &Event) -> bool {
        use crate::editor::vim::{BACKSPACE, ENTER, ESC, Mode, TAB, ctrl};
        let Some(v) = self.vim.as_ref() else { return false };
        let typing = matches!(v.mode, Mode::Insert | Mode::Replace) && v.cmdline.is_none() && !v.awaits_key();
        match e {
            Event::Text(t) => {
                if t.chars().all(|c| c.is_control()) {
                    return true;
                }
                if !typing {
                    for c in t.chars() {
                        self.vim_feed(ui, style, c);
                    }
                    return true;
                }
                let replace = v.mode == Mode::Replace;
                for c in t.chars() {
                    if replace && let Some(v) = self.vim.as_ref() {
                        v.overwrite_next(&mut self.doc);
                    }
                    self.doc.type_text(&c.to_string(), style.settings.auto_close_brackets);
                    if let Some(v) = self.vim.as_mut() {
                        v.note_insert(c);
                    }
                }
                self.goal_col = None;
                self.update_completion_after_typing(t, style.settings.auto_complete);
                true
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                let (key, m) = (*key, *modifiers);
                if m.command || m.mac_cmd {
                    return false;
                }
                let ctrl_key = m.ctrl;
                if typing {
                    if key == Key::Escape || (ctrl_key && matches!(key, Key::OpenBracket | Key::C)) {
                        self.completion = None;
                        self.vim_feed(ui, style, ESC);
                        return true;
                    }
                    // Insert mode's control keys: ^W ^U ^H delete, ^T ^D shift, ^R pastes a
                    // register, ^E ^Y copy from below/above, ^O runs one normal command.
                    let insert_ctrl = [(Key::W, 'w'), (Key::U, 'u'), (Key::H, 'h'), (Key::T, 't'), (Key::D, 'd'), (Key::R, 'r'), (Key::E, 'e'), (Key::Y, 'y'), (Key::O, 'o'), (Key::J, 'j')];
                    if ctrl_key
                        && !m.shift
                        && let Some((_, c)) = insert_ctrl.iter().find(|(k, _)| *k == key)
                    {
                        self.completion = None;
                        self.vim_feed(ui, style, ctrl(*c));
                        return true;
                    }
                    // Typed by the editor below; noted for `.` and macros (not when Enter or Tab
                    // accept a completion).
                    let noted = match key {
                        Key::Enter | Key::Tab if self.completion.is_some() => None,
                        Key::Enter => Some(ENTER),
                        Key::Backspace => Some(BACKSPACE),
                        Key::Tab => Some(TAB),
                        _ => None,
                    };
                    if let (Some(k), Some(v)) = (noted, self.vim.as_mut()) {
                        v.note_insert(k);
                    }
                    return false;
                }
                let k = if ctrl_key {
                    match key {
                        Key::OpenBracket | Key::C => Some(ESC),
                        k => {
                            let n = k.name();
                            (n.len() == 1 && n.chars().all(|c| c.is_ascii_alphabetic())).then(|| ctrl(n.chars().next().unwrap_or('a').to_ascii_lowercase()))
                        }
                    }
                } else {
                    match key {
                        Key::Escape => Some(ESC),
                        Key::Enter => Some(ENTER),
                        Key::Backspace => Some(BACKSPACE),
                        Key::Tab => Some(TAB),
                        Key::ArrowLeft => Some('h'),
                        Key::ArrowRight => Some('l'),
                        Key::ArrowUp => Some('k'),
                        Key::ArrowDown => Some('j'),
                        Key::Delete => Some('x'),
                        Key::Home => Some('0'),
                        Key::End => Some('$'),
                        Key::PageUp => Some(ctrl('b')),
                        Key::PageDown => Some(ctrl('f')),
                        _ => None,
                    }
                };
                if let Some(k) = k {
                    self.vim_feed(ui, style, k);
                }
                // Printable keys arrive as text; nothing else types in normal mode.
                true
            }
            _ => false,
        }
    }

    /// Send a key to Vim and carry out what it asks for.
    fn vim_feed(&mut self, ui: &Ui, style: &EditorStyle, k: char) {
        use crate::editor::vim::{Effect, View};
        let line_h = self.geom.line_h.max(1.0);
        let first = (self.scroll.y / line_h).floor() as usize;
        let rows = ((self.geom.rect.height() / line_h) as usize).max(2);
        let last = (first + rows - 2).min(self.doc.buffer.line_count().saturating_sub(1));
        let clipboard = || arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
        let view = View { first_line: first, last_line: last, auto_close: style.settings.auto_close_brackets, clipboard: &clipboard };
        let Some(vim) = self.vim.as_mut() else { return };
        let effects = vim.feed(&mut self.doc, &view, k);
        self.goal_col = None;
        if !matches!(vim.mode, crate::editor::vim::Mode::Insert | crate::editor::vim::Mode::Replace) {
            self.completion = None;
        }
        let head = self.doc.buffer.selections.primary().head;
        let mut saving = false;
        for e in effects {
            match e {
                Effect::Clipboard(t) => {
                    self.doc.images.shelve_for_clipboard(&t);
                    ui.ctx().copy_text(t);
                }
                Effect::Scroll(s) => self.vim_scroll = Some(s),
                Effect::Close { force: false } if self.doc.buffer.is_dirty() && !saving => {
                    if let Some(v) = self.vim.as_mut() {
                        v.message = Some("E37: No write since last change (add ! to override)".into());
                    }
                }
                Effect::GotoDefinition => self.actions.push(EditorAction::GotoDefinition(head)),
                Effect::ShowDocs => self.actions.push(EditorAction::OpenDocs(head)),
                Effect::Rename => self.actions.push(EditorAction::Rename(head)),
                other => {
                    saving |= other == Effect::Save;
                    self.actions.push(EditorAction::Vim(other));
                }
            }
        }
        self.scroll_to_cursor = true;
    }

    fn vertical(&mut self, lines: isize, extend: bool) {
        let mut sels = self.doc.buffer.selections.clone();
        let goal = *self.goal_col.get_or_insert_with(|| self.doc.buffer.col_of(sels.primary().head));
        let n = self.doc.buffer.line_count() as isize;
        for s in sels.list.iter_mut() {
            let line = self.doc.buffer.line_of(s.head) as isize;
            let target = line + lines;
            let pos = if target < 0 {
                0
            } else if target >= n {
                self.doc.buffer.len()
            } else {
                let t = target as usize;
                self.doc.buffer.line_start(t) + goal.min(self.doc.buffer.line_len(t))
            };
            *s = if extend { Selection::new(s.anchor, pos) } else { Selection::caret(pos) };
        }
        sels.normalize();
        self.doc.buffer.selections = sels;
        self.doc.buffer.break_undo_group();
    }

    fn key(&mut self, key: Key, m: Modifiers, repl: bool, style: &EditorStyle) {
        let shift = m.shift;
        let cmd = m.command;
        let alt = m.alt;
        let ctrl = m.ctrl && !m.mac_cmd;
        let page = ((self.geom.rect.height() / self.geom.line_h) as isize - 2).max(1);
        let keep_goal = matches!(key, Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown);
        match key {
            Key::ArrowLeft | Key::ArrowRight => {
                let left = key == Key::ArrowLeft;
                let motion = if cmd {
                    if left { Motion::LineStart } else { Motion::LineEnd }
                } else if alt && ctrl {
                    if left { Motion::SexpLeft } else { Motion::SexpRight }
                } else if alt && shift && !ctrl {
                    if left {
                        self.doc.shrink_selection();
                    } else {
                        self.doc.expand_selection();
                    }
                    return;
                } else if alt {
                    if left { Motion::WordLeft } else { Motion::WordRight }
                } else if left {
                    Motion::Left
                } else {
                    Motion::Right
                };
                self.doc.move_selections(motion, shift);
            }
            Key::ArrowUp | Key::ArrowDown => {
                let down = key == Key::ArrowDown;
                if repl && !shift && !alt && (cmd || ctrl || self.on_edge_line(down)) {
                    self.actions.push(if down { EditorAction::HistoryNext } else { EditorAction::HistoryPrev });
                    return;
                }
                if cmd && alt {
                    self.doc.add_cursor_vertical(down);
                } else if alt && shift {
                    self.doc.duplicate_lines(down);
                } else if alt {
                    self.doc.move_lines(down);
                } else if cmd {
                    self.doc.move_selections(if down { Motion::DocEnd } else { Motion::DocStart }, shift);
                } else if ctrl {
                    self.doc.move_selections(if down { Motion::DownSexp } else { Motion::UpSexp }, shift);
                } else {
                    self.vertical(if down { 1 } else { -1 }, shift);
                }
            }
            Key::PageUp => {
                self.vertical(-page, shift);
                self.scroll.y -= page as f32 * self.geom.line_h;
            }
            Key::PageDown => {
                self.vertical(page, shift);
                self.scroll.y += page as f32 * self.geom.line_h;
            }
            Key::Home => self.doc.move_selections(if cmd { Motion::DocStart } else { Motion::LineStart }, shift),
            Key::End => self.doc.move_selections(if cmd { Motion::DocEnd } else { Motion::LineEnd }, shift),
            Key::Backspace => {
                let unit = if cmd { Motion::LineStart } else if alt && ctrl { Motion::SexpLeft } else if alt { Motion::WordLeft } else { Motion::Left };
                self.doc.backspace(unit);
                self.refresh_completion();
            }
            Key::Delete => {
                let unit = if alt && ctrl { Motion::SexpRight } else if alt { Motion::WordRight } else if cmd { Motion::LineEnd } else { Motion::Right };
                self.doc.delete_forward(unit);
            }
            Key::Enter => {
                if repl && !shift && !alt && self.input_complete() {
                    self.actions.push(EditorAction::Submit);
                    return;
                }
                if cmd && shift {
                    self.doc.open_line(true);
                } else if cmd {
                    self.doc.open_line(false);
                } else {
                    self.doc.newline(true);
                }
            }
            Key::Tab => {
                if shift {
                    return;
                }
                self.doc.indent_selection_lines();
            }
            Key::Escape => {
                if self.find.open {
                    self.find.open = false;
                } else if self.doc.buffer.selections.list.len() > 1 {
                    let p = self.doc.buffer.selections.primary();
                    self.doc.buffer.selections = Selections::single(p);
                } else {
                    let p = self.doc.buffer.selections.primary();
                    self.doc.buffer.selections = Selections::single(Selection::caret(p.head));
                }
            }
            Key::A if cmd => self.doc.select_all(),
            Key::Z if cmd && shift => {
                self.doc.buffer.redo();
            }
            Key::Z if cmd => {
                self.doc.buffer.undo();
            }
            Key::Y if cmd => {
                self.doc.buffer.redo();
            }
            Key::D if cmd => self.doc.select_next_occurrence(),
            Key::L if cmd && shift => self.doc.select_all_occurrences(),
            Key::K if cmd && shift => self.doc.delete_lines(),
            Key::Slash if cmd && alt => self.doc.toggle_block_comment(),
            Key::Slash if cmd => self.doc.toggle_comment(),
            Key::Semicolon if cmd && alt => self.doc.toggle_datum_comment(),
            Key::I if cmd && !shift => self.doc.reindent_all(),
            Key::Backslash if cmd || ctrl => self.doc.type_text("λ", false),
            Key::Space if ctrl => self.open_completion(true),
            Key::F if cmd && alt => {
                self.open_find(true);
            }
            Key::F if cmd => self.open_find(false),
            Key::G if cmd => self.find_step(!shift),
            Key::F3 => self.find_step(!shift),
            Key::F12 => {
                let pos = self.doc.buffer.selections.primary().head;
                self.actions.push(EditorAction::GotoDefinition(pos));
            }
            Key::F2 => {
                let pos = self.doc.buffer.selections.primary().head;
                self.actions.push(EditorAction::Rename(pos));
            }
            Key::F1 => {
                let pos = self.doc.buffer.selections.primary().head;
                self.actions.push(EditorAction::OpenDocs(pos));
            }
            Key::K if ctrl && alt => self.doc.kill_sexp(),
            Key::T if ctrl && alt => self.doc.transpose_sexp(),
            _ => {}
        }
        if !keep_goal {
            self.goal_col = None;
        }
        let _ = style;
    }

    fn on_edge_line(&self, down: bool) -> bool {
        let b = &self.doc.buffer;
        let line = b.line_of(b.selections.primary().head);
        if down { line + 1 >= b.line_count() } else { line == 0 }
    }

    /// REPL input is complete when every bracket is closed and no string is open.
    pub fn input_complete(&mut self) -> bool {
        let text = self.doc.text();
        if text.trim().is_empty() {
            return false;
        }
        let toks = self.doc.tokens();
        let unbalanced = toks.iter().any(|t| t.is_open() && t.partner.is_none());
        let open_string = toks.last().is_some_and(|t| {
            matches!(t.kind, Kind::String | Kind::StringEscape) && !text.trim_end().ends_with('"') || t.kind == Kind::BlockComment && !text.trim_end().ends_with("|#")
        });
        let cursor_at_end = self.doc.buffer.selections.primary().head >= self.doc.buffer.len() - (self.doc.buffer.len() - text.trim_end().chars().count());
        !unbalanced && !open_string && cursor_at_end
    }

    // ================================================================= completion

    fn word_prefix(&mut self) -> Option<(usize, String)> {
        let head = self.doc.buffer.selections.primary().head;
        if self.doc.in_string_or_comment(head) {
            return None;
        }
        let chars = self.doc.chars();
        let mut s = head;
        while s > 0 && !lexer::is_delimiter(chars[s - 1]) && chars[s - 1] != '#' {
            s -= 1;
        }
        let prefix: String = chars[s..head].iter().collect();
        if prefix.is_empty() || prefix.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return None;
        }
        Some((s, prefix))
    }

    /// Keep an open completion list in sync with typing; open one automatically only when
    /// `auto` is on (otherwise Ctrl-Space opens it).
    fn update_completion_after_typing(&mut self, typed: &str, auto: bool) {
        let ident = typed.chars().all(|c| !lexer::is_delimiter(c));
        if !ident {
            self.completion = None;
            return;
        }
        if self.completion.is_some() {
            self.refresh_completion();
        } else if auto && self.word_prefix().is_some_and(|(_, p)| p.chars().count() >= 2) {
            self.open_completion(false);
        }
    }

    fn refresh_completion(&mut self) {
        if self.completion.is_some() {
            self.open_completion(false);
        }
    }

    fn open_completion(&mut self, explicit: bool) {
        let Some((anchor, prefix)) = self.word_prefix().or_else(|| explicit.then(|| (self.doc.buffer.selections.primary().head, String::new()))) else {
            self.completion = None;
            return;
        };
        // Identifiers in the document near the cursor, plus language words.
        let toks = self.doc.tokens_cached();
        let chars = self.doc.chars();
        let i = lexer::token_index_at(toks, anchor);
        let mut local: Vec<String> = toks[i.saturating_sub(4000)..(i + 4000).min(toks.len())]
            .iter()
            .filter(|t| t.kind == Kind::Symbol && t.start != anchor)
            .map(|t| chars[t.start..t.end].iter().collect())
            .collect();
        local.sort();
        local.dedup();
        let mut scored: Vec<(i64, String, bool)> = Vec::new();
        for (w, is_local) in local.iter().map(|w| (w, true)).chain(self.completion_words.iter().map(|w| (w, false))) {
            if w == &prefix {
                continue;
            }
            if let Some(score) = crate::fuzzy::score(&prefix, w) {
                scored.push((score + if is_local { 30 } else { 0 }, w.clone(), is_local));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.len().cmp(&b.1.len())).then(a.1.cmp(&b.1)));
        let mut seen = std::collections::HashSet::new();
        let items: Vec<(String, bool)> = scored.into_iter().filter(|(_, w, _)| seen.insert(w.clone())).take(60).map(|(_, w, l)| (w, l)).collect();
        if items.is_empty() {
            self.completion = None;
            return;
        }
        let selected = self.completion.as_ref().filter(|c| c.anchor == anchor).map(|c| c.selected.min(items.len() - 1)).unwrap_or(0);
        self.completion = Some(Completion { anchor, items, selected });
    }

    fn completion_key(&mut self, key: Key, m: Modifiers) -> bool {
        let Some(c) = &mut self.completion else { return false };
        match key {
            Key::ArrowDown if !m.any() => {
                c.selected = (c.selected + 1) % c.items.len();
                true
            }
            Key::ArrowUp if !m.any() => {
                c.selected = (c.selected + c.items.len() - 1) % c.items.len();
                true
            }
            Key::Enter | Key::Tab if !m.any() => {
                let (word, _) = c.items[c.selected].clone();
                let anchor = c.anchor;
                let head = self.doc.buffer.selections.primary().head;
                self.doc.buffer.selections = Selections::single(Selection::new(anchor, head));
                self.doc.paste(&word);
                self.completion = None;
                true
            }
            Key::Escape => {
                self.completion = None;
                true
            }
            _ => false,
        }
    }

    fn show_completion(&mut self, ui: &Ui, style: &EditorStyle) {
        let Some(c) = self.completion.clone() else { return };
        let theme = style.theme;
        let tk = Tokens::of(theme);
        let line = self.doc.buffer.line_of(c.anchor);
        let row_h = 26.0;
        let shown = c.items.len().min(10);
        let width = 300.0;
        // Below the line when it fits, otherwise above it: never over the line being typed.
        let screen = ui.ctx().content_rect();
        let height = shown as f32 * row_h + 10.0;
        let below = self.line_y(line) + self.geom.line_h + 2.0;
        let y = if below + height <= screen.bottom() - 4.0 { below } else { self.line_y(line) - 2.0 - height };
        let x = (self.x_of(line, c.anchor - self.doc.buffer.line_start(line)) - 28.0).min(screen.right() - width - 12.0);
        let mut chosen = None;
        egui::Area::new(self.id.with("completion")).order(egui::Order::Foreground).fixed_pos(pos2(x, y)).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).corner_radius(CornerRadius::same(8)).fill(tk.popup).show(ui, |ui| {
                ui.set_width(width);
                let start = c.selected.saturating_sub(shown - 1).min(c.items.len() - shown);
                for (k, (word, local)) in c.items.iter().enumerate().skip(start).take(shown) {
                    let (r, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
                    if k == c.selected {
                        ui.painter().rect_filled(r, CornerRadius::same(5), tk.selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(5), tk.hover);
                    }
                    let (glyph, color) = if *local { (ic::DATA_OBJECT, theme.syntax.variable.color) } else { (ic::FUNCTION, theme.syntax.function.color) };
                    ui.painter().text(pos2(r.left() + 14.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(15.0), color);
                    ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, word, ui::mono(style.font_size * 0.82), theme.text);
                    if resp.clicked() {
                        chosen = Some(k);
                    }
                }
            });
        });
        if let Some(k) = chosen {
            let word = c.items[k].0.clone();
            let head = self.doc.buffer.selections.primary().head;
            self.doc.buffer.selections = Selections::single(Selection::new(c.anchor, head));
            self.doc.paste(&word);
            self.completion = None;
        }
    }

    // ================================================================= find

    pub fn open_find(&mut self, replace: bool) {
        self.find.open = true;
        self.find.replace_open = replace;
        self.find.focus_query = true;
        let p = self.doc.buffer.selections.primary();
        if !p.is_empty() && self.doc.buffer.line_of(p.start()) == self.doc.buffer.line_of(p.end()) {
            self.find.query = self.doc.buffer.slice(p.range());
        }
        self.find.matches_key.0 = u64::MAX;
    }

    fn update_find_matches(&mut self) {
        let key = (self.doc.version(), self.find.query.clone(), self.find.case_sensitive, self.find.regex, self.find.whole_word);
        if key == self.find.matches_key {
            return;
        }
        self.find.matches_key = key;
        self.find.matches.clear();
        self.find.error = false;
        if self.find.query.is_empty() {
            return;
        }
        let text = self.doc.text();
        let pat = if self.find.regex { self.find.query.clone() } else { regex::escape(&self.find.query) };
        let pat = if self.find.whole_word { format!(r"(?:^|[\s()\[\]{{}}\x22,'`;])({pat})(?:$|[\s()\[\]{{}}\x22,'`;])") } else { format!("({pat})") };
        let re = regex::RegexBuilder::new(&pat).case_insensitive(!self.find.case_sensitive).multi_line(true).build();
        let Ok(re) = re else {
            self.find.error = true;
            return;
        };
        // Convert byte offsets to char offsets incrementally.
        let mut byte_to_char = 0usize;
        let mut last_byte = 0usize;
        for caps in re.captures_iter(&text).take(100_000) {
            let Some(m) = caps.get(1) else { continue };
            if m.start() == m.end() {
                continue;
            }
            byte_to_char += text[last_byte..m.start()].chars().count();
            let len = m.as_str().chars().count();
            self.find.matches.push(byte_to_char..byte_to_char + len);
            byte_to_char += len;
            last_byte = m.end();
        }
        let head = self.doc.buffer.selections.primary().start();
        self.find.current = self.find.matches.partition_point(|r| r.start < head).min(self.find.matches.len().saturating_sub(1));
    }

    pub fn find_step(&mut self, forward: bool) {
        if !self.find.open && self.find.query.is_empty() {
            return;
        }
        self.update_find_matches();
        let n = self.find.matches.len();
        if n == 0 {
            return;
        }
        let head = self.doc.buffer.selections.primary();
        self.find.current = if forward {
            self.find.matches.iter().position(|r| r.start > head.start()).unwrap_or(0)
        } else {
            self.find.matches.iter().rposition(|r| r.start < head.start()).unwrap_or(n - 1)
        };
        let r = self.find.matches[self.find.current].clone();
        self.doc.buffer.selections = Selections::single(Selection::new(r.start, r.end));
        self.pending_reveal = Some(self.doc.buffer.line_of(r.start));
    }

    fn replace_current(&mut self) {
        self.update_find_matches();
        let Some(r) = self.find.matches.get(self.find.current).cloned() else { return };
        self.doc.rename(&[r], &self.find.replacement.clone());
        self.find.matches_key.0 = u64::MAX;
        self.find_step(true);
    }

    fn replace_all(&mut self) {
        self.update_find_matches();
        let ranges = self.find.matches.clone();
        if ranges.is_empty() {
            return;
        }
        self.doc.rename(&ranges, &self.find.replacement.clone());
        self.find.matches_key.0 = u64::MAX;
    }

    fn show_find_bar(&mut self, ui: &mut Ui, rect: Rect, style: &EditorStyle) {
        let theme = style.theme;
        let tk = Tokens::of(theme);
        let w = 420.0_f32.min(rect.width() - 40.0);
        let h = if self.find.replace_open { 76.0 } else { 42.0 };
        let r = Rect::from_min_size(pos2(rect.right() - w - SCROLLBAR_W - 10.0, rect.top() + 8.0), vec2(w, h));
        let id = self.id.with("findbar");
        egui::Area::new(id).order(egui::Order::Middle).fixed_pos(r.min).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(10)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.set_width(w - 16.0);
                ui.horizontal(|ui| {
                    let toggle = |ui: &mut Ui, on: &mut bool, glyph: &str, tip: &str| {
                        if ui::widgets::icon_button_sized(ui, theme, glyph, tip, 24.0, 16.0, None, *on).clicked() {
                            *on = !*on;
                        }
                    };
                    let field_w = w - 16.0 - 24.0 * 6.0 - 64.0;
                    let qid = id.with("q");
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.find.query)
                            .id(qid)
                            .hint_text("Find")
                            .desired_width(field_w)
                            .font(egui::TextStyle::Body)
                            .text_color(if self.find.error { theme.error } else { theme.text }),
                    );
                    if self.find.focus_query {
                        resp.request_focus();
                        self.find.focus_query = false;
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        let back = ui.input(|i| i.modifiers.shift);
                        self.find_step(!back);
                        resp.request_focus();
                    }
                    if ui.input(|i| i.key_pressed(Key::Escape)) && (resp.has_focus() || resp.lost_focus()) {
                        self.find.open = false;
                        self.focus_request = true;
                    }
                    self.update_find_matches();
                    let count = if self.find.matches.is_empty() { "No results".to_string() } else { format!("{} of {}", self.find.current + 1, self.find.matches.len()) };
                    ui.label(egui::RichText::new(count).size(12.0).color(theme.text_muted));
                    toggle(ui, &mut self.find.case_sensitive, ic::TEXT_FIELDS, "Match Case");
                    toggle(ui, &mut self.find.whole_word, ic::SEGMENT, "Match Whole Word");
                    toggle(ui, &mut self.find.regex, ic::DATA_OBJECT, "Use Regular Expression");
                    if ui::widgets::icon_button_sized(ui, theme, ic::ARROW_UPWARD, &crate::platform::keys("Previous Match (⇧⌘G)"), 24.0, 16.0, None, false).clicked() {
                        self.find_step(false);
                    }
                    if ui::widgets::icon_button_sized(ui, theme, ic::ARROW_DOWNWARD, &crate::platform::keys("Next Match (⌘G)"), 24.0, 16.0, None, false).clicked() {
                        self.find_step(true);
                    }
                    if ui::widgets::icon_button_sized(ui, theme, ic::CLOSE, "Close (Esc)", 24.0, 16.0, None, false).clicked() {
                        self.find.open = false;
                        self.focus_request = true;
                    }
                });
                if self.find.replace_open {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let field_w = w - 16.0 - 24.0 * 6.0 - 64.0;
                        ui.add(egui::TextEdit::singleline(&mut self.find.replacement).hint_text("Replace").desired_width(field_w));
                        if ui.add(egui::Button::new("Replace").small()).clicked() {
                            self.replace_current();
                        }
                        if ui.add(egui::Button::new("All").small()).clicked() {
                            self.replace_all();
                        }
                    });
                }
            });
        });
    }
}

/// Floating documentation / info card.
pub fn hover_card(ui: &Ui, id: Id, anchor: Pos2, theme: &Theme, doc_lines: &[String], sections: &[(Color32, String)], font_size: f32) {
    let tk = Tokens::of(theme);
    egui::Area::new(id).order(egui::Order::Tooltip).fixed_pos(anchor).interactable(false).show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
            ui.set_max_width(560.0);
            if !doc_lines.is_empty() {
                let bb = mix(tk.popup, theme.text_accent, 0.08);
                egui::Frame::NONE.fill(bb).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
                    for l in doc_lines {
                        ui.label(egui::RichText::new(l).font(ui::mono((font_size * 0.8).round())).color(theme.text));
                    }
                });
                if !sections.is_empty() {
                    ui.add_space(6.0);
                }
            }
            for (c, text) in sections {
                ui.label(egui::RichText::new(text).color(*c).size(13.5));
            }
        });
    });
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Run one editor frame with the given input events.
    fn frame(ctx: &egui::Context, ed: &mut EditorView, style: &EditorStyle, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let rect = ui.max_rect();
            ed.show(ui, rect, style);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn scroll_lock_keeps_diagonal_swipes_vertical() {
        let mut lock = ScrollLock::default();
        // A vertical swipe that drifts sideways stays vertical...
        assert_eq!(lock.filter(Vec2::new(2.0, 10.0), 0.0), Vec2::new(0.0, 10.0));
        assert_eq!(lock.filter(Vec2::new(6.0, 8.0), 0.016), Vec2::new(0.0, 8.0));
        // ...until it turns clearly sideways.
        assert_eq!(lock.filter(Vec2::new(12.0, 2.0), 0.032), Vec2::new(12.0, 0.0));
        // A new gesture that starts sideways scrolls horizontally.
        let mut lock = ScrollLock::default();
        assert_eq!(lock.filter(Vec2::new(9.0, 3.0), 1.0), Vec2::new(9.0, 0.0));
        assert_eq!(lock.filter(Vec2::new(5.0, 4.0), 1.016), Vec2::new(5.0, 0.0));
    }

    fn key(k: Key) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }
    }

    /// Deleting the last line must not paint the stale line count (crashed in the indent
    /// highlight painter: "Attempt to index past end of Rope").
    #[test]
    fn deleting_last_line_in_the_same_frame_does_not_panic() {
        let themes = crate::theme::ThemeRegistry::load();
        let theme = themes.get(crate::theme::DEFAULT_THEME).unwrap().clone();
        let settings = crate::config::Settings::default();
        let style = EditorStyle { theme: &theme, settings: &settings, font_size: 14.0, mode: EditorMode::Code };
        let ctx = egui::Context::default();
        let mut ed = EditorView::new("(big-bang 0\n  [on-tick add1]\n  [stop-when stop?])\n  x\n", Id::unique("t"));
        ed.request_focus();
        frame(&ctx, &mut ed, &style, vec![]);
        let end = ed.doc.buffer.len();
        ed.doc.buffer.selections = crate::editor::buffer::Selections::single(crate::editor::buffer::Selection::caret(end));
        for _ in 0..6 {
            frame(&ctx, &mut ed, &style, vec![key(Key::Backspace)]);
        }
        assert!(ed.doc.buffer.line_count() < 5, "{:?}", ed.doc.text());
    }
}
