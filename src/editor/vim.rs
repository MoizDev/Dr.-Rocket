//! Vim mode: Neovim's normal, visual, replace and insert modes over a `Document`.
//!
//! Keys arrive as characters (printable keys as themselves, special keys as control codes, see
//! `ESC` and `ctrl`). In normal and visual mode they accumulate into a command, parsed as
//! `["x][count]{operator}[count]{motion | text object}` or a simple command, and run when
//! complete. Insert mode is typed by the editor itself; this module only sees Escape there, plus
//! a copy of each key for `.` and macros (`note_insert`), and replays them on its own.
//!
//! Words follow Racket: `make-node` is one word (only brackets, quotes and whitespace split).
//! Yanks and deletes also go to the system clipboard (Neovim's `clipboard=unnamedplus`).
//!
//! On top of Vim itself, a LazyVim-style layer: Space is the leader (`Space e` toggles the
//! sidebar, `Space f f` finds files, `Space r r` runs...), with a which-key list of what can
//! follow, `gc` comments, `gd` / `K` go to definitions and docs, and `:` opens a command menu
//! that also offers the app's commands (`:Run`, `:Visualize`).

use super::buffer::{EditKind, Replace, Selection, Selections};
use super::document::{Document, Motion as DocMotion};
use super::lexer::{self, Kind};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Visual,
    VisualLine,
}


pub const ESC: char = '\x1b';
pub const ENTER: char = '\r';
pub const BACKSPACE: char = '\x08';
pub const TAB: char = '\t';

/// Ctrl+letter as a control code (`ctrl('d')` is ^D).
pub const fn ctrl(c: char) -> char {
    ((c as u8) & 0x1f) as char
}

/// What the editor knows that Vim needs: the visible lines, typing settings and the clipboard.
pub struct View<'a> {
    pub first_line: usize,
    pub last_line: usize,
    pub auto_close: bool,
    pub clipboard: &'a dyn Fn() -> Option<String>,
}

/// What Vim asks of the editor and the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Save,
    Close { force: bool },
    /// Put this text on the system clipboard.
    Clipboard(String),
    Scroll(ScrollTo),
    /// Run one of the app's commands, by its name in `commands.rs` (`ToggleLeftPanel`).
    App(&'static str),
    GotoDefinition,
    ShowDocs,
    Rename,
}

/// The leader map (after Space): keys, what they do, and the which-key description. Groups
/// (a key that only leads to others) are listed with `None`.
pub const LEADER: &[(&str, Option<Leader>, &str)] = &[
    ("e", Some(Leader::App("ToggleLeftPanel")), "Explorer"),
    ("E", Some(Leader::App("ShowFiles")), "Reveal in explorer"),
    (" ", Some(Leader::App("QuickOpen")), "Find files"),
    ("/", Some(Leader::App("ShowSearch")), "Search in project"),
    (",", Some(Leader::App("QuickOpen")), "Switch file"),
    (":", Some(Leader::App("Palette")), "Command palette"),
    ("?", Some(Leader::App("KeyboardShortcuts")), "Keyboard shortcuts"),
    ("f", None, "+file"),
    ("ff", Some(Leader::App("QuickOpen")), "Find files"),
    ("fr", Some(Leader::App("QuickOpen")), "Recent files"),
    ("fn", Some(Leader::App("NewFile")), "New file"),
    ("fo", Some(Leader::App("OpenFolder")), "Open folder"),
    ("fs", Some(Leader::App("Save")), "Save"),
    ("fS", Some(Leader::App("SaveAll")), "Save all"),
    ("fR", Some(Leader::App("RevealInFinder")), "Reveal in Finder"),
    ("b", None, "+buffer"),
    ("bd", Some(Leader::App("CloseTab")), "Close buffer"),
    ("bn", Some(Leader::App("NextTab")), "Next buffer"),
    ("bp", Some(Leader::App("PrevTab")), "Previous buffer"),
    ("bb", Some(Leader::App("PrevTab")), "Other buffer"),
    ("bu", Some(Leader::App("ReopenClosedTab")), "Reopen closed buffer"),
    ("c", None, "+code"),
    ("cf", Some(Leader::App("ReindentAll")), "Format (reindent)"),
    ("cr", Some(Leader::Rename), "Rename symbol"),
    ("cd", Some(Leader::App("CheckSyntax")), "Check syntax"),
    ("cc", Some(Leader::Comment), "Toggle comment"),
    ("cs", Some(Leader::App("GoToSymbol")), "Go to symbol"),
    ("r", None, "+racket"),
    ("rr", Some(Leader::App("Run")), "Run"),
    ("rv", Some(Leader::App("Visualize")), "Visualize"),
    ("rs", Some(Leader::App("Stepper")), "Stepper"),
    ("rd", Some(Leader::App("Debug")), "Debug"),
    ("rt", Some(Leader::App("RunTests")), "Run tests"),
    ("rc", Some(Leader::App("RunWithCoverage")), "Run with coverage"),
    ("rp", Some(Leader::App("Profile")), "Profile"),
    ("rm", Some(Leader::App("MacroStepper")), "Macro stepper"),
    ("rk", Some(Leader::App("Kill")), "Kill"),
    ("rl", Some(Leader::App("ChooseLanguage")), "Choose language"),
    ("ri", Some(Leader::App("FocusInteractions")), "Interactions"),
    ("rx", Some(Leader::App("ClearInteractions")), "Clear interactions"),
    ("s", None, "+search"),
    ("sg", Some(Leader::App("ShowSearch")), "Grep in project"),
    ("ss", Some(Leader::App("GoToSymbol")), "Symbols"),
    ("sh", Some(Leader::App("SearchDocs")), "Search docs"),
    ("sk", Some(Leader::App("KeyboardShortcuts")), "Keymaps"),
    ("sc", Some(Leader::App("Palette")), "Commands"),
    ("t", None, "+terminal"),
    ("tt", Some(Leader::App("NewTerminal")), "New terminal"),
    ("tb", Some(Leader::App("ToggleBottomPanel")), "Toggle bottom panel"),
    ("u", None, "+ui"),
    ("ue", Some(Leader::App("ToggleLeftPanel")), "Toggle explorer"),
    ("ur", Some(Leader::App("ToggleRightPanel")), "Toggle right panel"),
    ("ub", Some(Leader::App("ToggleBottomPanel")), "Toggle bottom panel"),
    ("ut", Some(Leader::App("SelectTheme")), "Theme"),
    ("uR", Some(Leader::App("ToggleRainbowBrackets")), "Rainbow brackets"),
    ("ua", Some(Leader::App("ToggleBindingArrows")), "Binding arrows"),
    ("us", Some(Leader::App("Settings")), "Settings"),
    ("x", None, "+problems"),
    ("xx", Some(Leader::App("NextProblem")), "Next problem"),
    ("xl", Some(Leader::App("ShowLog")), "Log"),
    ("q", None, "+quit"),
    ("qq", Some(Leader::App("CloseTab")), "Close file"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leader {
    App(&'static str),
    Rename,
    Comment,
}

/// One which-key entry: the key, its description, and whether it opens a group.
pub type WhichKey = (String, &'static str, bool);

/// What `:` offers: Vim's commands and the app's (shown as you type, Tab completes).
pub const EX_COMMANDS: &[(&str, &str)] = &[
    ("w", "Save"),
    ("q", "Close the file"),
    ("wq", "Save and close"),
    ("x", "Save and close"),
    ("q!", "Close without saving"),
    ("wa", "Save all"),
    ("e", "Open a file"),
    ("%s/", "Replace in the whole file"),
    ("s/", "Replace in this line"),
    ("noh", "Clear the search highlight"),
    ("$", "Go to the last line"),
    ("Run", "Run the program"),
    ("Visualize", "Visualize the program"),
    ("Stepper", "Step through the program"),
    ("Debug", "Debug the program"),
    ("Test", "Run the tests"),
    ("Format", "Reindent the file"),
    ("Explorer", "Toggle the file explorer"),
    ("Terminal", "Open a terminal"),
    ("Theme", "Choose a theme"),
    ("Language", "Choose the language"),
    ("Settings", "Open settings"),
    ("Docs", "Search the documentation"),
];

/// The app command an ex command stands for.
fn ex_app(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "Run" | "run" => "Run",
        "Visualize" | "visualize" | "Viz" => "Visualize",
        "Stepper" | "stepper" | "Step" => "Stepper",
        "Debug" | "debug" => "Debug",
        "Test" | "test" | "Tests" => "RunTests",
        "Format" | "format" => "ReindentAll",
        "Explorer" | "explorer" | "Ex" | "Lex" | "NvimTreeToggle" | "Neotree" => "ToggleLeftPanel",
        "Terminal" | "terminal" | "term" => "NewTerminal",
        "Theme" | "colorscheme" | "colo" => "SelectTheme",
        "Language" | "language" => "ChooseLanguage",
        "Settings" | "settings" => "Settings",
        "Docs" | "docs" | "help" | "h" => "SearchDocs",
        "wa" | "wall" => "SaveAll",
        "e" | "edit" | "Files" => "QuickOpen",
        "bn" | "bnext" => "NextTab",
        "bp" | "bprevious" | "bprev" => "PrevTab",
        "bd" | "bdelete" => "CloseTab",
        "vs" | "vsplit" | "sp" | "split" => "QuickOpen",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollTo {
    /// The cursor line at the center, top or bottom of the view.
    Center,
    Top,
    Bottom,
    /// By this many lines (half pages, pages).
    Lines(isize),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Register {
    text: String,
    linewise: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    Reindent,
    Lower,
    Upper,
    Toggle,
    Comment,
}

/// How a motion's range counts: up to (exclusive) or including (inclusive) the target
/// character, or whole lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Clone, Debug, PartialEq)]
enum Action {
    Motion(char, Option<char>),
    /// `iw`, `a(`...: (inner, object).
    Object(bool, char),
    /// `dd`, `cc`, `yy`, `>>`: the operator on whole lines.
    Lines,
    Simple(String),
}

#[derive(Debug, PartialEq)]
struct Cmd {
    reg: Option<char>,
    count: Option<usize>,
    op: Option<Op>,
    action: Action,
}

enum Parse {
    Incomplete,
    Invalid,
    Done(Cmd),
}

pub struct Vim {
    pub mode: Mode,
    /// The command being typed (normal and visual mode).
    keys: Vec<char>,
    /// `:`, `/` or `?` and what's typed after it.
    pub cmdline: Option<(char, String)>,
    /// A short note for the status line ("search hit BOTTOM", errors).
    pub message: Option<String>,
    regs: HashMap<char, Register>,
    marks: HashMap<char, usize>,
    last_find: Option<(char, char)>,
    search: Option<(String, bool)>,
    /// The last change, as keys, for `.`.
    dot: Vec<char>,
    /// The change being recorded (a command that entered insert mode, until Escape).
    change: Option<Vec<char>>,
    recording: Option<(char, Vec<char>)>,
    last_macro: Option<char>,
    replaying: usize,
    /// Visual mode: the fixed end and the cursor.
    anchor: usize,
    cursor: usize,
    last_visual: Option<(usize, usize, Mode)>,
    goal: Option<usize>,
    /// An undo group is open for the current insert session.
    insert_group: bool,
    /// Insert mode's ^O: one normal-mode command, then back to inserting.
    one_shot: bool,
    /// Insert mode's ^R, waiting for the register name.
    insert_reg: bool,
    /// The highlighted suggestion in the `:` menu.
    pub ex_selected: Option<usize>,
    /// Text just yanked, for the editor to flash.
    pub yanked: Option<std::ops::Range<usize>>,
}

impl Default for Vim {
    fn default() -> Self {
        Vim {
            mode: Mode::Normal,
            keys: Vec::new(),
            cmdline: None,
            message: None,
            regs: HashMap::new(),
            marks: HashMap::new(),
            last_find: None,
            search: None,
            dot: Vec::new(),
            change: None,
            recording: None,
            last_macro: None,
            replaying: 0,
            anchor: 0,
            cursor: 0,
            last_visual: None,
            goal: None,
            insert_group: false,
            one_shot: false,
            insert_reg: false,
            ex_selected: None,
            yanked: None,
        }
    }
}

// ------------------------------------------------------------------ text helpers

/// Character classes for word motions: blank, word (Racket identifiers), punctuation.
fn class(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || !lexer::is_delimiter(c) {
        1
    } else {
        2
    }
}

fn line_of(doc: &Document, pos: usize) -> usize {
    doc.buffer.line_of(pos)
}

fn line_start(doc: &Document, line: usize) -> usize {
    doc.buffer.line_start(line)
}

/// End of the line's text (before its newline).
fn line_end(doc: &Document, line: usize) -> usize {
    doc.buffer.line_end(line)
}

fn first_non_blank(doc: &Document, line: usize) -> usize {
    line_start(doc, line) + doc.buffer.indentation(line).min(doc.buffer.line_len(line))
}

fn last_line(doc: &Document) -> usize {
    doc.buffer.line_count().saturating_sub(1)
}

fn blank_line(doc: &Document, line: usize) -> bool {
    doc.buffer.line_text(line).trim().is_empty()
}

/// In normal mode the cursor sits on a character: never past the last one of its line.
fn clamp_normal(doc: &Document, pos: usize) -> usize {
    let pos = pos.min(doc.buffer.len());
    let line = line_of(doc, pos);
    let (s, e) = (line_start(doc, line), line_end(doc, line));
    if e > s && pos >= e { e - 1 } else { pos }
}

impl Vim {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keys typed so far for the current command (shown beside the mode).
    pub fn pending(&self) -> String {
        if self.insert_reg {
            return "^R".into();
        }
        self.keys.iter().map(|c| display_key(*c)).collect()
    }

    /// Insert mode is waiting for a key that isn't text to type (the register after ^R).
    pub fn awaits_key(&self) -> bool {
        self.insert_reg
    }

    /// In the middle of insert mode's ^O (one normal-mode command).
    pub fn one_shot(&self) -> bool {
        self.one_shot
    }

    /// After the leader: the keys typed after it, and what can follow (key, description, is a
    /// group), for the which-key popup.
    pub fn which_key(&self) -> Option<(String, Vec<WhichKey>)> {
        if !matches!(self.mode, Mode::Normal) {
            return None;
        }
        let start = self.keys.iter().position(|c| *c == ' ')?;
        if !self.keys[..start].iter().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let typed: String = self.keys[start + 1..].iter().collect();
        let n = typed.chars().count();
        let items = LEADER
            .iter()
            .filter(|(k, _, _)| k.starts_with(typed.as_str()) && k.chars().count() == n + 1)
            .map(|(k, a, d)| (k.chars().last().unwrap_or(' ').to_string(), *d, a.is_none()))
            .collect();
        Some((typed, items))
    }

    /// Recording a macro into this register.
    pub fn recording_into(&self) -> Option<char> {
        self.recording.as_ref().map(|r| r.0)
    }

    /// Where the block cursor goes, when it isn't the selection's head (visual mode).
    pub fn cursor_pos(&self) -> Option<usize> {
        matches!(self.mode, Mode::Visual | Mode::VisualLine).then_some(self.cursor)
    }

    /// A selection made outside Vim (the mouse, Cmd+A, a double-click): continue it in visual mode.
    pub fn adopt_selection(&mut self, sel: Selection) {
        if self.mode == Mode::Normal && !sel.is_empty() {
            self.mode = Mode::Visual;
            self.anchor = sel.anchor;
            self.cursor = if sel.head > sel.anchor { sel.head - 1 } else { sel.head };
        }
    }

    /// A key typed in insert mode by the editor itself: kept for `.` and macros.
    pub fn note_insert(&mut self, k: char) {
        if self.replaying > 0 {
            return;
        }
        if let Some(c) = self.change.as_mut() {
            c.push(k);
        }
        if let Some((_, keys)) = self.recording.as_mut() {
            keys.push(k);
        }
    }

    /// Feed one key. In insert mode only Escape (and replayed keys) come here.
    pub fn feed(&mut self, doc: &mut Document, view: &View, k: char) -> Vec<Effect> {
        doc.sync();
        let mut fx = Vec::new();
        if self.replaying == 0
            && let Some((_, keys)) = self.recording.as_mut()
            && self.cmdline.is_none()
        {
            keys.push(k);
        }
        if self.cmdline.is_some() {
            self.cmdline_key(doc, k, &mut fx);
            return fx;
        }
        if self.mode == Mode::Normal && self.keys.is_empty() && doc.buffer.selections.list.len() == 1 {
            self.adopt_selection(doc.buffer.selections.primary());
        }
        match self.mode {
            Mode::Insert | Mode::Replace => {
                if self.replaying == 0
                    && k != ESC
                    && let Some(c) = self.change.as_mut()
                {
                    c.push(k);
                }
                self.insert_key(doc, view, k);
            }
            Mode::Normal | Mode::Visual | Mode::VisualLine => {
                if k == ESC && self.keys.is_empty() && self.one_shot && self.mode == Mode::Normal {
                    self.one_shot = false;
                    self.mode = Mode::Insert;
                    return fx;
                }
                if k == ESC && self.keys.is_empty() {
                    if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
                        self.leave_visual(doc);
                    }
                    self.message = None;
                    return fx;
                }
                self.keys.push(k);
                match self.parse() {
                    Parse::Incomplete => {}
                    Parse::Invalid => {
                        self.keys.clear();
                        self.end_one_shot();
                    }
                    Parse::Done(cmd) => {
                        let keys = std::mem::take(&mut self.keys);
                        self.message = None;
                        doc.buffer.begin_group();
                        let changed = self.run(doc, view, cmd, &mut fx);
                        // A change that went into insert mode is recorded until Escape.
                        if matches!(self.mode, Mode::Insert | Mode::Replace) {
                            self.insert_group = true;
                            if self.replaying == 0 {
                                self.change = Some(keys);
                            }
                        } else {
                            doc.buffer.end_group();
                            if changed && self.replaying == 0 {
                                self.dot = keys;
                            }
                        }
                        self.end_one_shot();
                    }
                }
            }
        }
        doc.sync();
        if self.mode == Mode::Normal && !self.one_shot {
            let p = doc.buffer.selections.primary();
            if doc.buffer.selections.list.len() == 1 && p.is_empty() {
                let c = clamp_normal(doc, p.head);
                doc.buffer.selections = Selections::single(Selection::caret(c));
            }
        }
        if matches!(self.mode, Mode::Visual | Mode::VisualLine) {
            self.show_visual(doc);
        }
        fx
    }

    // ------------------------------------------------------------------ insert mode

    fn insert_key(&mut self, doc: &mut Document, view: &View, k: char) {
        if std::mem::take(&mut self.insert_reg) {
            // ^R{register}: type its contents.
            let reg = if k == ctrl('w') { Some('"') } else { Some(k) };
            if let Some(r) = self.fetch(reg, view).filter(|_| !k.is_control() || k == ctrl('w')) {
                doc.type_text(&r.text, false);
            }
            return;
        }
        if k == ESC || k == ctrl('[') {
            if let Some(mut c) = self.change.take() {
                c.push(ESC);
                self.dot = c;
            }
            if self.insert_group {
                doc.buffer.end_group();
                self.insert_group = false;
            }
            self.mode = Mode::Normal;
            // Leaving insert mode steps back onto the last typed character.
            let p = doc.buffer.selections.primary().head;
            let ls = line_start(doc, line_of(doc, p));
            let to = if p > ls { p - 1 } else { p };
            doc.buffer.selections = Selections::single(Selection::caret(to));
            doc.buffer.break_undo_group();
            return;
        }
        let p = doc.buffer.selections.primary().head;
        let line = line_of(doc, p);
        let ls = line_start(doc, line);
        // Replayed keys (`.`, macros) and the control keys; live typing goes through the editor.
        match k {
            ENTER => doc.newline(true),
            c if c == ctrl('j') || c == ctrl('m') => doc.newline(true),
            BACKSPACE => doc.backspace(DocMotion::Left),
            c if c == ctrl('w') => doc.backspace(DocMotion::WordLeft),
            c if c == ctrl('h') => doc.backspace(DocMotion::Left),
            // ^U: delete what was typed before the cursor on this line (to the indent, then the
            // indent itself).
            c if c == ctrl('u') => {
                let first = first_non_blank(doc, line);
                let to = if p > first { first } else { ls };
                if to < p {
                    doc.buffer.apply(vec![Replace { range: to..p, text: String::new() }], Some(Selections::single(Selection::caret(to))), EditKind::Other);
                }
            }
            // ^T / ^D: shift the line right or left, keeping the cursor on its text.
            c if c == ctrl('t') || c == ctrl('d') => {
                let have = doc.buffer.indentation(line);
                let (range, text, moved) = if c == ctrl('t') {
                    (ls..ls, "  ".to_string(), 2isize)
                } else {
                    let n = have.min(2);
                    (ls..ls + n, String::new(), -(n as isize))
                };
                let at = if p < ls + have.min(2) && moved < 0 { ls } else { (p as isize + moved).max(ls as isize) as usize };
                doc.buffer.apply(vec![Replace { range, text }], Some(Selections::single(Selection::caret(at))), EditKind::Other);
            }
            // ^E / ^Y: the character below or above the cursor.
            c if c == ctrl('e') || c == ctrl('y') => {
                let other = if c == ctrl('e') { line + 1 } else { line.wrapping_sub(1) };
                if other < doc.buffer.line_count() {
                    let os = line_start(doc, other);
                    let col = p - ls;
                    if os + col < line_end(doc, other) {
                        let ch: String = doc.buffer.slice(os + col..os + col + 1);
                        doc.type_text(&ch, false);
                    }
                }
            }
            c if c == ctrl('r') => self.insert_reg = true,
            // ^O: one normal-mode command, then back here.
            c if c == ctrl('o') => {
                if let Some(mut c) = self.change.take() {
                    c.pop();
                    c.push(ESC);
                    self.dot = c;
                }
                if self.insert_group {
                    doc.buffer.end_group();
                    self.insert_group = false;
                }
                self.mode = Mode::Normal;
                self.one_shot = true;
                doc.buffer.break_undo_group();
            }
            TAB => doc.indent_selection_lines(),
            c if !c.is_control() => {
                if self.mode == Mode::Replace {
                    self.overwrite_next(doc);
                }
                doc.type_text(&c.to_string(), view.auto_close);
            }
            _ => {}
        }
    }

    /// After the command ^O allowed: back to inserting (unless it changed mode itself).
    fn end_one_shot(&mut self) {
        if std::mem::take(&mut self.one_shot) && self.mode == Mode::Normal {
            self.mode = Mode::Insert;
        }
    }

    /// Replace mode: the character under the cursor makes way for the typed one.
    pub fn overwrite_next(&self, doc: &mut Document) {
        let p = doc.buffer.selections.primary().head;
        if p < line_end(doc, line_of(doc, p)) {
            doc.buffer.apply(vec![Replace { range: p..p + 1, text: String::new() }], Some(Selections::single(Selection::caret(p))), EditKind::Insert);
        }
    }

    // ------------------------------------------------------------------ parsing

    fn parse(&self) -> Parse {
        let k = &self.keys;
        let visual = matches!(self.mode, Mode::Visual | Mode::VisualLine);
        let mut i = 0;
        let mut reg = None;
        if k.first() == Some(&'"') {
            match k.get(1) {
                None => return Parse::Incomplete,
                Some(r) if r.is_ascii_alphanumeric() || matches!(r, '"' | '+' | '*' | '_') => reg = Some(*r),
                Some(_) => return Parse::Invalid,
            }
            i = 2;
        }
        let count_at = |i: &mut usize| -> Option<usize> {
            let s = *i;
            while *i < k.len() && (k[*i].is_ascii_digit() && (k[*i] != '0' || *i > s)) {
                *i += 1;
            }
            (*i > s).then(|| k[s..*i].iter().collect::<String>().parse().ok()).flatten()
        };
        let count = count_at(&mut i);
        let Some(&c) = k.get(i) else { return Parse::Incomplete };
        let rest = &k[i..];
        // The leader (Space) and the which-key map.
        if c == ' ' {
            let typed: String = rest[1..].iter().collect();
            if typed.is_empty() {
                return Parse::Incomplete;
            }
            if LEADER.iter().any(|(keys, a, _)| *keys == typed && a.is_some()) {
                return Parse::Done(Cmd { reg, count, op: None, action: Action::Simple(format!("leader:{typed}")) });
            }
            return if LEADER.iter().any(|(keys, _, _)| keys.starts_with(typed.as_str())) { Parse::Incomplete } else { Parse::Invalid };
        }
        let done = |op: Option<Op>, action: Action, count2: Option<usize>| {
            let count = match (count, count2) {
                (Some(a), Some(b)) => Some(a * b),
                (a, b) => a.or(b),
            };
            Parse::Done(Cmd { reg, count, op, action })
        };
        // Operators (in visual mode they apply to the selection right away).
        let op = match rest {
            ['d', ..] => Some((Op::Delete, 1)),
            ['c', ..] => Some((Op::Change, 1)),
            ['y', ..] => Some((Op::Yank, 1)),
            ['>', ..] => Some((Op::Indent, 1)),
            ['<', ..] => Some((Op::Outdent, 1)),
            ['=', ..] => Some((Op::Reindent, 1)),
            ['g', '~', ..] => Some((Op::Toggle, 2)),
            ['g', 'u', ..] => Some((Op::Lower, 2)),
            ['g', 'U', ..] => Some((Op::Upper, 2)),
            ['g', 'c', ..] => Some((Op::Comment, 2)),
            ['g'] => return Parse::Incomplete,
            _ => None,
        };
        if let Some((op, n)) = op {
            if visual {
                return done(Some(op), Action::Simple(String::new()), None);
            }
            let mut j = i + n;
            let count2 = count_at(&mut j);
            let Some(&m) = k.get(j) else { return Parse::Incomplete };
            // dd, cc, yy, >>, <<, ==, g~~ / g~g~, guu / gugu, gUU / gUgU.
            let doubled = match op {
                Op::Toggle => m == '~' || (m == 'g' && k.get(j + 1).map(|c| *c == '~').unwrap_or(false)),
                Op::Lower => m == 'u' || (m == 'g' && k.get(j + 1).map(|c| *c == 'u').unwrap_or(false)),
                Op::Upper => m == 'U' || (m == 'g' && k.get(j + 1).map(|c| *c == 'U').unwrap_or(false)),
                Op::Comment => m == 'c',
                _ => m == k[i],
            };
            if matches!(op, Op::Toggle | Op::Lower | Op::Upper) && m == 'g' && k.len() == j + 1 {
                return Parse::Incomplete;
            }
            if doubled {
                return done(Some(op), Action::Lines, count2);
            }
            return match motion_or_object(&k[j..], true) {
                Parse::Done(Cmd { action, .. }) => done(Some(op), action, count2),
                p => p,
            };
        }
        // Motions (and, in visual mode, text objects).
        match motion_or_object(rest, visual) {
            Parse::Done(Cmd { action, .. }) => return done(None, action, None),
            Parse::Incomplete => return Parse::Incomplete,
            Parse::Invalid => {}
        }
        // Simple commands.
        let simple = |s: &str| done(None, Action::Simple(s.to_string()), None);
        match rest {
            [c] if "xXDCYsSJpPuiaIAoORvV~.:/?".contains(*c) => simple(&c.to_string()),
            [c] if *c == ctrl('r') || *c == ctrl('v') || *c == ctrl('a') || *c == ctrl('x') || *c == ctrl('s') => simple(&c.to_string()),
            ['r'] | ['m'] | ['q'] | ['@'] | ['Z'] | ['z'] | ['g'] => {
                if rest == ['q'] && self.recording.is_some() {
                    return simple("q");
                }
                Parse::Incomplete
            }
            ['r', x] => simple(&format!("r{x}")),
            ['m', x] if x.is_ascii_alphabetic() => simple(&format!("m{x}")),
            ['q', x] if x.is_ascii_alphanumeric() => simple(&format!("q{x}")),
            ['@', x] if x.is_ascii_alphanumeric() || *x == '@' => simple(&format!("@{x}")),
            ['Z', x] if matches!(x, 'Z' | 'Q') => simple(&format!("Z{x}")),
            ['z', x] if matches!(x, 'z' | 't' | 'b' | '.' | '-') || *x == ENTER => simple(&format!("z{x}")),
            ['g', x] if matches!(x, 'v' | 'J' | 'i' | 'I' | 'd' | 'D' | 'r') => simple(&format!("g{x}")),
            ['K'] => simple("K"),
            ['[' | ']'] => Parse::Incomplete,
            [b @ ('[' | ']'), x] if matches!(x, 'b' | 'd' | 'q') => simple(&format!("{b}{x}")),
            [c] if visual && "UoyY".contains(*c) => simple(&c.to_string()),
            _ => Parse::Invalid,
        }
    }

    // ------------------------------------------------------------------ running commands

    /// Run a parsed command; returns whether it changed the text (for `.`).
    fn run(&mut self, doc: &mut Document, view: &View, cmd: Cmd, fx: &mut Vec<Effect>) -> bool {
        let count = cmd.count.unwrap_or(1).max(1);
        let visual = matches!(self.mode, Mode::Visual | Mode::VisualLine);
        if visual {
            return self.run_visual(doc, view, cmd, fx);
        }
        let pos = doc.buffer.selections.primary().head;
        match (cmd.op, &cmd.action) {
            (None, Action::Motion(m, arg)) => {
                if let Some((to, span)) = self.motion(doc, view, pos, *m, *arg, cmd.count, false, fx) {
                    let to = if span == Span::Linewise && matches!(*m, 'G' | 'g' | 'H' | 'M' | 'L' | '\'' | '+' | '-' | '_' | ENTER) { first_non_blank(doc, line_of(doc, to)) } else { to };
                    doc.buffer.selections = Selections::single(Selection::caret(to));
                    doc.buffer.break_undo_group();
                }
                false
            }
            (Some(op), Action::Lines) => {
                let l1 = line_of(doc, pos);
                let l2 = (l1 + count - 1).min(last_line(doc));
                self.apply_op(doc, op, line_start(doc, l1), line_start(doc, l2), Span::Linewise, cmd.reg, fx);
                op != Op::Yank
            }
            (Some(op), Action::Motion(m, arg)) => {
                let Some((mut to, mut span)) = self.motion(doc, view, pos, *m, *arg, cmd.count, true, fx) else { return false };
                // `cw` changes to the end of the word, like `ce`; `dw` stops at the line's end.
                if op == Op::Change && matches!(m, 'w' | 'W') && doc.buffer.char_at(pos).is_some_and(|c| !c.is_whitespace()) {
                    if let Some((e, _)) = self.motion(doc, view, pos, if *m == 'w' { 'e' } else { 'E' }, None, cmd.count, true, fx) {
                        to = e;
                        span = Span::Inclusive;
                    }
                } else if matches!(m, 'w' | 'W') && line_of(doc, to) > line_of(doc, pos) {
                    let l = line_of(doc, to);
                    let lend = line_end(doc, line_of(doc, pos));
                    if to == first_non_blank(doc, l) || to == line_start(doc, l) {
                        to = lend.max(pos);
                    }
                }
                self.apply_op(doc, op, pos, to, span, cmd.reg, fx);
                op != Op::Yank
            }
            (Some(op), Action::Object(inner, obj)) => {
                let Some((a, b, linewise)) = self.object(doc, pos, *inner, *obj, count) else { return false };
                if linewise {
                    self.apply_op(doc, op, a, b.saturating_sub(1), Span::Linewise, cmd.reg, fx);
                } else {
                    self.apply_op(doc, op, a, b, Span::Exclusive, cmd.reg, fx);
                }
                op != Op::Yank
            }
            (None, Action::Simple(s)) => self.simple(doc, view, s, count, cmd.count, cmd.reg, fx),
            _ => false,
        }
    }

    /// Apply an operator from `a` to `b` (either order) with the span type.
    #[allow(clippy::too_many_arguments)]
    fn apply_op(&mut self, doc: &mut Document, op: Op, a: usize, b: usize, span: Span, reg: Option<char>, fx: &mut Vec<Effect>) {
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let n = doc.buffer.len();
        let (start, end) = match span {
            Span::Exclusive => (a, b),
            Span::Inclusive => (a, (b + 1).min(n)),
            Span::Linewise => {
                let (l1, l2) = (line_of(doc, a), line_of(doc, b));
                let end = if l2 < last_line(doc) { line_start(doc, l2 + 1) } else { n };
                (line_start(doc, l1), end)
            }
        };
        let linewise = span == Span::Linewise;
        let (l1, l2) = (line_of(doc, start), line_of(doc, end.saturating_sub(1).max(start)));
        match op {
            Op::Yank | Op::Delete | Op::Change => {
                let mut text = doc.buffer.slice(start..end);
                if linewise && !text.ends_with('\n') {
                    text.push('\n');
                }
                self.store(reg, Register { text, linewise }, op == Op::Yank, fx);
                match op {
                    Op::Yank => {
                        self.yanked = Some(start..if linewise { end.saturating_sub(1).max(start) } else { end });
                        let to = if linewise { doc.buffer.selections.primary().head.max(start).min(end) } else { start };
                        let to = if linewise && line_of(doc, to) != l1 { start } else { to };
                        doc.buffer.selections = Selections::single(Selection::caret(to));
                    }
                    Op::Delete => {
                        // The last lines take the newline before them with them.
                        let (s, e) = if linewise && end == n && l1 > 0 { (line_end(doc, l1 - 1), n) } else { (start, end) };
                        doc.buffer.apply(vec![Replace { range: s..e, text: String::new() }], Some(Selections::single(Selection::caret(s))), EditKind::Other);
                        doc.sync();
                        if linewise {
                            let l = l1.min(last_line(doc));
                            let to = first_non_blank(doc, l);
                            doc.buffer.selections = Selections::single(Selection::caret(to));
                        }
                    }
                    _ => {
                        if linewise {
                            // Change lines: keep the first line's indentation.
                            let indent = " ".repeat(doc.buffer.indentation(l1));
                            let e = line_end(doc, l2);
                            let at = start + indent.chars().count();
                            doc.buffer.apply(vec![Replace { range: start..e, text: indent }], Some(Selections::single(Selection::caret(at))), EditKind::Other);
                        } else {
                            doc.buffer.apply(vec![Replace { range: start..end, text: String::new() }], Some(Selections::single(Selection::caret(start))), EditKind::Other);
                        }
                        self.mode = Mode::Insert;
                    }
                }
            }
            Op::Indent | Op::Outdent => {
                let mut edits = Vec::new();
                for l in l1..=l2 {
                    if blank_line(doc, l) {
                        continue;
                    }
                    let ls = line_start(doc, l);
                    if op == Op::Indent {
                        edits.push(Replace { range: ls..ls, text: "  ".into() });
                    } else {
                        let k = doc.buffer.indentation(l).min(2);
                        edits.push(Replace { range: ls..ls + k, text: String::new() });
                    }
                }
                doc.buffer.apply(edits, None, EditKind::Other);
                doc.sync();
                doc.buffer.selections = Selections::single(Selection::caret(first_non_blank(doc, l1)));
            }
            Op::Comment => {
                let (s, e) = (line_start(doc, l1), line_end(doc, l2));
                doc.buffer.selections = Selections::single(Selection::new(s, e));
                doc.toggle_comment();
                doc.sync();
                doc.buffer.selections = Selections::single(Selection::caret(first_non_blank(doc, l1)));
            }
            Op::Reindent => {
                let lines: Vec<usize> = (l1..=l2).collect();
                doc.indent_lines(&lines, false);
                doc.sync();
                doc.buffer.selections = Selections::single(Selection::caret(first_non_blank(doc, l1)));
            }
            Op::Lower | Op::Upper | Op::Toggle => {
                let text: String = doc
                    .buffer
                    .slice(start..end)
                    .chars()
                    .map(|c| match op {
                        Op::Lower => c.to_lowercase().next().unwrap_or(c),
                        Op::Upper => c.to_uppercase().next().unwrap_or(c),
                        _ if c.is_uppercase() => c.to_lowercase().next().unwrap_or(c),
                        _ => c.to_uppercase().next().unwrap_or(c),
                    })
                    .collect();
                doc.buffer.apply(vec![Replace { range: start..end, text }], Some(Selections::single(Selection::caret(start))), EditKind::Other);
            }
        }
    }

    /// Save to a register. Unnamed yanks and deletes also go to the system clipboard.
    fn store(&mut self, reg: Option<char>, r: Register, yank: bool, fx: &mut Vec<Effect>) {
        match reg {
            Some('_') => return,
            Some(c) if c.is_ascii_uppercase() => {
                let e = self.regs.entry(c.to_ascii_lowercase()).or_default();
                e.text.push_str(&r.text);
                e.linewise |= r.linewise;
            }
            Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {
                self.regs.insert(c, r.clone());
            }
            _ => fx.push(Effect::Clipboard(r.text.clone())),
        }
        if yank {
            self.regs.insert('0', r.clone());
        }
        self.regs.insert('"', r);
    }

    /// The register to put: a named one, or the clipboard (with the unnamed register's
    /// linewise-ness when it still holds the same text).
    fn fetch(&self, reg: Option<char>, view: &View) -> Option<Register> {
        match reg {
            Some(c) if c.is_ascii_alphanumeric() => self.regs.get(&c.to_ascii_lowercase()).cloned(),
            _ => {
                let unnamed = self.regs.get(&'"').cloned();
                match (view.clipboard)() {
                    Some(sys) if unnamed.as_ref().is_none_or(|u| u.text != sys) => {
                        let linewise = sys.ends_with('\n');
                        Some(Register { text: sys, linewise })
                    }
                    _ => unnamed,
                }
            }
        }
    }

    fn put(&mut self, doc: &mut Document, view: &View, before: bool, count: usize, reg: Option<char>) {
        let Some(r) = self.fetch(reg, view) else { return };
        let text = r.text.repeat(count);
        let pos = doc.buffer.selections.primary().head;
        if r.linewise {
            let line = line_of(doc, pos);
            let (at, text) = if before {
                (line_start(doc, line), text)
            } else if line < last_line(doc) {
                (line_start(doc, line + 1), text)
            } else {
                // After the last line: a newline first, none after.
                (doc.buffer.len(), format!("\n{}", text.trim_end_matches('\n')))
            };
            doc.buffer.apply(vec![Replace { range: at..at, text }], Some(Selections::single(Selection::caret(at))), EditKind::Other);
            doc.sync();
            let first = if before { line } else { line + 1 };
            doc.buffer.selections = Selections::single(Selection::caret(first_non_blank(doc, first.min(last_line(doc)))));
        } else {
            let line_has_text = line_end(doc, line_of(doc, pos)) > line_start(doc, line_of(doc, pos));
            let at = if before || !line_has_text { pos } else { (pos + 1).min(doc.buffer.len()) };
            let len = text.chars().count();
            doc.buffer.apply(vec![Replace { range: at..at, text }], Some(Selections::single(Selection::caret(at + len.saturating_sub(1)))), EditKind::Other);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn simple(&mut self, doc: &mut Document, view: &View, s: &str, count: usize, raw_count: Option<usize>, reg: Option<char>, fx: &mut Vec<Effect>) -> bool {
        let pos = doc.buffer.selections.primary().head;
        let line = line_of(doc, pos);
        let mut chars = s.chars();
        let first = chars.next().unwrap_or(' ');
        let arg = chars.next();
        let lend = line_end(doc, line);
        let caret = |doc: &mut Document, p: usize| doc.buffer.selections = Selections::single(Selection::caret(p));
        match (first, arg) {
            ('x', None) => {
                let e = (pos + count).min(lend);
                if e > pos {
                    self.apply_op(doc, Op::Delete, pos, e, Span::Exclusive, reg, fx);
                }
                e > pos
            }
            ('X', None) => {
                let ls = line_start(doc, line);
                let s = pos.saturating_sub(count).max(ls);
                if s < pos {
                    self.apply_op(doc, Op::Delete, s, pos, Span::Exclusive, reg, fx);
                }
                s < pos
            }
            ('D', None) | ('C', None) => {
                let l2 = (line + count - 1).min(last_line(doc));
                let e = line_end(doc, l2);
                self.apply_op(doc, if first == 'D' { Op::Delete } else { Op::Change }, pos, e, Span::Exclusive, reg, fx);
                true
            }
            ('Y', None) => {
                let l2 = (line + count - 1).min(last_line(doc));
                self.apply_op(doc, Op::Yank, line_start(doc, line), line_start(doc, l2), Span::Linewise, reg, fx);
                caret(doc, pos);
                false
            }
            ('s', None) => {
                let e = (pos + count).min(lend);
                self.apply_op(doc, Op::Change, pos, e, Span::Exclusive, reg, fx);
                true
            }
            ('S', None) => {
                let l2 = (line + count - 1).min(last_line(doc));
                self.apply_op(doc, Op::Change, line_start(doc, line), line_start(doc, l2), Span::Linewise, reg, fx);
                true
            }
            ('r', Some(c)) => {
                if pos + count > lend {
                    return false;
                }
                let text = if c == ENTER { "\n".to_string() } else { c.to_string().repeat(count) };
                let to = if c == ENTER { pos + 1 } else { pos + count - 1 };
                doc.buffer.apply(vec![Replace { range: pos..pos + count, text }], Some(Selections::single(Selection::caret(to))), EditKind::Other);
                true
            }
            ('~', None) => {
                let e = (pos + count).min(lend);
                if e > pos {
                    self.apply_op(doc, Op::Toggle, pos, e, Span::Exclusive, reg, fx);
                    caret(doc, e.min(lend.saturating_sub(1)).max(pos));
                }
                e > pos
            }
            ('J', None) | ('g', Some('J')) => {
                let joins = count.max(2) - 1;
                let raw = arg == Some('J');
                let mut changed = false;
                for _ in 0..joins {
                    let l = line_of(doc, doc.buffer.selections.primary().head);
                    if l >= last_line(doc) {
                        break;
                    }
                    let e = line_end(doc, l);
                    let next_start = line_start(doc, l + 1);
                    let next_text = first_non_blank(doc, l + 1);
                    let rest_empty = next_text >= line_end(doc, l + 1);
                    let trimmed_end = {
                        let mut p = e;
                        while !raw && p > line_start(doc, l) && doc.buffer.char_at(p - 1).is_some_and(|c| c == ' ' || c == '\t') {
                            p -= 1;
                        }
                        p
                    };
                    let closing = doc.buffer.char_at(next_text).is_some_and(|c| matches!(c, ')' | ']' | '}'));
                    let sep = if raw || rest_empty || closing || trimmed_end == line_start(doc, l) { "" } else { " " };
                    let range = if raw { e..next_start } else { trimmed_end..next_text };
                    let at = range.start;
                    doc.buffer.apply(vec![Replace { range, text: sep.to_string() }], Some(Selections::single(Selection::caret(at))), EditKind::Other);
                    doc.sync();
                    changed = true;
                }
                changed
            }
            ('p', None) | ('P', None) => {
                self.put(doc, view, first == 'P', count, reg);
                true
            }
            ('u', None) => {
                for _ in 0..count {
                    if !doc.buffer.undo() {
                        self.message = Some("Already at oldest change".into());
                        break;
                    }
                }
                self.collapse(doc);
                false
            }
            (c, None) if c == ctrl('r') => {
                for _ in 0..count {
                    if !doc.buffer.redo() {
                        self.message = Some("Already at newest change".into());
                        break;
                    }
                }
                self.collapse(doc);
                false
            }
            (c, None) if c == ctrl('a') || c == ctrl('x') => self.increment(doc, if c == ctrl('a') { count as i64 } else { -(count as i64) }),
            ('i', None) | ('a', None) | ('I', None) | ('A', None) | ('g', Some('I')) => {
                let to = match (first, arg) {
                    ('a', _) if lend > line_start(doc, line) => (pos + 1).min(lend),
                    ('I', _) => first_non_blank(doc, line),
                    ('g', Some('I')) => line_start(doc, line),
                    ('A', _) => lend,
                    _ => pos,
                };
                caret(doc, to);
                self.mode = Mode::Insert;
                false
            }
            ('o', None) | ('O', None) => {
                doc.open_line(first == 'O');
                self.mode = Mode::Insert;
                true
            }
            ('R', None) => {
                self.mode = Mode::Replace;
                false
            }
            ('v', None) | ('V', None) | (_, None) if first == 'v' || first == 'V' || first == ctrl('v') => {
                self.mode = if first == 'V' { Mode::VisualLine } else { Mode::Visual };
                self.anchor = pos;
                self.cursor = pos;
                false
            }
            ('g', Some('v')) => {
                if let Some((a, c, m)) = self.last_visual {
                    let n = doc.buffer.len();
                    self.mode = m;
                    self.anchor = a.min(n);
                    self.cursor = c.min(n);
                }
                false
            }
            ('g', Some('i')) => {
                if let Some(p) = self.marks.get(&'^').copied() {
                    caret(doc, p.min(doc.buffer.len()));
                }
                self.mode = Mode::Insert;
                false
            }
            ('.', None) => {
                let keys = self.dot.clone();
                if keys.is_empty() {
                    return false;
                }
                let times = if raw_count.is_some() { count } else { 1 };
                self.replay(doc, view, &keys, times, fx);
                false
            }
            ('m', Some(c)) => {
                self.marks.insert(c, pos);
                false
            }
            ('q', None) => {
                if let Some((r, mut keys)) = self.recording.take() {
                    // Drop the `q` that stopped the recording.
                    keys.pop();
                    self.regs.insert(r, Register { text: keys.into_iter().collect(), linewise: false });
                }
                false
            }
            ('q', Some(r)) => {
                self.recording = Some((r.to_ascii_lowercase(), Vec::new()));
                false
            }
            ('@', Some(r)) => {
                let r = if r == '@' { self.last_macro.unwrap_or('q') } else { r.to_ascii_lowercase() };
                self.last_macro = Some(r);
                if let Some(m) = self.regs.get(&r).cloned() {
                    let keys: Vec<char> = m.text.chars().collect();
                    self.replay(doc, view, &keys, count, fx);
                }
                false
            }
            ('l', Some('e')) if s.starts_with("leader:") => {
                let keys = &s["leader:".len()..];
                match LEADER.iter().find(|(k, _, _)| *k == keys).and_then(|(_, a, _)| *a) {
                    Some(Leader::App(name)) => fx.push(Effect::App(name)),
                    Some(Leader::Rename) => fx.push(Effect::Rename),
                    Some(Leader::Comment) => {
                        let l2 = (line + count - 1).min(last_line(doc));
                        self.apply_op(doc, Op::Comment, line_start(doc, line), line_start(doc, l2), Span::Linewise, None, fx);
                        return true;
                    }
                    None => {}
                }
                false
            }
            ('g', Some('d')) | ('g', Some('D')) => {
                fx.push(Effect::GotoDefinition);
                false
            }
            ('g', Some('r')) => {
                fx.push(Effect::Rename);
                false
            }
            ('K', None) => {
                fx.push(Effect::ShowDocs);
                false
            }
            ('[', Some(x)) | (']', Some(x)) => {
                let next = first == ']';
                fx.push(Effect::App(match (x, next) {
                    ('b', true) => "NextTab",
                    ('b', false) => "PrevTab",
                    _ => "NextProblem",
                }));
                false
            }
            (c, None) if c == ctrl('s') => {
                fx.push(Effect::Save);
                false
            }
            ('Z', Some('Z')) => {
                fx.push(Effect::Save);
                fx.push(Effect::Close { force: false });
                false
            }
            ('Z', Some('Q')) => {
                fx.push(Effect::Close { force: true });
                false
            }
            ('z', Some(c)) => {
                fx.push(Effect::Scroll(match c {
                    'z' | '.' => ScrollTo::Center,
                    't' | ENTER => ScrollTo::Top,
                    _ => ScrollTo::Bottom,
                }));
                false
            }
            (':', None) | ('/', None) | ('?', None) => {
                self.cmdline = Some((first, String::new()));
                false
            }
            _ => false,
        }
    }

    /// After undo/redo: back to a single caret (on a character).
    fn collapse(&self, doc: &mut Document) {
        let p = doc.buffer.selections.primary();
        doc.buffer.selections = Selections::single(Selection::caret(p.start()));
    }

    /// ^A / ^X: add to the number at or after the cursor on its line.
    fn increment(&mut self, doc: &mut Document, by: i64) -> bool {
        let pos = doc.buffer.selections.primary().head;
        let line = line_of(doc, pos);
        let text: Vec<char> = doc.buffer.line_text(line).chars().collect();
        let ls = line_start(doc, line);
        let mut i = pos - ls;
        // Back to the start of a number under the cursor, else forward to the next one.
        while i > 0 && text.get(i - 1).is_some_and(|c| c.is_ascii_digit()) {
            i -= 1;
        }
        while i < text.len() && !text[i].is_ascii_digit() {
            i += 1;
        }
        if i >= text.len() {
            return false;
        }
        let neg = i > 0 && text[i - 1] == '-';
        let s = if neg { i - 1 } else { i };
        let mut e = i;
        while e < text.len() && text[e].is_ascii_digit() {
            e += 1;
        }
        let Ok(n) = text[s..e].iter().collect::<String>().parse::<i64>() else { return false };
        let new = (n + by).to_string();
        let to = ls + s + new.chars().count() - 1;
        doc.buffer.apply(vec![Replace { range: ls + s..ls + e, text: new }], Some(Selections::single(Selection::caret(to))), EditKind::Other);
        true
    }

    fn replay(&mut self, doc: &mut Document, view: &View, keys: &[char], times: usize, fx: &mut Vec<Effect>) {
        if self.replaying > 20 {
            return;
        }
        self.replaying += 1;
        for _ in 0..times {
            for &k in keys {
                fx.extend(self.feed(doc, view, k));
            }
        }
        self.replaying -= 1;
    }

    // ------------------------------------------------------------------ visual mode

    fn leave_visual(&mut self, doc: &mut Document) {
        self.last_visual = Some((self.anchor, self.cursor, self.mode));
        self.mode = Mode::Normal;
        let c = clamp_normal(doc, self.cursor);
        doc.buffer.selections = Selections::single(Selection::caret(c));
    }

    /// The visual selection as a buffer range (inclusive of the cursor's character).
    fn visual_range(&self, doc: &Document) -> (usize, usize) {
        let (a, b) = if self.anchor <= self.cursor { (self.anchor, self.cursor) } else { (self.cursor, self.anchor) };
        let n = doc.buffer.len();
        if self.mode == Mode::VisualLine {
            let (l1, l2) = (line_of(doc, a), line_of(doc, b));
            let end = if l2 < last_line(doc) { line_start(doc, l2 + 1) } else { n };
            (line_start(doc, l1), end)
        } else {
            (a.min(n), (b + 1).min(n))
        }
    }

    fn show_visual(&self, doc: &mut Document) {
        let (a, b) = self.visual_range(doc);
        let sel = if self.cursor < self.anchor { Selection::new(b, a) } else { Selection::new(a, b) };
        doc.buffer.selections = Selections::single(sel);
    }

    fn run_visual(&mut self, doc: &mut Document, view: &View, cmd: Cmd, fx: &mut Vec<Effect>) -> bool {
        let count = cmd.count.unwrap_or(1).max(1);
        let linewise = self.mode == Mode::VisualLine;
        let (a, b) = self.visual_range(doc);
        let span = if linewise { Span::Linewise } else { Span::Exclusive };
        let end_for_op = if linewise { b.saturating_sub(1) } else { b };
        let finish = |me: &mut Self, doc: &mut Document| {
            me.last_visual = Some((me.anchor, me.cursor, me.mode));
            if me.mode != Mode::Insert {
                me.mode = Mode::Normal;
            }
            let _ = doc;
        };
        match (cmd.op, &cmd.action) {
            (Some(op), _) => {
                self.apply_op(doc, op, a, end_for_op, span, cmd.reg, fx);
                finish(self, doc);
                op != Op::Yank
            }
            (None, Action::Motion(m, arg)) => {
                if let Some((to, _)) = self.motion(doc, view, self.cursor, *m, *arg, cmd.count, false, fx) {
                    self.cursor = to.min(doc.buffer.len());
                }
                false
            }
            (None, Action::Object(inner, obj)) => {
                if let Some((s, e, _)) = self.object(doc, self.cursor, *inner, *obj, count) {
                    self.anchor = s;
                    self.cursor = e.saturating_sub(1).max(s);
                }
                false
            }
            (None, Action::Simple(s)) => {
                let op = match s.as_str() {
                    "x" | "d" | "X" | "D" => Some((Op::Delete, matches!(s.as_str(), "X" | "D"))),
                    "c" | "s" | "C" | "S" => Some((Op::Change, matches!(s.as_str(), "C" | "S"))),
                    "y" | "Y" => Some((Op::Yank, s == "Y")),
                    "~" => Some((Op::Toggle, false)),
                    "u" => Some((Op::Lower, false)),
                    "U" => Some((Op::Upper, false)),
                    _ => None,
                };
                if let Some((op, lines)) = op {
                    if lines {
                        self.apply_op(doc, op, a, b.saturating_sub(1), Span::Linewise, cmd.reg, fx);
                    } else {
                        self.apply_op(doc, op, a, end_for_op, span, cmd.reg, fx);
                    }
                    finish(self, doc);
                    return op != Op::Yank;
                }
                match s.as_str() {
                    "o" | "O" => {
                        std::mem::swap(&mut self.anchor, &mut self.cursor);
                        false
                    }
                    "v" | "V" => {
                        let to = if s == "V" { Mode::VisualLine } else { Mode::Visual };
                        if self.mode == to {
                            self.leave_visual(doc);
                        } else {
                            self.mode = to;
                        }
                        false
                    }
                    "J" => {
                        let lines = (line_of(doc, b.saturating_sub(1)) - line_of(doc, a) + 1).max(2);
                        doc.buffer.selections = Selections::single(Selection::caret(a));
                        self.mode = Mode::Normal;
                        self.simple(doc, view, "J", lines, Some(lines), None, fx)
                    }
                    "p" | "P" => {
                        // Replace the selection with the register (the old text isn't kept).
                        let reg = self.fetch(cmd.reg, view);
                        doc.buffer.apply(vec![Replace { range: a..b, text: String::new() }], Some(Selections::single(Selection::caret(a))), EditKind::Other);
                        doc.sync();
                        self.mode = Mode::Normal;
                        if let Some(r) = reg {
                            let text = if linewise && !r.linewise { format!("{}\n", r.text) } else { r.text };
                            let len = text.chars().count();
                            doc.buffer.apply(vec![Replace { range: a..a, text }], Some(Selections::single(Selection::caret(a + len.saturating_sub(1)))), EditKind::Other);
                        }
                        true
                    }
                    "r" => false,
                    s if s.starts_with('r') => {
                        let c = s.chars().nth(1).unwrap_or(' ');
                        let text: String = doc.buffer.slice(a..b).chars().map(|x| if x == '\n' { x } else { c }).collect();
                        doc.buffer.apply(vec![Replace { range: a..b, text }], Some(Selections::single(Selection::caret(a))), EditKind::Other);
                        finish(self, doc);
                        true
                    }
                    ":" => {
                        self.leave_visual(doc);
                        self.cmdline = Some((':', "'<,'>".into()));
                        self.marks.insert('<', a);
                        self.marks.insert('>', b.saturating_sub(1));
                        false
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------------ motions

    /// Where a motion goes from `pos`, and how its range counts.
    #[allow(clippy::too_many_arguments)]
    fn motion(&mut self, doc: &mut Document, view: &View, pos: usize, m: char, arg: Option<char>, count: Option<usize>, for_op: bool, fx: &mut Vec<Effect>) -> Option<(usize, Span)> {
        let n = doc.buffer.len();
        let c = count.unwrap_or(1).max(1);
        let line = line_of(doc, pos);
        let chars: Vec<char> = doc.chars().to_vec();
        let at = |p: usize| chars.get(p).copied().unwrap_or('\n');
        let vertical = |me: &mut Self, doc: &Document, target: usize| -> usize {
            let goal = *me.goal.get_or_insert_with(|| doc.buffer.col_of(pos));
            let t = target.min(last_line(doc));
            line_start(doc, t) + goal.min(doc.buffer.line_len(t).saturating_sub(usize::from(!for_op && doc.buffer.line_len(t) > 0)))
        };
        if !matches!(m, 'j' | 'k') && m != ctrl('d') && m != ctrl('u') && m != ctrl('f') && m != ctrl('b') {
            self.goal = None;
        }
        let r = match m {
            'h' | BACKSPACE => (pos.saturating_sub(c).max(line_start(doc, line)), Span::Exclusive),
            'l' | ' ' => {
                let lim = if for_op { line_end(doc, line) } else { line_end(doc, line).saturating_sub(1).max(line_start(doc, line)) };
                ((pos + c).min(lim), Span::Exclusive)
            }
            'j' => (vertical(self, doc, line + c), Span::Linewise),
            'k' => (vertical(self, doc, line.saturating_sub(c)), Span::Linewise),
            '+' | ENTER => (line_start(doc, (line + c).min(last_line(doc))), Span::Linewise),
            '-' => (line_start(doc, line.saturating_sub(c)), Span::Linewise),
            '_' => (first_non_blank(doc, (line + c - 1).min(last_line(doc))), Span::Linewise),
            '0' => (line_start(doc, line), Span::Exclusive),
            '^' => (first_non_blank(doc, line), Span::Exclusive),
            '$' => {
                let l = (line + c - 1).min(last_line(doc));
                let e = line_end(doc, l);
                (if e > line_start(doc, l) { e - 1 } else { e }, Span::Inclusive)
            }
            '|' => (line_start(doc, line) + (c - 1).min(doc.buffer.line_len(line)), Span::Exclusive),
            'G' => (line_start(doc, count.map_or(last_line(doc), |k| k.saturating_sub(1).min(last_line(doc)))), Span::Linewise),
            'g' => match arg {
                Some('g') => (line_start(doc, count.map_or(0, |k| k.saturating_sub(1).min(last_line(doc)))), Span::Linewise),
                Some('e') | Some('E') => {
                    let big = arg == Some('E');
                    let mut p = pos;
                    for _ in 0..c {
                        p = word_end_back(&chars, p, big);
                    }
                    (p, Span::Inclusive)
                }
                Some('_') => {
                    let l = (line + c - 1).min(last_line(doc));
                    let mut e = line_end(doc, l);
                    while e > line_start(doc, l) && at(e - 1).is_whitespace() {
                        e -= 1;
                    }
                    (e.saturating_sub(1).max(line_start(doc, l)), Span::Inclusive)
                }
                Some('0') => (line_start(doc, line), Span::Exclusive),
                _ => return None,
            },
            'w' | 'W' => {
                let mut p = pos;
                for _ in 0..c {
                    p = word_start_next(&chars, p, m == 'W');
                }
                (p.min(n), Span::Exclusive)
            }
            'b' | 'B' => {
                let mut p = pos;
                for _ in 0..c {
                    p = word_start_back(&chars, p, m == 'B');
                }
                (p, Span::Exclusive)
            }
            'e' | 'E' => {
                let mut p = pos;
                for _ in 0..c {
                    p = word_end_next(&chars, p, m == 'E');
                }
                (p.min(n.saturating_sub(1)), Span::Inclusive)
            }
            'f' | 'F' | 't' | 'T' => {
                let ch = arg?;
                self.last_find = Some((m, ch));
                (find_in_line(doc, &chars, pos, m, ch, c, false)?, if matches!(m, 'f' | 't') { Span::Inclusive } else { Span::Exclusive })
            }
            ';' | ',' => {
                let (k, ch) = self.last_find?;
                let k = if m == ',' {
                    match k {
                        'f' => 'F',
                        'F' => 'f',
                        't' => 'T',
                        _ => 't',
                    }
                } else {
                    k
                };
                (find_in_line(doc, &chars, pos, k, ch, c, true)?, if matches!(k, 'f' | 't') { Span::Inclusive } else { Span::Exclusive })
            }
            '%' => {
                if let Some(k) = count.filter(|_| count.is_some()) {
                    let l = (last_line(doc) * k.min(100)).div_ceil(100);
                    (first_non_blank(doc, l), Span::Linewise)
                } else {
                    (match_bracket(doc, pos)?, Span::Inclusive)
                }
            }
            '}' | '{' => {
                let mut l = line;
                for _ in 0..c {
                    if m == '}' {
                        while l < last_line(doc) && blank_line(doc, l) {
                            l += 1;
                        }
                        while l < last_line(doc) && !blank_line(doc, l) {
                            l += 1;
                        }
                    } else {
                        while l > 0 && blank_line(doc, l) {
                            l -= 1;
                        }
                        while l > 0 && !blank_line(doc, l) {
                            l -= 1;
                        }
                    }
                }
                let to = if m == '}' && l == last_line(doc) && !blank_line(doc, l) { line_end(doc, l) } else { line_start(doc, l) };
                (to, Span::Exclusive)
            }
            'H' => (line_start(doc, (view.first_line + c - 1).min(view.last_line)), Span::Linewise),
            'L' => (line_start(doc, view.last_line.saturating_sub(c - 1).max(view.first_line)), Span::Linewise),
            'M' => (line_start(doc, (view.first_line + view.last_line) / 2), Span::Linewise),
            '\'' | '`' => {
                let p = *self.marks.get(&arg?)?;
                let p = p.min(n);
                if m == '\'' { (first_non_blank(doc, line_of(doc, p)), Span::Linewise) } else { (p, Span::Exclusive) }
            }
            'n' | 'N' | '*' | '#' => {
                if matches!(m, '*' | '#') {
                    let word = doc.word_at(pos).map(|r| doc.buffer.slice(r))?;
                    self.search = Some((format!("\\<{word}\\>"), m == '*'));
                }
                let (pat, fwd) = self.search.clone()?;
                let fwd = if m == 'N' { !fwd } else { fwd };
                let mut p = pos;
                for _ in 0..c {
                    match search(doc, &pat, p, fwd) {
                        Some((q, wrapped)) => {
                            p = q;
                            if wrapped {
                                self.message = Some(if fwd { "search hit BOTTOM, continuing at TOP".into() } else { "search hit TOP, continuing at BOTTOM".into() });
                            }
                        }
                        None => {
                            self.message = Some(format!("Pattern not found: {}", pat.trim_start_matches("\\<").trim_end_matches("\\>")));
                            return None;
                        }
                    }
                }
                (p, Span::Exclusive)
            }
            k if k == ctrl('d') || k == ctrl('u') || k == ctrl('f') || k == ctrl('b') => {
                let page = view.last_line.saturating_sub(view.first_line).max(2);
                let step = if k == ctrl('d') || k == ctrl('u') { (page / 2).max(1) } else { page.saturating_sub(2).max(1) } * c.min(if k == ctrl('d') || k == ctrl('u') { 1 } else { c });
                let down = k == ctrl('d') || k == ctrl('f');
                fx.push(Effect::Scroll(ScrollTo::Lines(if down { step as isize } else { -(step as isize) })));
                let t = if down { line + step } else { line.saturating_sub(step) };
                (vertical(self, doc, t), Span::Linewise)
            }
            _ => return None,
        };
        Some(r)
    }

    // ------------------------------------------------------------------ text objects

    /// A text object around `pos`: (start, end exclusive, linewise).
    fn object(&self, doc: &mut Document, pos: usize, inner: bool, obj: char, count: usize) -> Option<(usize, usize, bool)> {
        let chars: Vec<char> = doc.chars().to_vec();
        match obj {
            'w' | 'W' => {
                let big = obj == 'W';
                let n = chars.len();
                if n == 0 {
                    return None;
                }
                let p = pos.min(n - 1);
                let cl = class(chars[p], big);
                let mut s = p;
                while s > 0 && class(chars[s - 1], big) == cl && chars[s - 1] != '\n' {
                    s -= 1;
                }
                let mut e = p;
                while e < n && class(chars[e], big) == cl && chars[e] != '\n' {
                    e += 1;
                }
                if !inner {
                    // The word and the blanks after it (or before it, if none follow).
                    let mut e2 = e;
                    while e2 < n && (chars[e2] == ' ' || chars[e2] == '\t') {
                        e2 += 1;
                    }
                    if e2 > e {
                        e = e2;
                    } else {
                        while s > 0 && (chars[s - 1] == ' ' || chars[s - 1] == '\t') {
                            s -= 1;
                        }
                    }
                }
                Some((s, e, false))
            }
            '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '<' | '>' => {
                let open = match obj {
                    '(' | ')' | 'b' => '(',
                    '[' | ']' => '[',
                    '<' | '>' => '<',
                    _ => '{',
                };
                let (o, c) = enclosing_bracket(doc, pos, open, count)?;
                if inner {
                    let (mut s, mut e) = (o + 1, c);
                    // Brackets on lines of their own: just the lines between them.
                    if chars.get(s) == Some(&'\n') {
                        s += 1;
                    }
                    if e > s && chars[..e].iter().rev().take_while(|c| **c == ' ' || **c == '\t').count() == e - line_start(doc, line_of(doc, e)) && line_of(doc, e) > line_of(doc, s) {
                        e = line_start(doc, line_of(doc, e));
                    }
                    Some((s, e.max(s), false))
                } else {
                    Some((o, c + 1, false))
                }
            }
            '"' | '\'' | '`' => {
                let line = line_of(doc, pos);
                let (ls, le) = (line_start(doc, line), line_end(doc, line));
                let qs: Vec<usize> = (ls..le).filter(|&i| chars[i] == obj && (i == 0 || chars[i - 1] != '\\')).collect();
                let pair = qs.chunks(2).find(|p| p.len() == 2 && p[0] <= pos && pos <= p[1]).or_else(|| qs.chunks(2).find(|p| p.len() == 2 && p[0] > pos))?;
                let (a, b) = (pair[0], pair[1]);
                if inner {
                    Some((a + 1, b, false))
                } else {
                    let mut e = b + 1;
                    while e < le && (chars[e] == ' ' || chars[e] == '\t') {
                        e += 1;
                    }
                    Some((a, e, false))
                }
            }
            'p' => {
                let line = line_of(doc, pos);
                let blank = blank_line(doc, line);
                let mut l1 = line;
                while l1 > 0 && blank_line(doc, l1 - 1) == blank {
                    l1 -= 1;
                }
                let mut l2 = line;
                while l2 < last_line(doc) && blank_line(doc, l2 + 1) == blank {
                    l2 += 1;
                }
                if !inner && !blank {
                    while l2 < last_line(doc) && blank_line(doc, l2 + 1) {
                        l2 += 1;
                    }
                }
                let end = if l2 < last_line(doc) { line_start(doc, l2 + 1) } else { doc.buffer.len() };
                Some((line_start(doc, l1), end, true))
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------ command line

    /// The `:` menu's suggestions for what's typed so far.
    pub fn ex_suggestions(&self) -> Vec<(&'static str, &'static str)> {
        let Some((':', text)) = &self.cmdline else { return Vec::new() };
        let t = text.trim().to_lowercase();
        EX_COMMANDS.iter().filter(|(c, _)| t.is_empty() || (c.to_lowercase().starts_with(&t) && c.to_lowercase() != t)).copied().collect()
    }

    fn cmdline_key(&mut self, doc: &mut Document, k: char, fx: &mut Vec<Effect>) {
        // Tab / ^N / ^P move through the `:` menu.
        if k == TAB || k == ctrl('n') || k == ctrl('p') {
            let n = self.ex_suggestions().len();
            if n > 0 {
                self.ex_selected = Some(match (self.ex_selected, k == ctrl('p')) {
                    (None, false) => 0,
                    (None, true) => n - 1,
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                });
            }
            return;
        }
        // Enter on a highlighted suggestion takes it (one that needs more typing just fills in).
        if (k == ENTER || k == '\n')
            && let Some(i) = self.ex_selected.take()
            && let Some((c, _)) = self.ex_suggestions().get(i).copied()
        {
            if c.ends_with('/') {
                self.cmdline = Some((':', c.to_string()));
                return;
            }
            self.cmdline = Some((':', c.to_string()));
        }
        let Some((kind, mut text)) = self.cmdline.take() else { return };
        if k != ENTER {
            self.ex_selected = None;
        }
        match k {
            ESC => {}
            BACKSPACE => {
                if text.pop().is_some() {
                    self.cmdline = Some((kind, text));
                }
            }
            ENTER | '\n' => match kind {
                ':' => self.ex(doc, text.trim(), fx),
                _ => {
                    let fwd = kind == '/';
                    if !text.is_empty() {
                        self.search = Some((text, fwd));
                    }
                    let pos = doc.buffer.selections.primary().head;
                    let view = View { first_line: 0, last_line: 0, auto_close: false, clipboard: &|| None };
                    if let Some((p, _)) = self.motion(doc, &view, pos, 'n', None, None, false, fx) {
                        doc.buffer.selections = Selections::single(Selection::caret(p));
                    }
                }
            },
            c if !c.is_control() => {
                text.push(c);
                self.cmdline = Some((kind, text));
            }
            _ => self.cmdline = Some((kind, text)),
        }
    }

    /// Ex commands: :w :q :q! :wq :x :{line} :$ :s/a/b/g :%s/a/b/g :noh.
    fn ex(&mut self, doc: &mut Document, cmd: &str, fx: &mut Vec<Effect>) {
        let (range, cmd) = if let Some(rest) = cmd.strip_prefix('%') {
            (Some((0, last_line(doc))), rest)
        } else if let Some(rest) = cmd.strip_prefix("'<,'>") {
            let a = self.marks.get(&'<').map(|p| line_of(doc, *p)).unwrap_or(0);
            let b = self.marks.get(&'>').map(|p| line_of(doc, *p)).unwrap_or(a);
            (Some((a, b)), rest)
        } else {
            (None, cmd)
        };
        match cmd {
            "w" | "write" => fx.push(Effect::Save),
            "q" | "quit" | "close" => fx.push(Effect::Close { force: false }),
            "q!" | "quit!" => fx.push(Effect::Close { force: true }),
            "wq" | "x" | "xit" | "exit" => {
                fx.push(Effect::Save);
                fx.push(Effect::Close { force: false });
            }
            "noh" | "nohlsearch" | "" => {}
            "$" => {
                let p = first_non_blank(doc, last_line(doc));
                doc.buffer.selections = Selections::single(Selection::caret(p));
            }
            c if c.chars().all(|c| c.is_ascii_digit()) => {
                let l = c.parse::<usize>().unwrap_or(1).saturating_sub(1).min(last_line(doc));
                let p = first_non_blank(doc, l);
                doc.buffer.selections = Selections::single(Selection::caret(p));
            }
            c if c.starts_with("s/") || c.starts_with("s#") => {
                let sep = c.chars().nth(1).unwrap_or('/');
                let parts: Vec<&str> = c[2..].split(sep).collect();
                if parts.len() < 2 {
                    self.message = Some("E486: Pattern not found".into());
                    return;
                }
                let (pat, rep, flags) = (parts[0], parts[1], parts.get(2).copied().unwrap_or(""));
                let line = line_of(doc, doc.buffer.selections.primary().head);
                let (l1, l2) = range.unwrap_or((line, line));
                self.substitute(doc, l1, l2, pat, rep, flags.contains('g'));
            }
            other => match ex_app(other) {
                Some(name) => fx.push(Effect::App(name)),
                None => self.message = Some(format!("E492: Not an editor command: {other}")),
            },
        }
    }

    fn substitute(&mut self, doc: &mut Document, l1: usize, l2: usize, pat: &str, rep: &str, global: bool) {
        if pat.is_empty() {
            return;
        }
        let mut edits = Vec::new();
        let pat_c: Vec<char> = pat.chars().collect();
        for l in l1..=l2.min(last_line(doc)) {
            let ls = line_start(doc, l);
            let text: Vec<char> = doc.buffer.line_text(l).chars().collect();
            let mut i = 0;
            while i + pat_c.len() <= text.len() {
                if text[i..i + pat_c.len()] == pat_c[..] {
                    edits.push(Replace { range: ls + i..ls + i + pat_c.len(), text: rep.to_string() });
                    i += pat_c.len();
                    if !global {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        }
        if edits.is_empty() {
            self.message = Some(format!("E486: Pattern not found: {pat}"));
            return;
        }
        let n = edits.len();
        let at = edits[0].range.start;
        doc.buffer.begin_group();
        doc.buffer.apply(edits, Some(Selections::single(Selection::caret(at))), EditKind::Other);
        doc.buffer.end_group();
        self.message = Some(format!("{n} substitution{}", if n == 1 { "" } else { "s" }));
    }
}

/// Motions and text objects at the start of `k` (`w`, `fx`, `gg`, `iw`, `'a`).
fn motion_or_object(k: &[char], objects: bool) -> Parse {
    let done = |action| Parse::Done(Cmd { reg: None, count: None, op: None, action });
    match k {
        [] => Parse::Incomplete,
        [c, ..] if "hjklwWbBeE0^$GHLM%{};,|+-_nN*#".contains(*c) || *c == ENTER || *c == BACKSPACE => {
            if k.len() == 1 { done(Action::Motion(*c, None)) } else { Parse::Invalid }
        }
        [c] if *c == ctrl('d') || *c == ctrl('u') || *c == ctrl('f') || *c == ctrl('b') => done(Action::Motion(*c, None)),
        ['f' | 'F' | 't' | 'T' | '\'' | '`' | 'g'] => Parse::Incomplete,
        [c, x] if matches!(c, 'f' | 'F' | 't' | 'T') && *x != ESC => done(Action::Motion(*c, Some(*x))),
        [c, x] if matches!(c, '\'' | '`') && (x.is_ascii_alphabetic() || matches!(x, '<' | '>' | '^')) => done(Action::Motion(*c, Some(*x))),
        ['g', x] if matches!(x, 'g' | 'e' | 'E' | '_' | '0') => done(Action::Motion('g', Some(*x))),
        [c, ..] if objects && matches!(c, 'i' | 'a') => match k.get(1) {
            None => Parse::Incomplete,
            Some(o) if "wWpbB()[]{}<>\"'`".contains(*o) => done(Action::Object(*c == 'i', *o)),
            _ => Parse::Invalid,
        },
        _ => Parse::Invalid,
    }
}

// ------------------------------------------------------------------ word motions

fn word_start_next(chars: &[char], pos: usize, big: bool) -> usize {
    let n = chars.len();
    let mut p = pos;
    if p >= n {
        return n;
    }
    let cl = class(chars[p], big);
    if cl != 0 {
        while p < n && class(chars[p], big) == cl {
            p += 1;
        }
    }
    // Skip blanks; an empty line is a word of its own.
    while p < n && chars[p].is_whitespace() {
        if chars[p] == '\n' && p + 1 < n && chars[p + 1] == '\n' {
            return p + 1;
        }
        p += 1;
    }
    p
}

fn word_start_back(chars: &[char], pos: usize, big: bool) -> usize {
    let mut p = pos;
    while p > 0 && chars[p - 1].is_whitespace() {
        if chars[p - 1] == '\n' && p >= 2 && chars[p - 2] == '\n' && p - 1 != pos {
            return p - 1;
        }
        p -= 1;
    }
    if p == 0 {
        return 0;
    }
    let cl = class(chars[p - 1], big);
    while p > 0 && class(chars[p - 1], big) == cl {
        p -= 1;
    }
    p
}

fn word_end_next(chars: &[char], pos: usize, big: bool) -> usize {
    let n = chars.len();
    let mut p = pos + 1;
    while p < n && chars[p].is_whitespace() {
        p += 1;
    }
    if p >= n {
        return n.saturating_sub(1);
    }
    let cl = class(chars[p], big);
    while p + 1 < n && class(chars[p + 1], big) == cl {
        p += 1;
    }
    p
}

fn word_end_back(chars: &[char], pos: usize, big: bool) -> usize {
    let mut p = pos;
    if p < chars.len() {
        let cl = class(chars[p], big);
        if cl != 0 {
            while p > 0 && class(chars[p - 1], big) == cl {
                p -= 1;
            }
        }
    }
    while p > 0 && chars[p - 1].is_whitespace() {
        p -= 1;
    }
    p.saturating_sub(1)
}

/// f / F / t / T within the line. `repeat`: ; and , skip a character the cursor is already
/// up against (so `t)` then `;` moves on).
fn find_in_line(doc: &Document, chars: &[char], pos: usize, k: char, ch: char, count: usize, repeat: bool) -> Option<usize> {
    let line = line_of(doc, pos);
    let (ls, le) = (line_start(doc, line), line_end(doc, line));
    let mut p = pos;
    let fwd = matches!(k, 'f' | 't');
    let till = matches!(k, 't' | 'T');
    for i in 0..count {
        let skip = usize::from(till && (repeat || i > 0));
        p = if fwd { (p + 1 + skip..le).find(|&q| chars[q] == ch)? } else { (ls..p.saturating_sub(skip)).rev().find(|&q| chars[q] == ch)? };
    }
    Some(match k {
        't' => p - 1,
        'T' => p + 1,
        _ => p,
    })
}

/// `%`: the bracket matching the one at (or next on the line after) the cursor.
fn match_bracket(doc: &mut Document, pos: usize) -> Option<usize> {
    let line = line_of(doc, pos);
    let le = line_end(doc, line);
    let toks = doc.tokens().to_vec();
    let mut i = lexer::token_index_at(&toks, pos);
    while let Some(t) = toks.get(i) {
        if t.start >= le {
            return None;
        }
        if matches!(t.kind, Kind::Open | Kind::Close)
            && let Some(p) = t.partner
        {
            let o = &toks[p as usize];
            return Some(if o.kind == Kind::Open { o.end - 1 } else { o.start });
        }
        i += 1;
    }
    None
}

/// The `count`-th enclosing bracket pair of this kind around `pos`: (open, close) positions.
fn enclosing_bracket(doc: &mut Document, pos: usize, open: char, count: usize) -> Option<(usize, usize)> {
    let toks = doc.tokens().to_vec();
    let chars = doc.chars().to_vec();
    let mut found: Vec<(usize, usize)> = toks
        .iter()
        .filter(|t| t.kind == Kind::Open && chars.get(t.end - 1) == Some(&open))
        .filter_map(|t| {
            let c = toks.get(t.partner? as usize)?;
            let (o, cl) = (t.end - 1, c.start);
            (o <= pos && pos <= cl).then_some((o, cl))
        })
        .collect();
    // Innermost first.
    found.sort_by_key(|(o, _)| std::cmp::Reverse(*o));
    found.get(count.saturating_sub(1)).copied()
}

/// Literal search (`\<word\>` for whole words; lowercase patterns ignore case). Returns the
/// match and whether it wrapped around the end.
fn search(doc: &Document, pat: &str, from: usize, forward: bool) -> Option<(usize, bool)> {
    let (needle, whole) = match pat.strip_prefix("\\<").and_then(|p| p.strip_suffix("\\>")) {
        Some(w) => (w, true),
        None => (pat, false),
    };
    let ignore_case = !needle.chars().any(char::is_uppercase);
    let norm = |c: char| if ignore_case { c.to_lowercase().next().unwrap_or(c) } else { c };
    let hay: Vec<char> = doc.chars().iter().map(|c| norm(*c)).collect();
    let nd: Vec<char> = needle.chars().map(norm).collect();
    if nd.is_empty() || nd.len() > hay.len() {
        return None;
    }
    let is_match = |i: usize| {
        hay[i..i + nd.len()] == nd[..] && (!whole || ((i == 0 || lexer::is_delimiter(hay[i - 1])) && (i + nd.len() >= hay.len() || lexer::is_delimiter(hay[i + nd.len()]))))
    };
    let last = hay.len() - nd.len();
    if forward {
        (from + 1..=last).find(|&i| is_match(i)).map(|i| (i, false)).or_else(|| (0..=from.min(last)).find(|&i| is_match(i)).map(|i| (i, true)))
    } else {
        (0..from.min(last + 1)).rev().find(|&i| is_match(i)).map(|i| (i, false)).or_else(|| (from..=last).rev().find(|&i| is_match(i)).map(|i| (i, true)))
    }
}

/// How a key shows in the pending-command display.
fn display_key(c: char) -> String {
    match c {
        ESC => "<Esc>".into(),
        ENTER => "<CR>".into(),
        BACKSPACE => "<BS>".into(),
        TAB => "<Tab>".into(),
        ' ' => "␣".into(),
        c if (c as u32) < 0x20 => format!("^{}", ((c as u8) + b'@') as char),
        c => c.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run keys (as a string, `<Esc>` style escapes allowed) on `text` with the cursor at `at`;
    /// return the text and the cursor.
    fn run(text: &str, at: usize, keys: &str) -> (String, usize) {
        let mut doc = Document::new(text);
        doc.buffer.selections = Selections::single(Selection::caret(at));
        let mut vim = Vim::new();
        feed(&mut vim, &mut doc, keys);
        (doc.buffer.text(), doc.buffer.selections.primary().head)
    }

    fn feed(vim: &mut Vim, doc: &mut Document, keys: &str) {
        let clip = || None;
        let view = View { first_line: 0, last_line: 40, auto_close: false, clipboard: &clip };
        let mut keys = keys.replace("<Esc>", "\x1b").replace("<CR>", "\r").replace("<BS>", "\x08");
        for c in 'a'..='z' {
            keys = keys.replace(&format!("<C-{c}>"), &ctrl(c).to_string());
        }
        for k in keys.chars() {
            if matches!(vim.mode, Mode::Insert | Mode::Replace) && k != ESC {
                // Live typing goes through the editor; replay through Vim.
                vim.note_insert(k);
                vim.insert_key(doc, &view, k);
                doc.sync();
            } else {
                vim.feed(doc, &view, k);
            }
        }
    }

    #[test]
    fn words_follow_racket_identifiers() {
        assert_eq!(run("(make-node k empty)", 1, "w").1, 11);
        assert_eq!(run("(make-node k empty)", 1, "e").1, 9);
        assert_eq!(run("(make-node k empty)", 11, "b").1, 1);
        assert_eq!(run("(a b)\n\n(c)", 0, "3w").1, 4, "a closing bracket is a word of its own");
        assert_eq!(run("(a b)\n\n(c)", 0, "4w").1, 6, "an empty line is a word");
    }

    #[test]
    fn operators_with_motions_and_counts() {
        assert_eq!(run("one two three", 0, "dw").0, "two three");
        assert_eq!(run("one two three", 0, "d2w").0, "three");
        assert_eq!(run("one two three", 0, "2dw").0, "three");
        assert_eq!(run("one two three", 0, "cwuno<Esc>").0, "uno two three");
        assert_eq!(run("one two three", 4, "d$").0, "one ");
        assert_eq!(run("(f x) ; c", 0, "dt;").0, "; c");
        assert_eq!(run("(f x) ; c", 0, "df)").0, " ; c");
        assert_eq!(run("a\nb\nc\nd", 0, "2dd").0, "c\nd");
        assert_eq!(run("a\nb\nc", 4, "dd").0, "a\nb", "the last line takes its newline");
        assert_eq!(run("a\nb\nc", 0, "dj").0, "c");
        assert_eq!(run("a\nb\nc", 0, "dG").0, "");
    }

    #[test]
    fn s_expression_text_objects() {
        let src = "(define (f x) (+ x 1))";
        let at = src.find("+ x").unwrap() + 2;
        assert_eq!(run(src, at, "di(").0, "(define (f x) ())");
        assert_eq!(run(src, at, "da(").0, "(define (f x) )");
        assert_eq!(run(src, at, "ci(* x 2<Esc>").0, "(define (f x) (* x 2))");
        assert_eq!(run(src, at, "d2i(").0, "()", "the second enclosing pair");
        assert_eq!(run("(cond [a 1] [b 2])", 8, "di[").0, "(cond [] [b 2])");
        assert_eq!(run("(x \"hi there\")", 6, "ci\"bye<Esc>").0, "(x \"bye\")");
        assert_eq!(run("(make-node k)", 3, "diw").0, "( k)");
        assert_eq!(run("a b c", 2, "daw").0, "a c");
    }

    #[test]
    fn yank_put_undo_redo_and_dot() {
        assert_eq!(run("a\nb", 0, "yyp").0, "a\na\nb");
        assert_eq!(run("a\nb", 0, "yyP").0, "a\na\nb");
        assert_eq!(run("a\nb", 2, "yyp").0, "a\nb\nb");
        assert_eq!(run("ab", 0, "xp").0, "ba");
        assert_eq!(run("one two", 0, "dwu").0, "one two");
        assert_eq!(run("one two", 0, "dwu<C-r>").0, "two");
        assert_eq!(run("a b c d", 0, "dw..").0, "d");
        assert_eq!(run("x\ny", 0, "Ahi<Esc>j.").0, "xhi\nyhi");
        assert_eq!(run("a\nb", 0, "\"ayyj\"ap").0, "a\nb\na");
    }

    #[test]
    fn inserting_joining_and_small_edits() {
        assert_eq!(run("b", 0, "ia<Esc>").0, "ab");
        assert_eq!(run("a", 0, "ab<Esc>").0, "ab");
        assert_eq!(run("  x", 2, "Iy<Esc>").0, "  yx");
        assert_eq!(run("x", 0, "oy<Esc>").0, "x\ny");
        assert_eq!(run("a\n  b", 0, "J").0, "a b");
        assert_eq!(run("abc", 0, "rz").0, "zbc");
        assert_eq!(run("abc", 0, "~~").0, "ABc");
        assert_eq!(run("x 9 y", 0, "<C-a>").0, "x 10 y");
        assert_eq!(run("abcd", 1, "Rxy<Esc>").0, "axyd");
        let (t, c) = run("(a)", 0, "ia<Esc>");
        assert_eq!((t.as_str(), c), ("a(a)", 0), "Escape steps back onto the typed character");
    }

    #[test]
    fn selections_made_outside_vim_are_visual() {
        // Cmd+A, then y: the whole file is yanked.
        let mut doc = Document::new("(a)\n(b)");
        let mut vim = Vim::default();
        doc.buffer.selections = Selections::single(Selection::new(0, 7));
        let clip = || None;
        let view = View { first_line: 0, last_line: 10, auto_close: false, clipboard: &clip };
        assert_eq!(vim.feed(&mut doc, &view, 'y'), vec![Effect::Clipboard("(a)\n(b)".into())]);
        assert_eq!(vim.mode, Mode::Normal);
        doc.buffer.selections = Selections::single(Selection::new(0, 3));
        vim.feed(&mut doc, &view, 'd');
        assert_eq!(doc.text(), "\n(b)");
    }

    #[test]
    fn insert_mode_control_keys() {
        assert_eq!(run("  ab", 4, "a<C-u><Esc>").0, "  ", "^U deletes back to the indent");
        assert_eq!(run("one two", 6, "a<C-w><Esc>").0, "one ");
        assert_eq!(run("ab", 1, "a<C-h><Esc>").0, "a");
        assert_eq!(run("x", 0, "i<C-t><Esc>").0, "  x");
        assert_eq!(run("    x", 4, "i<C-d><Esc>").0, "  x");
        assert_eq!(run("abc\nd", 4, "a<C-y><C-y><Esc>").0, "abc\ndbc", "^Y copies from the line above");
        assert_eq!(run("hey x", 0, "yiwA <C-r>\"<Esc>").0, "hey x hey");
        // ^O: one normal command (here `0`), then still inserting.
        assert_eq!(run("bc", 1, "a<C-o>0a<Esc>").0, "abc");
        // A change with insert-mode keys repeats with `.`.
        assert_eq!(run("a\nb", 0, "A1<C-h>2<Esc>j.").0, "a2\nb2");
    }

    #[test]
    fn visual_mode() {
        assert_eq!(run("one two", 0, "vwd").0, "wo", "v is inclusive of the cursor");
        assert_eq!(run("a\nb\nc", 0, "Vjd").0, "c");
        assert_eq!(run("one two", 0, "veU").0, "ONE two");
        assert_eq!(run("(a (b c))", 5, "vi(d").0, "(a ())");
        assert_eq!(run("a\nb", 0, "Vy").0, "a\nb");
    }

    #[test]
    fn search_marks_brackets_and_ex() {
        let (_, c) = run("(f x) (g x) (h x)", 0, "/x<CR>n");
        assert_eq!(c, 9);
        assert_eq!(run("(f x) (g x)", 0, "%").1, 4);
        assert_eq!(run("a\nb\nc", 4, "majjd'a").0, "a\nb", "lines from the mark");
        assert_eq!(run("a a\na", 0, ":%s/a/b/g<CR>").0, "b b\nb");
        assert_eq!(run("a\nb\nc", 0, ":3<CR>").1, 4);
        let mut doc = Document::new("x");
        let mut vim = Vim::new();
        let clip = || None;
        let view = View { first_line: 0, last_line: 10, auto_close: false, clipboard: &clip };
        let mut fx = Vec::new();
        for k in ":wq\r".chars() {
            fx.extend(vim.feed(&mut doc, &view, k));
        }
        assert_eq!(fx, vec![Effect::Save, Effect::Close { force: false }]);
    }

    #[test]
    fn leader_keys_and_which_key() {
        let mut doc = Document::new("(f x)\n(g y)");
        let mut vim = Vim::new();
        let clip = || None;
        let view = View { first_line: 0, last_line: 10, auto_close: false, clipboard: &clip };
        assert!(vim.feed(&mut doc, &view, ' ').is_empty());
        let (typed, items) = vim.which_key().unwrap();
        assert_eq!(typed, "");
        assert!(items.contains(&("e".into(), "Explorer", false)));
        assert!(items.contains(&("f".into(), "+file", true)));
        assert_eq!(vim.feed(&mut doc, &view, 'e'), vec![Effect::App("ToggleLeftPanel")]);
        assert!(vim.which_key().is_none());
        for k in " r".chars() {
            vim.feed(&mut doc, &view, k);
        }
        assert!(vim.which_key().unwrap().1.iter().any(|(k, d, _)| k == "r" && *d == "Run"));
        assert_eq!(vim.feed(&mut doc, &view, 'r'), vec![Effect::App("Run")]);
        // gcc comments the line; `:` offers the app's commands too.
        assert_eq!(run("(f x)", 0, "gcc").0, "; (f x)");
        let mut vim = Vim::new();
        for k in ":vi".chars() {
            vim.feed(&mut doc, &view, k);
        }
        assert_eq!(vim.ex_suggestions().first().map(|s| s.0), Some("Visualize"));
        let mut fx = vim.feed(&mut doc, &view, TAB);
        fx.extend(vim.feed(&mut doc, &view, ENTER));
        assert_eq!(fx, vec![Effect::App("Visualize")]);
    }

    #[test]
    fn macros() {
        assert_eq!(run("a\nb\nc", 0, "qqA!<Esc>jq2@q").0, "a!\nb!\nc!");
    }

    #[test]
    fn yanks_reach_the_clipboard() {
        let mut doc = Document::new("hello world");
        let mut vim = Vim::new();
        let clip = || None;
        let view = View { first_line: 0, last_line: 10, auto_close: false, clipboard: &clip };
        let mut fx = Vec::new();
        for k in "yw".chars() {
            fx.extend(vim.feed(&mut doc, &view, k));
        }
        assert_eq!(fx, vec![Effect::Clipboard("hello ".into())]);
        // A clipboard changed elsewhere is what p puts.
        let other = || Some("X".to_string());
        let view = View { first_line: 0, last_line: 10, auto_close: false, clipboard: &other };
        vim.feed(&mut doc, &view, 'p');
        assert_eq!(doc.buffer.text(), "hXello world");
    }
}
