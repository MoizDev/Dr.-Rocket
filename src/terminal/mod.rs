//! Integrated terminal emulator widget.
//!
//! Emulation (VT parsing, grid, scrollback, selection) is alacritty_terminal; this module
//! owns the PTY lifecycle, input translation and an efficient egui renderer: cells with the
//! same style are merged into runs, backgrounds into rects, and each row's layout is cached
//! by a hash of its content so idle frames cost almost nothing.

mod keys;
mod render;

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{CursorShape, Processor, Rgb};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, CursorIcon, FontFamily, FontId, Galley, Key, Modifiers, PointerButton, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use parking_lot::Mutex;

use crate::theme::Theme;
use keys::{KeyModes, encode_key, encode_paste};
use render::{Palette, draw_special, is_special};

/// Font family used for bold terminal text, if the host registered it; otherwise bold
/// text uses the regular monospace face.
pub const BOLD_FONT_FAMILY: &str = "monospace-bold";

/// Inner padding between the widget edge and the cell grid.
const PADDING: Vec2 = vec2(10.0, 6.0);
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

/// State shared with the PTY thread's event listener.
struct Shared {
    ctx: egui::Context,
    title: Mutex<Option<String>>,
    exit_status: Mutex<Option<i32>>,
    sender: Mutex<Option<EventLoopSender>>,
    window_size: Mutex<WindowSize>,
    /// Theme colors by index (0..16 ansi, 256 fg, 257 bg, 258 cursor) for OSC color queries.
    palette: Mutex<HashMap<usize, Color32>>,
}

#[derive(Clone)]
struct Listener(Arc<Shared>);

impl Listener {
    fn write(&self, bytes: Vec<u8>) {
        if let Some(s) = self.0.sender.lock().as_ref() {
            let _ = s.send(Msg::Input(Cow::Owned(bytes)));
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        let shared = &self.0;
        match event {
            TermEvent::Wakeup | TermEvent::CursorBlinkingChange | TermEvent::MouseCursorDirty => shared.ctx.request_repaint(),
            TermEvent::Title(t) => {
                *shared.title.lock() = Some(t);
                shared.ctx.request_repaint();
            }
            TermEvent::ResetTitle => *shared.title.lock() = None,
            TermEvent::ClipboardStore(_, text) => shared.ctx.copy_text(text),
            TermEvent::ClipboardLoad(..) => {}
            TermEvent::PtyWrite(text) => self.write(text.into_bytes()),
            TermEvent::ColorRequest(index, format) => {
                let c = shared.palette.lock().get(&index).copied().unwrap_or(Color32::GRAY);
                self.write(format(Rgb { r: c.r(), g: c.g(), b: c.b() }).into_bytes());
            }
            TermEvent::TextAreaSizeRequest(format) => {
                let size = *shared.window_size.lock();
                self.write(format(size).into_bytes());
            }
            TermEvent::ChildExit(status) => {
                use std::os::unix::process::ExitStatusExt;
                let code = status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(1);
                *shared.exit_status.lock() = Some(code);
                shared.ctx.request_repaint();
            }
            TermEvent::Exit => {
                shared.exit_status.lock().get_or_insert(0);
                shared.ctx.request_repaint();
            }
            TermEvent::Bell => {}
        }
    }
}

/// Grid dimensions for `Term::new`/`Term::resize`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct GridSize {
    cols: usize,
    lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// One resolved cell, as needed for drawing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct CellView {
    c: char,
    fg: Color32,
    bg: Option<Color32>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    wide: bool,
    spacer: bool,
}

/// A cached, position-independent row layout (x offsets relative to the row origin).
struct RowLayout {
    bgs: Vec<(f32, f32, Color32)>,
    runs: Vec<(f32, Arc<Galley>)>,
    specials: Vec<(usize, char, Color32)>,
    last_used: u64,
}

struct Click {
    at: Instant,
    point: Point,
    count: u8,
}

pub struct TerminalView {
    term: Arc<FairMutex<Term<Listener>>>,
    shared: Arc<Shared>,
    listener: Listener,
    /// Channel to the PTY event loop; `None` until the child is spawned.
    sender: Option<EventLoopSender>,
    /// The child is spawned on the first layout, once the real grid size is known, so the
    /// shell never starts at a placeholder size and then gets resized mid-startup.
    pending_spawn: Option<(tty::Options, u64)>,
    /// Input written before the child exists.
    pending_input: Vec<u8>,
    id: egui::Id,
    program: String,
    size: GridSize,
    cell: Vec2,
    font_key: (u32, u32),
    /// Vertical offset of text within a cell (centers the glyph row in the taller cell).
    text_dy: f32,
    has_bold_family: bool,
    row_cache: HashMap<u64, RowLayout>,
    /// Font-atlas fill ratio last frame; a drop means egui reset the atlas, which
    /// invalidates every cached galley.
    last_atlas_fill: f32,
    frame: u64,
    focused: bool,
    blink_reset: Instant,
    scroll_accum: f32,
    suppress_text: bool,
    last_click: Option<Click>,
    selecting: bool,
    mouse_button_down: Option<u8>,
    last_mouse_cell: Option<(usize, usize)>,
    scrollbar_drag: Option<(f32, usize)>,
    last_scroll_at: Option<Instant>,
    exit_reported: bool,
    parser: Processor,
}

impl TerminalView {
    /// Spawn the user's login shell (`shell` override, else `$SHELL`, else the passwd shell)
    /// in `cwd`.
    pub fn spawn(cwd: &Path, shell: Option<&str>, ctx: &egui::Context) -> anyhow::Result<Self> {
        let (program, opts_shell) = match shell {
            Some(s) => (s.to_string(), Some(tty::Shell::new(s.to_string(), vec!["-l".to_string()]))),
            None => (std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()), None),
        };
        let name = Path::new(&program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(program);
        Self::start(cwd, opts_shell, name, ctx)
    }

    /// Spawn a specific program (e.g. `raco pkg install x`). The terminal stays open after
    /// the program exits and shows its exit status.
    pub fn spawn_command(cwd: &Path, program: &str, args: &[String], ctx: &egui::Context) -> anyhow::Result<Self> {
        if find_program(program).is_none() {
            anyhow::bail!("program not found: {program}");
        }
        let mut title = Path::new(program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| program.to_string());
        for a in args {
            title.push(' ');
            title.push_str(a);
        }
        Self::start(cwd, Some(tty::Shell::new(program.to_string(), args.to_vec())), title, ctx)
    }

    fn start(cwd: &Path, shell: Option<tty::Shell>, program: String, ctx: &egui::Context) -> anyhow::Result<Self> {
        if !cwd.is_dir() {
            anyhow::bail!("not a directory: {}", cwd.display());
        }
        let mut env = HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        env.insert("TERM_PROGRAM".to_string(), "DrRocket".to_string());
        let options = tty::Options { shell, working_directory: Some(cwd.to_path_buf()), drain_on_exit: true, env };

        let size = GridSize { cols: 80, lines: 24 };
        let window_size = WindowSize { num_lines: 24, num_cols: 80, cell_width: 9, cell_height: 18 };
        let shared = Arc::new(Shared {
            ctx: ctx.clone(),
            title: Mutex::new(None),
            exit_status: Mutex::new(None),
            sender: Mutex::new(None),
            window_size: Mutex::new(window_size),
            palette: Mutex::new(HashMap::new()),
        });
        let listener = Listener(shared.clone());
        let config = TermConfig { scrolling_history: 10_000, ..TermConfig::default() };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));

        let window_id = NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed);

        Ok(Self {
            term,
            shared,
            listener,
            sender: None,
            pending_spawn: Some((options, window_id)),
            pending_input: Vec::new(),
            id: egui::Id::unique(("drrocket-terminal", window_id)),
            program,
            size: GridSize { cols: 0, lines: 0 },
            cell: vec2(9.0, 18.0),
            font_key: (0, 0),
            text_dy: 0.0,
            has_bold_family: false,
            row_cache: HashMap::new(),
            last_atlas_fill: 0.0,
            frame: 0,
            focused: false,
            blink_reset: Instant::now(),
            scroll_accum: 0.0,
            suppress_text: false,
            last_click: None,
            selecting: false,
            mouse_button_down: None,
            last_mouse_cell: None,
            scrollbar_drag: None,
            last_scroll_at: None,
            exit_reported: false,
            parser: Processor::new(),
        })
    }

    /// The OSC title if the program set one, else the program name.
    pub fn title(&self) -> String {
        self.shared.title.lock().clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| self.program.clone())
    }

    /// Exit status once the child process exited.
    pub fn exited(&self) -> Option<i32> {
        *self.shared.exit_status.lock()
    }


    /// Request keyboard focus for the terminal.
    pub fn focus(&mut self, ui: &egui::Ui) {
        ui.memory_mut(|m| m.request_focus(self.id));
    }

    fn write(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() || self.exited().is_some() {
            return;
        }
        match &self.sender {
            Some(sender) => {
                let _ = sender.send(Msg::Input(Cow::Owned(bytes)));
            }
            None => self.pending_input.extend_from_slice(&bytes),
        }
    }

    /// Start the child process with the PTY already at the final grid size.
    fn spawn_child(&mut self, window_size: WindowSize) {
        let Some((options, window_id)) = self.pending_spawn.take() else { return };
        let result = tty::new(&options, window_size, window_id)
            .and_then(|pty| EventLoop::new(self.term.clone(), self.listener.clone(), pty, true, false));
        match result {
            Ok(event_loop) => {
                let sender = event_loop.channel();
                *self.shared.sender.lock() = Some(sender.clone());
                event_loop.spawn();
                let pending = std::mem::take(&mut self.pending_input);
                if !pending.is_empty() {
                    let _ = sender.send(Msg::Input(Cow::Owned(pending)));
                }
                self.sender = Some(sender);
            }
            Err(err) => {
                let msg = format!("\x1b[31mFailed to start {}: {err}\x1b[0m\r\n", self.program);
                let mut term = self.term.lock();
                self.parser.advance(&mut *term, msg.as_bytes());
                drop(term);
                *self.shared.exit_status.lock() = Some(127);
                self.exit_reported = true;
            }
        }
    }

    /// Typing snaps back to the live screen, like every terminal does.
    fn on_user_input(&mut self) {
        self.blink_reset = Instant::now();
        let mut term = self.term.lock();
        if term.grid().display_offset() != 0 {
            term.scroll_display(Scroll::Bottom);
        }
    }

    /// Draw the terminal and handle its input. Fills the available rect.
    pub fn ui(&mut self, ui: &mut egui::Ui, theme: &Theme, font: &FontId) -> egui::Response {
        self.frame += 1;
        let outer = ui.available_rect_before_wrap();
        let response = ui.interact(outer, self.id, Sense::click_and_drag());
        ui.advance_cursor_after_rect(outer);
        let painter = ui.painter_at(outer);
        let ppp = ui.ctx().pixels_per_point();
        let snap = |v: f32| (v * ppp).round() / ppp;

        self.update_metrics(ui, font);
        self.sync_palette(theme);

        // Resize the grid (and the PTY) to fit.
        let inner = outer.shrink2(PADDING);
        let cols = ((inner.width() / self.cell.x).floor() as usize).max(2);
        let lines = ((inner.height() / self.cell.y).floor() as usize).max(1);
        let new_size = GridSize { cols, lines };
        if new_size != self.size {
            self.size = new_size;
            self.term.lock().resize(new_size);
            let ws = WindowSize {
                num_lines: lines as u16,
                num_cols: cols as u16,
                cell_width: self.cell.x.round() as u16,
                cell_height: self.cell.y.round() as u16,
            };
            *self.shared.window_size.lock() = ws;
            if let Some(sender) = &self.sender {
                let _ = sender.send(Msg::Resize(ws));
            }
        }
        if self.pending_spawn.is_some() {
            let ws = *self.shared.window_size.lock();
            self.spawn_child(ws);
        }
        let origin = pos2(snap(inner.left()), snap(inner.top()));

        // Focus handling.
        if response.clicked() || response.drag_started() || response.secondary_clicked() {
            response.request_focus();
        }
        let has_focus = response.has_focus();
        if has_focus {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    self.id,
                    egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
                )
            });
        }
        if has_focus != self.focused {
            self.focused = has_focus;
            self.blink_reset = Instant::now();
            if self.term.lock().mode().contains(TermMode::FOCUS_IN_OUT) {
                self.write(if has_focus { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() });
            }
        }

        self.handle_mouse(ui, &response, origin);
        if has_focus {
            self.handle_keyboard(ui);
        }
        self.report_exit();

        // Snapshot the visible grid under the lock, then render without holding it.
        let palette_theme = &theme.terminal;
        let (rows, cursor, display_offset, history, mode) = {
            let term = self.term.lock();
            let content = term.renderable_content();
            let palette = Palette { theme: palette_theme, overrides: content.colors };
            let selection = content.selection;
            let display_offset = content.display_offset;
            let mut rows: Vec<Vec<CellView>> = vec![Vec::with_capacity(cols); lines];
            let sel_bg = theme.selection;
            for indexed in content.display_iter {
                let vline = (indexed.point.line.0 + display_offset as i32) as usize;
                if vline >= lines {
                    continue;
                }
                let cell = indexed.cell;
                let flags = cell.flags;
                let (mut fg, mut bg) = (cell.fg, cell.bg);
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let mut fg_c = if flags.contains(Flags::DIM) { palette.dim(fg) } else { palette.resolve(fg) };
                let mut bg_c = if !flags.contains(Flags::INVERSE) && palette.is_default_bg(bg) { None } else { Some(palette.resolve(bg)) };
                if flags.contains(Flags::HIDDEN) {
                    fg_c = bg_c.unwrap_or(palette_theme.background);
                }
                let selected = selection.is_some_and(|s| s.contains_cell(&indexed, content.cursor.point, content.cursor.shape));
                if selected {
                    let base = bg_c.unwrap_or(palette_theme.background);
                    bg_c = Some(blend_over(base, sel_bg));
                }
                let c = if cell.c == '\t' { ' ' } else { cell.c };
                rows[vline].push(CellView {
                    c,
                    fg: fg_c,
                    bg: bg_c,
                    bold: flags.contains(Flags::BOLD),
                    italic: flags.contains(Flags::ITALIC),
                    underline: flags.intersects(Flags::ALL_UNDERLINES),
                    strike: flags.contains(Flags::STRIKEOUT),
                    wide: flags.contains(Flags::WIDE_CHAR),
                    spacer: flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
                });
            }
            let cursor = content.cursor;
            let history = term.grid().history_size();
            let mode = *term.mode();
            (rows, cursor, display_offset, history, mode)
        };

        // Background.
        painter.rect_filled(outer, 0.0, theme.terminal.background);

        // Cached galleys point into the font atlas; drop them if egui reset it.
        let fill = ui.fonts(|f| f.font_atlas_fill_ratio());
        if fill < self.last_atlas_fill {
            self.row_cache.clear();
        }
        self.last_atlas_fill = fill;

        // Rows.
        let bold_family = if self.has_bold_family { FontFamily::Name(BOLD_FONT_FAMILY.into()) } else { font.family.clone() };
        for (i, row) in rows.iter().enumerate() {
            let y = origin.y + i as f32 * self.cell.y;
            let key = row_hash(row, self.font_key);
            let frame = self.frame;
            if !self.row_cache.contains_key(&key) {
                let layout = self.layout_row(ui, row, font, &bold_family);
                self.row_cache.insert(key, layout);
            }
            let layout = self.row_cache.get_mut(&key).expect("just inserted");
            layout.last_used = frame;
            for &(x0, x1, color) in &layout.bgs {
                painter.rect_filled(Rect::from_min_max(pos2(origin.x + x0, y), pos2(origin.x + x1, y + self.cell.y)), 0.0, color);
            }
            let text_y = y + self.text_dy;
            for (x, galley) in &layout.runs {
                painter.galley(pos2(snap(origin.x + x), snap(text_y)), galley.clone(), theme.terminal.foreground);
            }
            for &(col, c, color) in &layout.specials {
                let x = origin.x + col as f32 * self.cell.x;
                let cell = Rect::from_min_max(pos2(snap(x), snap(y)), pos2(snap(x + self.cell.x), snap(y + self.cell.y)));
                draw_special(&painter, cell, c, color, ppp);
            }
        }
        // Evict layouts that haven't been used for a while.
        if self.row_cache.len() > lines * 6 + 64 {
            let cutoff = self.frame.saturating_sub(2);
            self.row_cache.retain(|_, l| l.last_used >= cutoff);
        }

        // Cursor.
        let cursor_visible = mode.contains(TermMode::SHOW_CURSOR) && cursor.shape != CursorShape::Hidden && display_offset == 0;
        if cursor_visible {
            let vline = cursor.point.line.0 + display_offset as i32;
            if vline >= 0 && (vline as usize) < lines {
                let col = cursor.point.column.0;
                let wide = rows[vline as usize].get(col).is_some_and(|c| c.wide);
                let x = origin.x + col as f32 * self.cell.x;
                let y = origin.y + vline as f32 * self.cell.y;
                let w = if wide { self.cell.x * 2.0 } else { self.cell.x };
                let rect = Rect::from_min_size(pos2(snap(x), snap(y)), vec2(snap(w), snap(self.cell.y)));
                let blink_on = !has_focus || self.blink_phase();
                let color = theme.cursor;
                if !has_focus {
                    painter.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                } else if blink_on {
                    match cursor.shape {
                        CursorShape::Beam => {
                            painter.rect_filled(Rect::from_min_size(rect.min, vec2(snap(2.0f32.max(1.0 / ppp)), rect.height())), 0.0, color);
                        }
                        CursorShape::Underline => {
                            painter.rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), rect.max), 0.0, color);
                        }
                        CursorShape::HollowBlock => {
                            painter.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
                        }
                        _ => {
                            painter.rect_filled(rect, 0.0, color);
                            if let Some(cv) = rows[vline as usize].get(col)
                                && cv.c != ' '
                            {
                                let text_color = cv.bg.unwrap_or(theme.terminal.background);
                                if !draw_special(&painter, rect, cv.c, text_color, ppp) {
                                    let family = if cv.bold { bold_family.clone() } else { font.family.clone() };
                                    let galley = painter.layout_no_wrap(cv.c.to_string(), FontId::new(font.size, family), text_color);
                                    let text_y = y + self.text_dy;
                                    painter.galley(pos2(snap(x), snap(text_y)), galley, text_color);
                                }
                            }
                        }
                    }
                }
                if has_focus {
                    let elapsed = self.blink_reset.elapsed().as_millis() as u64;
                    let interval = BLINK_INTERVAL.as_millis() as u64;
                    let next = interval - (elapsed % interval);
                    ui.ctx().request_repaint_after(Duration::from_millis(next));
                }
                // Let the platform IME place its candidate window at the cursor.
                if has_focus {
                    ui.ctx().output_mut(|o| {
                        o.ime = Some(egui::output::IMEOutput {
                            purpose: egui::IMEPurpose::Terminal,
                            rect: outer,
                            cursor_rect: rect,
                            should_interrupt_composition: false,
                        });
                    });
                }
            }
        }

        self.draw_scrollbar(ui, &painter, outer, ScrollState { history, offset: display_offset, lines }, theme);

        if response.hovered() {
            let mouse_mode = mode.intersects(TermMode::MOUSE_MODE) && !ui.input(|i| i.modifiers.shift);
            ui.ctx().set_cursor_icon(if mouse_mode { CursorIcon::Default } else { CursorIcon::Text });
        }
        response
    }

    fn blink_phase(&self) -> bool {
        let n = self.blink_reset.elapsed().as_millis() / BLINK_INTERVAL.as_millis();
        n.is_multiple_of(2)
    }

    fn update_metrics(&mut self, ui: &egui::Ui, font: &FontId) {
        let key = (font.size.to_bits(), ui.ctx().pixels_per_point().to_bits());
        if key == self.font_key {
            return;
        }
        self.font_key = key;
        let ppp = ui.ctx().pixels_per_point();
        let (advance, row_h, families) = ui.fonts_mut(|f| (f.glyph_width(font, 'M'), f.row_height(font), f.families()));
        self.has_bold_family = families.iter().any(|f| *f == FontFamily::Name(BOLD_FONT_FAMILY.into()));
        // Keep cell sizes on whole physical pixels so rows/columns never drift.
        self.cell = vec2((advance * ppp).round() / ppp, ((row_h * 1.22) * ppp).round() / ppp);
        self.text_dy = (((self.cell.y - row_h) / 2.0).max(0.0) * ppp).round() / ppp;
        self.row_cache.clear();
    }

    fn sync_palette(&self, theme: &Theme) {
        let mut p = self.shared.palette.lock();
        if p.get(&257) == Some(&theme.terminal.background) && p.get(&1) == Some(&theme.terminal.ansi[1]) {
            return;
        }
        p.clear();
        for (i, c) in theme.terminal.ansi.iter().enumerate() {
            p.insert(i, *c);
        }
        p.insert(256, theme.terminal.foreground);
        p.insert(257, theme.terminal.background);
        p.insert(258, theme.cursor);
    }

    fn layout_row(&self, ui: &egui::Ui, row: &[CellView], font: &FontId, bold_family: &FontFamily) -> RowLayout {
        let cw = self.cell.x;
        // Merge backgrounds.
        let mut bgs: Vec<(f32, f32, Color32)> = Vec::new();
        for (col, cell) in row.iter().enumerate() {
            if let Some(bg) = cell.bg {
                let x0 = col as f32 * cw;
                let x1 = x0 + cw;
                match bgs.last_mut() {
                    Some(last) if last.2 == bg && (last.1 - x0).abs() < 0.01 => last.1 = x1,
                    _ => bgs.push((x0, x1, bg)),
                }
            }
        }

        // Text runs: ASCII cells with identical style become one galley (monospace advance
        // keeps them aligned); every other character is placed at its exact column.
        let mut runs: Vec<(f32, Arc<Galley>)> = Vec::new();
        let mut specials = Vec::new();
        let mut run: Option<(usize, String, CellView)> = None;
        let flush = |run: &mut Option<(usize, String, CellView)>, runs: &mut Vec<(f32, Arc<Galley>)>| {
            if let Some((col, text, style)) = run.take() {
                let trimmed = text.trim_end();
                if trimmed.is_empty() && !style.underline && !style.strike {
                    return;
                }
                let text = if style.underline || style.strike { text } else { trimmed.to_string() };
                let family = if style.bold { bold_family.clone() } else { font.family.clone() };
                let mut fmt = TextFormat::simple(FontId::new(font.size, family), style.fg);
                fmt.italics = style.italic;
                if style.underline {
                    fmt.underline = Stroke::new(1.0, style.fg);
                }
                if style.strike {
                    fmt.strikethrough = Stroke::new(1.0, style.fg);
                }
                let mut job = LayoutJob::single_section(text, fmt);
                job.wrap.max_width = f32::INFINITY;
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                runs.push((col as f32 * cw, galley));
            }
        };
        let same_style = |a: &CellView, b: &CellView| {
            a.fg == b.fg && a.bold == b.bold && a.italic == b.italic && a.underline == b.underline && a.strike == b.strike
        };
        for (col, cell) in row.iter().enumerate() {
            if cell.spacer {
                flush(&mut run, &mut runs);
                continue;
            }
            if is_special(cell.c) {
                flush(&mut run, &mut runs);
                specials.push((col, cell.c, cell.fg));
                continue;
            }
            let ascii = cell.c.is_ascii() && !cell.wide;
            if !ascii {
                flush(&mut run, &mut runs);
                run = Some((col, cell.c.to_string(), *cell));
                flush(&mut run, &mut runs);
                continue;
            }
            match &mut run {
                Some((_, text, style)) if same_style(style, cell) || (cell.c == ' ' && !cell.underline && !cell.strike && !style.underline && !style.strike) => {
                    text.push(cell.c)
                }
                _ => {
                    flush(&mut run, &mut runs);
                    if cell.c != ' ' || cell.underline || cell.strike {
                        run = Some((col, cell.c.to_string(), *cell));
                    }
                }
            }
        }
        flush(&mut run, &mut runs);
        RowLayout { bgs, runs, specials, last_used: self.frame }
    }

    fn draw_scrollbar(&mut self, ui: &egui::Ui, painter: &egui::Painter, outer: Rect, scroll: ScrollState, theme: &Theme) {
        let ScrollState { history, offset, lines } = scroll;
        if history == 0 {
            return;
        }
        let total = (history + lines) as f32;
        let track = Rect::from_min_max(pos2(outer.right() - 8.0, outer.top() + 2.0), pos2(outer.right() - 2.0, outer.bottom() - 2.0));
        let thumb_h = (track.height() * lines as f32 / total).max(24.0).min(track.height());
        let frac = 1.0 - offset as f32 / history as f32; // 1.0 = bottom
        let thumb_top = track.top() + (track.height() - thumb_h) * frac;
        let thumb = Rect::from_min_size(pos2(track.left(), thumb_top), vec2(track.width(), thumb_h));

        let id = self.id.with("scrollbar");
        let resp = ui.interact(track.expand2(vec2(3.0, 0.0)), id, Sense::drag());
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.scrollbar_drag = Some((p.y, offset));
        }
        if resp.dragged()
            && let (Some((y0, off0)), Some(p)) = (self.scrollbar_drag, resp.interact_pointer_pos())
        {
            let per_px = history as f32 / (track.height() - thumb_h).max(1.0);
            let new = (off0 as f32 - (p.y - y0) * per_px).round().clamp(0.0, history as f32) as usize;
            let mut term = self.term.lock();
            let cur = term.grid().display_offset() as i32;
            term.scroll_display(Scroll::Delta(new as i32 - cur));
            self.last_scroll_at = Some(Instant::now());
        }
        if resp.drag_stopped() {
            self.scrollbar_drag = None;
        }

        let recently = self.last_scroll_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1200));
        let show = offset > 0 || resp.hovered() || resp.dragged() || recently;
        if !show {
            return;
        }
        if recently {
            ui.ctx().request_repaint_after(Duration::from_millis(1250));
        }
        let color = if resp.hovered() || resp.dragged() { theme.scrollbar_thumb_hover } else { theme.scrollbar_thumb };
        painter.rect_filled(thumb, 3.0, color);
    }

    /// Convert a screen position to a grid point (accounting for scrollback) and side.
    fn point_at(&self, origin: Pos2, pos: Pos2, display_offset: usize) -> (Point, Side, usize, usize) {
        let rel = pos - origin;
        let col_f = (rel.x / self.cell.x).max(0.0);
        let col = (col_f as usize).min(self.size.cols - 1);
        let row = ((rel.y / self.cell.y).max(0.0) as usize).min(self.size.lines - 1);
        let side = if col_f.fract() < 0.5 { Side::Left } else { Side::Right };
        let point = Point::new(Line(row as i32 - display_offset as i32), Column(col));
        (point, side, col, row)
    }

    fn handle_mouse(&mut self, ui: &egui::Ui, response: &egui::Response, origin: Pos2) {
        let (events, modifiers) = ui.input(|i| (i.events.clone(), i.modifiers));
        let (mode, mut display_offset) = {
            let t = self.term.lock();
            (*t.mode(), t.grid().display_offset())
        };
        let mouse_mode = mode.intersects(TermMode::MOUSE_MODE) && !modifiers.shift;
        let hovered = response.hovered() || response.dragged() || self.mouse_button_down.is_some();
        let interact_rect = response.rect;

        for event in &events {
            match event {
                egui::Event::MouseWheel { unit, delta, modifiers, .. } if response.hovered() => {
                    let lines_delta = match unit {
                        egui::MouseWheelUnit::Point => delta.y / self.cell.y,
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Page => delta.y * self.size.lines as f32,
                    };
                    self.scroll_accum += lines_delta;
                    let whole = self.scroll_accum.trunc() as i32;
                    if whole == 0 {
                        continue;
                    }
                    self.scroll_accum -= whole as f32;
                    if mouse_mode {
                        if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                            let (_, _, col, row) = self.point_at(origin, pos, display_offset);
                            let button = if whole > 0 { 64 } else { 65 };
                            for _ in 0..whole.unsigned_abs().min(10) {
                                self.mouse_report(mode, button, col, row, true, *modifiers);
                            }
                        }
                    } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
                        let app = mode.contains(TermMode::APP_CURSOR);
                        let seq: &[u8] = match (whole > 0, app) {
                            (true, true) => b"\x1bOA",
                            (true, false) => b"\x1b[A",
                            (false, true) => b"\x1bOB",
                            (false, false) => b"\x1b[B",
                        };
                        let mut bytes = Vec::new();
                        for _ in 0..whole.unsigned_abs() {
                            bytes.extend_from_slice(seq);
                        }
                        self.write(bytes);
                    } else {
                        let mut term = self.term.lock();
                        term.scroll_display(Scroll::Delta(whole));
                        display_offset = term.grid().display_offset();
                        self.last_scroll_at = Some(Instant::now());
                    }
                }
                egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                    let inside = interact_rect.contains(*pos);
                    let code = match button {
                        PointerButton::Primary => 0,
                        PointerButton::Middle => 1,
                        PointerButton::Secondary => 2,
                        _ => continue,
                    };
                    if *pressed && !inside {
                        continue;
                    }
                    if !*pressed && self.mouse_button_down.is_none() && !self.selecting {
                        continue;
                    }
                    let (point, side, col, row) = self.point_at(origin, *pos, display_offset);
                    if mouse_mode {
                        self.mouse_report(mode, code, col, row, *pressed, *modifiers);
                        self.mouse_button_down = if *pressed { Some(code) } else { None };
                        continue;
                    }
                    if code != 0 {
                        continue;
                    }
                    if *pressed {
                        let count = match &self.last_click {
                            Some(c) if c.at.elapsed() < Duration::from_millis(400) && c.point == point => c.count % 3 + 1,
                            _ => 1,
                        };
                        self.last_click = Some(Click { at: Instant::now(), point, count });
                        let ty = match count {
                            1 => SelectionType::Simple,
                            2 => SelectionType::Semantic,
                            _ => SelectionType::Lines,
                        };
                        let mut term = self.term.lock();
                        if count == 1 && modifiers.shift && term.selection.is_some() {
                            if let Some(sel) = term.selection.as_mut() {
                                sel.update(point, side);
                            }
                        } else {
                            term.selection = Some(Selection::new(ty, point, side));
                        }
                        self.selecting = true;
                    } else {
                        self.selecting = false;
                        let mut term = self.term.lock();
                        if term.selection.as_ref().is_some_and(|s| s.is_empty()) {
                            term.selection = None;
                        }
                    }
                }
                egui::Event::PointerMoved(pos) if hovered => {
                    let (point, side, col, row) = self.point_at(origin, *pos, display_offset);
                    if mouse_mode {
                        let moved = self.last_mouse_cell != Some((col, row));
                        self.last_mouse_cell = Some((col, row));
                        if moved {
                            if let Some(b) = self.mouse_button_down {
                                if mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) {
                                    self.mouse_report(mode, 32 + b, col, row, true, modifiers);
                                }
                            } else if mode.contains(TermMode::MOUSE_MOTION) && response.hovered() {
                                self.mouse_report(mode, 35, col, row, true, modifiers);
                            }
                        }
                    } else if self.selecting
                        && let Some(sel) = self.term.lock().selection.as_mut()
                    {
                        sel.update(point, side);
                    }
                }
                _ => {}
            }
        }

        // While drag-selecting past the top/bottom edge, keep scrolling (and extending the
        // selection) even if the pointer stops moving.
        if self.selecting && !mouse_mode
            && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
        {
            let delta = if pos.y < interact_rect.top() {
                1
            } else if pos.y > interact_rect.bottom() {
                -1
            } else {
                0
            };
            if delta != 0 {
                let mut term = self.term.lock();
                term.scroll_display(Scroll::Delta(delta));
                let offset = term.grid().display_offset();
                drop(term);
                let clamped = pos2(pos.x, pos.y.clamp(interact_rect.top(), interact_rect.bottom() - 1.0));
                let (point, side, _, _) = self.point_at(origin, clamped, offset);
                if let Some(sel) = self.term.lock().selection.as_mut() {
                    sel.update(point, side);
                }
                ui.ctx().request_repaint_after(Duration::from_millis(40));
            }
        }
    }

    /// Send an xterm mouse report (SGR or legacy encoding).
    fn mouse_report(&mut self, mode: TermMode, button: u8, col: usize, row: usize, pressed: bool, mods: Modifiers) {
        let mut b = button;
        if mods.shift {
            b += 4;
        }
        if mods.alt {
            b += 8;
        }
        if mods.ctrl {
            b += 16;
        }
        if mode.contains(TermMode::SGR_MOUSE) {
            let suffix = if pressed { 'M' } else { 'm' };
            self.write(format!("\x1b[<{};{};{}{}", b, col + 1, row + 1, suffix).into_bytes());
        } else {
            // Legacy X10 encoding: releases are button 3, coordinates offset by 32.
            let b = if pressed { b } else { 3 + (b & !3) };
            if col > 222 || row > 222 {
                return;
            }
            self.write(vec![0x1b, b'[', b'M', 32 + b, 32 + col as u8 + 1, 32 + row as u8 + 1]);
        }
    }

    fn handle_keyboard(&mut self, ui: &egui::Ui) {
        let events = ui.input(|i| i.events.clone());
        let mode = *self.term.lock().mode();
        let modes = KeyModes { app_cursor: mode.contains(TermMode::APP_CURSOR), app_keypad: mode.contains(TermMode::APP_KEYPAD) };
        for event in events {
            match event {
                egui::Event::Copy | egui::Event::Cut => {
                    let text = self.term.lock().selection_to_string();
                    if let Some(text) = text.filter(|t| !t.is_empty()) {
                        ui.ctx().copy_text(text);
                    }
                }
                egui::Event::Paste(text) => {
                    self.on_user_input();
                    self.write(encode_paste(&text, mode.contains(TermMode::BRACKETED_PASTE)));
                }
                egui::Event::Text(text) => {
                    if std::mem::take(&mut self.suppress_text) {
                        continue;
                    }
                    self.on_user_input();
                    self.write(text.into_bytes());
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    self.on_user_input();
                    self.write(text.into_bytes());
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    // Host shortcuts on macOS.
                    if modifiers.mac_cmd {
                        match key {
                            Key::K => {
                                let mut term = self.term.lock();
                                term.grid_mut().clear_history();
                                drop(term);
                                // Ctrl+L asks the shell to redraw the prompt at the top.
                                self.write(b"\x0c".to_vec());
                            }
                            Key::A => {
                                let mut term = self.term.lock();
                                let top = Point::new(Line(-(term.grid().history_size() as i32)), Column(0));
                                let bottom = Point::new(Line(term.screen_lines() as i32 - 1), Column(term.columns() - 1));
                                let mut sel = Selection::new(SelectionType::Simple, top, Side::Left);
                                sel.update(bottom, Side::Right);
                                term.selection = Some(sel);
                            }
                            _ => {}
                        }
                        continue;
                    }
                    if let Some(bytes) = encode_key(key, modifiers, modes) {
                        // Option+letter produces a composed character as Text too; we sent
                        // the Meta sequence instead, so drop that Text event.
                        if modifiers.alt && !modifiers.ctrl {
                            self.suppress_text = true;
                        }
                        // Ctrl combos never produce Text on macOS, but do on some platforms.
                        if modifiers.ctrl {
                            self.suppress_text = true;
                        }
                        self.on_user_input();
                        self.term.lock().selection = None;
                        self.write(bytes);
                    }
                }
                _ => {}
            }
        }
    }

    /// Print an exit notice into the terminal once the child is gone.
    fn report_exit(&mut self) {
        if self.exit_reported {
            return;
        }
        let Some(code) = self.exited() else { return };
        self.exit_reported = true;
        let mut term = self.term.lock();
        // Start on a fresh line, but don't leave a blank one if output already ended with \n.
        let newline = if term.grid().cursor.point.column.0 > 0 { "\r\n" } else { "" };
        let notice = if code == 0 {
            "\x1b[2m[Process completed]".to_string()
        } else {
            format!("\x1b[31m[Process exited with code {code}]")
        };
        // Reset attributes, print the notice, and hide the cursor: nothing reads input anymore.
        let msg = format!("{newline}\x1b[0m{notice}\x1b[0m\r\n\x1b[?25l");
        self.parser.advance(&mut *term, msg.as_bytes());
    }
}

impl Drop for TerminalView {
    fn drop(&mut self) {
        // Shutting down the event loop drops the PTY, which hangs up and kills the child.
        if let Some(sender) = &self.sender {
            let _ = sender.send(Msg::Shutdown);
        }
    }
}

/// Resolve `program` the way `execvp` would: as a path if it contains a slash, else via `$PATH`.
fn find_program(program: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let is_exec = |p: &Path| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if program.contains('/') {
        let p = Path::new(program);
        return is_exec(p).then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join(program)).find(|p| is_exec(p))
}

/// Scrollback position, for drawing the scrollbar.
#[derive(Clone, Copy)]
struct ScrollState {
    history: usize,
    offset: usize,
    lines: usize,
}

/// Composite a (possibly translucent) `top` over an opaque `bottom`.
///
/// `Color32` stores premultiplied alpha, so the channels must be unmultiplied first.
/// Very faint selection colors are boosted so a selection is always visible.
fn blend_over(bottom: Color32, top: Color32) -> Color32 {
    let [r, g, b, a] = top.to_srgba_unmultiplied();
    let a = (a.max(70)) as f32 / 255.0;
    let mix = |lo: u8, hi: u8| (lo as f32 * (1.0 - a) + hi as f32 * a).round() as u8;
    Color32::from_rgb(mix(bottom.r(), r), mix(bottom.g(), g), mix(bottom.b(), b))
}

fn row_hash(row: &[CellView], font_key: (u32, u32)) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    font_key.hash(&mut h);
    row.hash(&mut h);
    h.finish()
}
