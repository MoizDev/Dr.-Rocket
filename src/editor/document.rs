//! `Document`: a buffer plus its derived syntax (tokens), and every editing command.
//! Commands are pure buffer operations (no UI), so they are unit-testable.

use super::buffer::{Buffer, EditKind, Replace, Selection, Selections};
use super::lexer::{self, Class, Kind, Token, closer_for, is_delimiter};
use super::sexp::{self, Lines};
use std::ops::Range;

pub struct Document {
    pub buffer: Buffer,
    tokens: std::sync::Arc<Vec<Token>>,
    chars: Vec<char>,
    synced_version: u64,
    /// Stack of selections for "shrink selection" after "expand selection".
    expand_stack: Vec<Selections>,
    /// Edits applied since the last `take_edit_log`, for shifting decorations.
    edit_log: Vec<(Range<usize>, usize)>,
    /// Outline cached per buffer version (the status bar asks every frame).
    outline_cache: Option<(u64, std::sync::Arc<Vec<OutlineItem>>)>,
    /// Images embedded in the text (DrRacket's pasted images).
    pub images: super::images::ImageStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    SexpLeft,
    SexpRight,
    UpSexp,
    DownSexp,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
}

impl Lines for Buffer {
    fn line_of(&self, pos: usize) -> usize {
        Buffer::line_of(self, pos)
    }
}

/// How typing treats brackets and quotes (Settings > Editor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Typing {
    /// An opening bracket or quote also types its closer (or wraps the selection), and typing a
    /// closer over the same closer steps past it.
    pub auto_close: bool,
    /// A typed closing bracket becomes the one that matches the innermost open bracket, as in
    /// DrRacket: `)` after `[` types `]`.
    pub match_closers: bool,
}

impl Typing {
    /// Types exactly the given text.
    pub const LITERAL: Typing = Typing { auto_close: false, match_closers: false };
}

impl Default for Typing {
    fn default() -> Self {
        Typing { auto_close: true, match_closers: true }
    }
}

impl Document {
    pub fn new(text: &str) -> Self {
        let mut d = Self { buffer: Buffer::new(text), tokens: Default::default(), chars: Vec::new(), synced_version: u64::MAX, expand_stack: Vec::new(), edit_log: Vec::new(), outline_cache: None, images: Default::default() };
        d.sync();
        d.buffer.take_changes();
        d
    }

    /// Bring tokens up to date with the buffer. Incremental: only the edited region is
    /// re-lexed (about 0.2 ms per keystroke on a 20k-line file).
    pub fn sync(&mut self) {
        if self.synced_version == self.buffer.version() {
            return;
        }
        let changes = self.buffer.take_changes();
        self.edit_log.extend(changes.iter().cloned());
        let incremental = self.synced_version != u64::MAX && !changes.is_empty() && changes.len() <= 16;
        if !incremental {
            self.chars = self.buffer.rope().chars().collect();
            self.tokens = std::sync::Arc::new(lexer::lex_chars(&self.chars));
            self.synced_version = self.buffer.version();
            return;
        }
        // Apply the changes to the char cache and compute the dirty region in new coordinates.
        let old_len = self.chars.len();
        let (mut lo, mut hi) = (usize::MAX, 0usize);
        for (range, inserted) in &changes {
            let removed = range.end - range.start;
            let at = range.start;
            // The rope is already in its final state, so slicing it is only valid for a single
            // change; multiple changes rebuild the cache below.
            if changes.len() == 1 {
                let text: Vec<char> = self.buffer.rope().slice(at..at + inserted).chars().collect();
                self.chars.splice(at..at + removed, text);
            }
            if lo != usize::MAX {
                let map = |p: usize, end_side: bool| {
                    if p >= range.end {
                        p + inserted - removed
                    } else if p > range.start {
                        if end_side { range.start + inserted } else { range.start }
                    } else {
                        p
                    }
                };
                lo = map(lo, false);
                hi = map(hi, true);
            }
            lo = lo.min(at);
            hi = hi.max(at + inserted);
        }
        if changes.len() > 1 {
            self.chars = self.buffer.rope().chars().collect();
        }
        let delta = self.chars.len() as isize - old_len as isize;
        lexer::relex(std::sync::Arc::make_mut(&mut self.tokens), &self.chars, lo, hi, delta);
        self.synced_version = self.buffer.version();
    }

    /// Edits since the last call: (range in pre-edit coordinates, inserted length), in order.
    pub fn take_edit_log(&mut self) -> Vec<(Range<usize>, usize)> {
        self.sync();
        std::mem::take(&mut self.edit_log)
    }

    pub fn tokens(&mut self) -> &[Token] {
        self.sync();
        &self.tokens
    }
    /// Tokens without re-syncing (caller guarantees `sync` was called this frame).
    pub fn tokens_cached(&self) -> &[Token] {
        &self.tokens
    }

    /// A cheap shared handle to the current tokens (no copy).
    pub fn tokens_shared(&mut self) -> std::sync::Arc<Vec<Token>> {
        self.sync();
        self.tokens.clone()
    }
    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }
    pub fn version(&self) -> u64 {
        self.buffer.version()
    }

    /// Is `pos` inside a string or comment (where Racket structure editing is disabled)?
    pub fn in_string_or_comment(&mut self, pos: usize) -> bool {
        self.sync();
        lexer::token_at(&self.tokens, pos).is_some_and(|(_, t)| {
            t.start < pos
                && (matches!(t.kind, Kind::String | Kind::StringEscape | Kind::ByteString | Kind::HereString | Kind::Regex | Kind::BlockComment | Kind::LineComment)
                    || t.kind == Kind::Char)
        }) || self.line_comment_before(pos)
    }

    fn line_comment_before(&self, pos: usize) -> bool {
        // A line comment ending exactly at `pos` (cursor at end of a comment line).
        let i = lexer::token_index_at(&self.tokens, pos);
        i > 0 && self.tokens[i - 1].kind == Kind::LineComment && self.tokens[i - 1].end == pos
    }

    /// Range of the identifier/word at `pos` (for double click, find-selection, rename).
    pub fn word_at(&mut self, pos: usize) -> Option<Range<usize>> {
        self.sync();
        let i = lexer::token_index_at(&self.tokens, pos);
        let pick = |t: &Token| matches!(t.kind, Kind::Symbol | Kind::Number | Kind::Boolean | Kind::HashKeyword | Kind::Char | Kind::LangName);
        if let Some(t) = self.tokens.get(i)
            && t.start <= pos
            && pick(t)
        {
            return Some(t.start..t.end);
        }
        if i > 0 && self.tokens[i - 1].end == pos && pick(&self.tokens[i - 1]) {
            return Some(self.tokens[i - 1].start..self.tokens[i - 1].end);
        }
        // Inside strings/comments: alphanumeric words.
        let is_w = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
        let n = self.chars.len();
        if pos > n {
            return None;
        }
        let mut s = pos;
        while s > 0 && is_w(self.chars[s - 1]) {
            s -= 1;
        }
        let mut e = pos;
        while e < n && is_w(self.chars[e]) {
            e += 1;
        }
        (s < e).then_some(s..e)
    }

    /// Identifier token (symbol) at `pos`, if any.
    pub fn symbol_at(&mut self, pos: usize) -> Option<(Range<usize>, String)> {
        self.sync();
        let i = lexer::token_index_at(&self.tokens, pos);
        let cand = [self.tokens.get(i).filter(|t| t.start <= pos), i.checked_sub(1).and_then(|j| self.tokens.get(j)).filter(|t| t.end == pos)];
        cand.into_iter().flatten().find(|t| t.kind == Kind::Symbol).map(|t| (t.start..t.end, self.chars[t.start..t.end].iter().collect()))
    }

    // ------------------------------------------------------------------ motions

    fn word_boundary(&self, pos: usize, forward: bool) -> usize {
        let n = self.chars.len();
        let is_w = |c: char| !is_delimiter(c) && c != '#';
        let mut p = pos;
        if forward {
            while p < n && !is_w(self.chars[p]) {
                p += 1;
            }
            while p < n && is_w(self.chars[p]) {
                p += 1;
            }
        } else {
            while p > 0 && !is_w(self.chars[p - 1]) {
                p -= 1;
            }
            while p > 0 && is_w(self.chars[p - 1]) {
                p -= 1;
            }
        }
        p
    }

    pub fn motion_target(&mut self, pos: usize, m: Motion) -> usize {
        self.sync();
        let b = &self.buffer;
        let n = b.len();
        match m {
            Motion::Left => pos.saturating_sub(1),
            Motion::Right => (pos + 1).min(n),
            Motion::WordLeft => self.word_boundary(pos, false),
            Motion::WordRight => self.word_boundary(pos, true),
            Motion::SexpLeft => sexp::backward_sexp(&self.tokens, pos).unwrap_or_else(|| sexp::up_sexp(&self.tokens, pos).unwrap_or(0)),
            Motion::SexpRight => sexp::forward_sexp(&self.tokens, pos).unwrap_or_else(|| sexp::forward_up_sexp(&self.tokens, pos).unwrap_or(n)),
            Motion::UpSexp => sexp::up_sexp(&self.tokens, pos).unwrap_or(pos),
            Motion::DownSexp => sexp::down_sexp(&self.tokens, pos).unwrap_or(pos),
            Motion::LineStart => {
                // Smart home: toggle between first non-blank and column 0.
                let line = b.line_of(pos);
                let ls = b.line_start(line);
                let first = ls + b.indentation(line);
                if pos == first { ls } else { first }
            }
            Motion::LineEnd => b.line_end(b.line_of(pos)),
            Motion::DocStart => 0,
            Motion::DocEnd => n,
        }
    }

    /// Move (or extend) every selection by a motion.
    pub fn move_selections(&mut self, m: Motion, extend: bool) {
        let mut sels = self.buffer.selections.clone();
        for s in sels.list.iter_mut() {
            let target = if !extend && !s.is_empty() && matches!(m, Motion::Left | Motion::Right) {
                if m == Motion::Left { s.start() } else { s.end() }
            } else {
                self.motion_target(s.head, m)
            };
            *s = if extend { Selection::new(s.anchor, target) } else { Selection::caret(target) };
        }
        sels.normalize();
        self.buffer.selections = sels;
        self.buffer.break_undo_group();
        self.expand_stack.clear();
    }

    pub fn select_all(&mut self) {
        self.buffer.selections = Selections::single(Selection::new(0, self.buffer.len()));
    }

    pub fn expand_selection(&mut self) {
        self.sync();
        let mut sels = self.buffer.selections.clone();
        let mut changed = false;
        for s in sels.list.iter_mut() {
            if let Some((a, b)) = sexp::expand_selection(&self.tokens, s.start(), s.end()) {
                *s = Selection::new(a, b);
                changed = true;
            }
        }
        if changed {
            self.expand_stack.push(self.buffer.selections.clone());
            sels.normalize();
            self.buffer.selections = sels;
        }
    }

    pub fn shrink_selection(&mut self) {
        if let Some(prev) = self.expand_stack.pop() {
            self.buffer.selections = prev;
        }
    }

    // ------------------------------------------------------------------ editing

    /// Type text at every cursor, with Racket-aware bracket handling.
    pub fn type_text(&mut self, text: &str, typing: Typing) {
        let auto_close = typing.auto_close;
        self.sync();
        self.expand_stack.clear();
        let mut chars = text.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        let Some(c) = single else {
            self.buffer.insert_at_selections(text, EditKind::Insert);
            return;
        };
        let sels = self.buffer.selections.clone();
        let mut edits = Vec::new();
        // What's selected afterwards: a caret, or the wrapped text (still selected, so typing
        // again replaces it or wraps it once more).
        let mut after = Vec::new();
        // Track how positions shift as we build edits front-to-back.
        let mut shift: isize = 0;
        for s in &sels.list {
            let (start, end) = (s.start(), s.end());
            let in_str = self.in_string_or_comment(start);
            let next = self.buffer.char_at(end);
            let (repl, select): (String, Range<usize>) = match c {
                '(' | '[' | '{' if auto_close && !in_str && !s.is_empty() => {
                    // Wrap the selection.
                    let inner = self.buffer.slice(start..end);
                    let n = inner.chars().count();
                    (format!("{c}{inner}{}", closer_for(c)), 1..n + 1)
                }
                '(' | '[' | '{' if auto_close && !in_str && next.is_none_or(|n| n.is_whitespace() || matches!(n, ')' | ']' | '}')) => {
                    (format!("{c}{}", closer_for(c)), 1..1)
                }
                ')' | ']' | '}' if !in_str && s.is_empty() && (auto_close || typing.match_closers) => {
                    // DrRacket: insert the closer that matches the innermost open bracket, and
                    // step over an existing identical closer.
                    let opener = sexp::enclosing_open(&self.tokens, start).filter(|_| typing.match_closers);
                    let want = opener.map_or(c, |o| closer_for(self.chars[self.tokens[o].end - 1]));
                    if next == Some(want) {
                        edits.push(Replace { range: start..start, text: String::new() });
                        after.push(Selection::caret(((start as isize + shift) as usize) + 1));
                        continue;
                    }
                    (want.to_string(), 1..1)
                }
                '"' if auto_close && s.is_empty() => {
                    let in_string_tok = lexer::token_at(&self.tokens, start).is_some_and(|(_, t)| t.start < start && matches!(t.kind, Kind::String | Kind::StringEscape));
                    if next == Some('"') && (in_string_tok || self.prev_char(start) != Some('\\') && self.string_ends_at(start + 1)) {
                        edits.push(Replace { range: start..start, text: String::new() });
                        after.push(Selection::caret(((start as isize + shift) as usize) + 1));
                        continue;
                    }
                    if in_str {
                        ("\"".to_string(), 1..1)
                    } else if next.is_none_or(is_delimiter) && self.prev_char(start) != Some('\\') {
                        ("\"\"".to_string(), 1..1)
                    } else {
                        ("\"".to_string(), 1..1)
                    }
                }
                '"' if auto_close && !s.is_empty() && !in_str => {
                    let inner = self.buffer.slice(start..end);
                    let n = inner.chars().count();
                    (format!("\"{inner}\""), 1..n + 1)
                }
                _ => (c.to_string(), 1..1),
            };
            let new_start = (start as isize + shift) as usize;
            after.push(Selection::new(new_start + select.start, new_start + select.end));
            shift += repl.chars().count() as isize - (end - start) as isize;
            edits.push(Replace { range: start..end, text: repl });
        }
        let after = Selections { list: after, primary: sels.primary };
        let kind = if c.is_whitespace() { EditKind::Other } else { EditKind::Insert };
        self.buffer.apply(edits, Some(after), kind);
    }

    fn prev_char(&self, pos: usize) -> Option<char> {
        pos.checked_sub(1).and_then(|p| self.buffer.char_at(p))
    }

    fn string_ends_at(&self, end: usize) -> bool {
        lexer::token_index_at(&self.tokens, end.saturating_sub(1)) < self.tokens.len()
            && self.tokens.iter().any(|t| matches!(t.kind, Kind::String) && t.end == end)
    }

    /// Paste text (multi-cursor aware: N lines into N cursors distributes them).
    pub fn paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n");
        let n = self.buffer.selections.list.len();
        let lines: Vec<&str> = text.lines().collect();
        if n > 1 && lines.len() == n {
            let sels = self.buffer.selections.clone();
            let edits = sels.list.iter().zip(lines).map(|(s, l)| Replace { range: s.range(), text: l.to_string() }).collect();
            self.buffer.apply(edits, None, EditKind::Other);
            let mut carets = self.buffer.selections.clone();
            for s in carets.list.iter_mut() {
                *s = Selection::caret(s.end());
            }
            self.buffer.replace_selections_after_edit(carets);
        } else {
            self.buffer.insert_at_selections(&text, EditKind::Other);
        }
        self.buffer.break_undo_group();
    }

    pub fn backspace(&mut self, unit: Motion) {
        self.sync();
        let sels = self.buffer.selections.clone();
        let mut edits = Vec::new();
        for s in &sels.list {
            if !s.is_empty() {
                edits.push(Replace { range: s.range(), text: String::new() });
                continue;
            }
            let pos = s.head;
            if pos == 0 {
                continue;
            }
            let start = match unit {
                Motion::Left => {
                    // Delete an empty bracket pair or empty string together.
                    let prev = self.buffer.char_at(pos - 1);
                    let next = self.buffer.char_at(pos);
                    let pair = matches!((prev, next), (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}')) | (Some('"'), Some('"')));
                    if pair {
                        edits.push(Replace { range: pos - 1..pos + 1, text: String::new() });
                        continue;
                    }
                    pos - 1
                }
                Motion::LineStart => {
                    let ls = self.buffer.line_start(self.buffer.line_of(pos));
                    if ls == pos { pos - 1 } else { ls }
                }
                other => self.motion_target(pos, other),
            };
            edits.push(Replace { range: start.min(pos)..pos, text: String::new() });
        }
        self.buffer.apply(edits, None, EditKind::Delete);
        self.collapse_after_edit();
    }

    pub fn delete_forward(&mut self, unit: Motion) {
        self.sync();
        let sels = self.buffer.selections.clone();
        let mut edits = Vec::new();
        for s in &sels.list {
            if !s.is_empty() {
                edits.push(Replace { range: s.range(), text: String::new() });
                continue;
            }
            let end = match unit {
                Motion::Right => (s.head + 1).min(self.buffer.len()),
                other => self.motion_target(s.head, other),
            };
            edits.push(Replace { range: s.head..end.max(s.head), text: String::new() });
        }
        self.buffer.apply(edits, None, EditKind::Delete);
        self.collapse_after_edit();
    }

    fn collapse_after_edit(&mut self) {
        let mut sels = self.buffer.selections.clone();
        for s in sels.list.iter_mut() {
            *s = Selection::caret(s.head);
        }
        sels.normalize();
        self.buffer.replace_selections_after_edit(sels);
    }

    /// Delete the sexp after each cursor (DrRacket's kill-sexp).
    pub fn kill_sexp(&mut self) {
        self.sync();
        let sels = self.buffer.selections.clone();
        let edits = sels
            .list
            .iter()
            .filter_map(|s| sexp::forward_sexp(&self.tokens, s.head).map(|e| Replace { range: s.head..e, text: String::new() }))
            .collect();
        self.buffer.apply(edits, None, EditKind::Other);
        self.collapse_after_edit();
    }

    /// Swap the sexps before and after the cursor.
    pub fn transpose_sexp(&mut self) {
        self.sync();
        let pos = self.buffer.selections.primary().head;
        let Some(a_start) = sexp::backward_sexp(&self.tokens, pos) else { return };
        let Some(a_end) = sexp::forward_sexp(&self.tokens, a_start) else { return };
        let Some(b_end) = sexp::forward_sexp(&self.tokens, a_end) else { return };
        let Some(b_start) = sexp::backward_sexp(&self.tokens, b_end) else { return };
        if b_start < a_end {
            return;
        }
        let a = self.buffer.slice(a_start..a_end);
        let b = self.buffer.slice(b_start..b_end);
        let edits = vec![Replace { range: a_start..a_end, text: b }, Replace { range: b_start..b_end, text: a }];
        self.buffer.apply(edits, Some(Selections::single(Selection::caret(b_end))), EditKind::Other);
    }

    /// Insert a newline at each cursor and indent the new line.
    pub fn newline(&mut self, indent: bool) {
        self.buffer.begin_group();
        self.buffer.insert_at_selections("\n", EditKind::Other);
        if indent {
            let lines: Vec<usize> = self.buffer.selections.list.iter().map(|s| self.buffer.line_of(s.head)).collect();
            self.indent_lines(&lines, true);
        }
        self.buffer.end_group();
    }

    /// Open a new line below (`above = false`) or above the cursor line.
    pub fn open_line(&mut self, above: bool) {
        let pos = self.buffer.selections.primary().head;
        let line = self.buffer.line_of(pos);
        let target = if above { self.buffer.line_start(line) } else { self.buffer.line_end(line) };
        self.buffer.selections = Selections::single(Selection::caret(target));
        if above {
            self.buffer.begin_group();
            self.buffer.insert_at_selections("\n", EditKind::Other);
            self.buffer.selections = Selections::single(Selection::caret(target));
            self.indent_lines(&[line], true);
            self.buffer.end_group();
        } else {
            self.newline(true);
        }
    }

    /// Desired indentation for a line, if Racket rules apply.
    pub fn desired_indent(&mut self, line: usize) -> Option<usize> {
        self.sync();
        let ls = self.buffer.line_start(line);
        sexp::indent_for(&self.tokens, &self.chars, &self.buffer, ls)
    }

    /// Reindent the given lines (sequentially, since each line's indent depends on the ones
    /// before it). `merge` folds the edit into the current undo step.
    pub fn indent_lines(&mut self, lines: &[usize], merge: bool) {
        let mut lines = lines.to_vec();
        lines.sort_unstable();
        lines.dedup();
        if !merge {
            self.buffer.begin_group();
        }
        for line in lines {
            let Some(want) = self.desired_indent(line) else { continue };
            let have = self.buffer.indentation(line);
            let ls = self.buffer.line_start(line);
            let line_blank = self.buffer.line_len(line) == have;
            // Blank lines are left empty (no trailing spaces), except the one being typed on.
            let want = if line_blank && !self.buffer.selections.list.iter().any(|s| self.buffer.line_of(s.head) == line) {
                0
            } else {
                want
            };
            let current: String = self.buffer.slice(ls..ls + have);
            if current.chars().all(|c| c == ' ') && have == want {
                continue;
            }
            let sels = self.buffer.selections.clone();
            let mut after = sels.clone();
            for s in after.list.iter_mut() {
                let fix = |p: usize| {
                    if p < ls {
                        p
                    } else if p <= ls + have {
                        ls + want
                    } else {
                        p + want - have
                    }
                };
                *s = Selection::new(fix(s.anchor), fix(s.head));
            }
            self.buffer.apply(vec![Replace { range: ls..ls + have, text: " ".repeat(want) }], Some(after), EditKind::Other);
            self.sync();
        }
        if !merge {
            self.buffer.end_group();
        }
    }

    /// DrRacket's Tab: reindent the lines touched by the selections.
    pub fn indent_selection_lines(&mut self) {
        let mut lines = Vec::new();
        for s in &self.buffer.selections.list {
            let (a, b) = (self.buffer.line_of(s.start()), self.buffer.line_of(s.end()));
            lines.extend(a..=b);
        }
        self.indent_lines(&lines, false);
        // Tab with a caret in leading whitespace moves to the first non-blank.
        let mut sels = self.buffer.selections.clone();
        for s in sels.list.iter_mut() {
            if s.is_empty() {
                let line = self.buffer.line_of(s.head);
                let first = self.buffer.line_start(line) + self.buffer.indentation(line);
                if s.head < first {
                    *s = Selection::caret(first);
                }
            }
        }
        self.buffer.selections = sels;
    }

    pub fn reindent_all(&mut self) {
        let n = self.buffer.line_count();
        let lines: Vec<usize> = (0..n).collect();
        self.indent_lines(&lines, false);
    }

    /// Reindent with Style Guide: lay out each top-level form to fit in `width` columns (see
    /// `format`), then Reindent All, as one undo step. Returns how many forms were left alone
    /// because their brackets don't match.
    pub fn reformat(&mut self, width: usize) -> usize {
        self.sync();
        let out = super::format::reformat(&self.chars, &self.tokens, width);
        self.buffer.begin_group();
        if !out.edits.is_empty() {
            // Only whitespace changes, so a cursor keeps its place among the other characters.
            let mut new = String::new();
            let mut at = 0;
            for (r, text) in &out.edits {
                new.extend(&self.chars[at..r.start]);
                new.push_str(text);
                at = r.end;
            }
            new.extend(&self.chars[at..]);
            let new: Vec<char> = new.chars().collect();
            let old = &self.chars;
            let map = |p: usize| {
                // Count the non-space characters before the cursor; the cursor stays glued to
                // the one on its left (or, after a space, to the one on its right).
                let n = old[..p].iter().filter(|c| !c.is_whitespace()).count();
                let glued_left = p > 0 && !old[p - 1].is_whitespace();
                let mut nth = new.iter().enumerate().filter(|(_, c)| !c.is_whitespace()).map(|(i, _)| i);
                if glued_left {
                    nth.nth(n - 1).map_or(new.len(), |i| i + 1)
                } else {
                    nth.nth(n).unwrap_or(new.len())
                }
            };
            let mut after = self.buffer.selections.clone();
            for s in after.list.iter_mut() {
                *s = Selection::new(map(s.anchor), map(s.head));
            }
            let edits = out.edits.into_iter().map(|(range, text)| Replace { range, text }).collect();
            self.buffer.apply(edits, Some(after), EditKind::Other);
            self.sync();
        }
        self.reindent_all();
        self.buffer.end_group();
        out.skipped
    }

    /// Toggle `;` line comments on the selected lines.
    pub fn toggle_comment(&mut self) {
        let mut lines = Vec::new();
        for s in &self.buffer.selections.list {
            let (a, mut b) = (self.buffer.line_of(s.start()), self.buffer.line_of(s.end()));
            if b > a && s.end() == self.buffer.line_start(b) {
                b -= 1;
            }
            lines.extend(a..=b);
        }
        lines.sort_unstable();
        lines.dedup();
        let non_blank: Vec<usize> = lines.iter().copied().filter(|&l| self.buffer.line_len(l) > self.buffer.indentation(l)).collect();
        if non_blank.is_empty() {
            return;
        }
        let all_commented = non_blank.iter().all(|&l| {
            let t = self.buffer.line_text(l);
            t.trim_start().starts_with(';')
        });
        let min_indent = non_blank.iter().map(|&l| self.buffer.indentation(l)).min().unwrap_or(0);
        let mut edits = Vec::new();
        for &l in &non_blank {
            let ls = self.buffer.line_start(l);
            if all_commented {
                let ind = self.buffer.indentation(l);
                let text = self.buffer.line_text(l);
                let rest = text.trim_start();
                let semis = rest.chars().take_while(|c| *c == ';').count().min(2);
                let space = rest.chars().nth(semis) == Some(' ');
                let n = semis + usize::from(space);
                edits.push(Replace { range: ls + ind..ls + ind + n, text: String::new() });
            } else {
                edits.push(Replace { range: ls + min_indent..ls + min_indent, text: "; ".into() });
            }
        }
        self.buffer.break_undo_group();
        self.buffer.apply(edits, None, EditKind::Other);
        self.buffer.break_undo_group();
    }

    /// Wrap the selection in a `#| |#` block comment, or remove the one around the cursor.
    pub fn toggle_block_comment(&mut self) {
        self.sync();
        let s = self.buffer.selections.primary();
        if let Some((_, t)) = lexer::token_at(&self.tokens, s.start()).filter(|(_, t)| t.kind == Kind::BlockComment) {
            let (a, b) = (t.start, t.end);
            let edits = vec![Replace { range: a..a + 2, text: String::new() }, Replace { range: b - 2..b, text: String::new() }];
            self.buffer.apply(edits, None, EditKind::Other);
        } else if !s.is_empty() {
            let edits = vec![Replace { range: s.start()..s.start(), text: "#|".into() }, Replace { range: s.end()..s.end(), text: "|#".into() }];
            self.buffer.apply(edits, None, EditKind::Other);
        }
    }

    /// Comment out the sexp after the cursor with `#;` (or remove an existing `#;`).
    pub fn toggle_datum_comment(&mut self) {
        self.sync();
        let pos = self.buffer.selections.primary().head;
        let i = lexer::token_index_at(&self.tokens, pos);
        if let Some(t) = self.tokens.get(i).filter(|t| t.kind == Kind::DatumComment && t.start <= pos + 2) {
            let r = t.start..t.end;
            self.buffer.apply(vec![Replace { range: r, text: String::new() }], None, EditKind::Other);
        } else {
            self.buffer.apply(vec![Replace { range: pos..pos, text: "#;".into() }], None, EditKind::Other);
        }
    }

    /// Lines covered by all selections.
    fn selected_lines(&self) -> Vec<usize> {
        let mut lines = Vec::new();
        for s in &self.buffer.selections.list {
            let (a, mut b) = (self.buffer.line_of(s.start()), self.buffer.line_of(s.end()));
            if b > a && s.end() == self.buffer.line_start(b) {
                b -= 1;
            }
            lines.extend(a..=b);
        }
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    pub fn delete_lines(&mut self) {
        let lines = self.selected_lines();
        let mut edits = Vec::new();
        for &l in &lines {
            let s = self.buffer.line_start(l);
            let e = if l + 1 < self.buffer.line_count() { self.buffer.line_start(l + 1) } else { self.buffer.len() };
            let s = if l + 1 >= self.buffer.line_count() && s > 0 { s - 1 } else { s };
            edits.push(Replace { range: s..e, text: String::new() });
        }
        self.buffer.break_undo_group();
        self.buffer.apply(edits, None, EditKind::Other);
        self.collapse_after_edit();
    }

    /// Move selected lines up or down by one.
    pub fn move_lines(&mut self, down: bool) {
        let lines = self.selected_lines();
        let (Some(&first), Some(&last)) = (lines.first(), lines.last()) else { return };
        let n = self.buffer.line_count();
        if (down && last + 1 >= n) || (!down && first == 0) {
            return;
        }
        let block_start = self.buffer.line_start(first);
        let block_end = self.buffer.line_end(last);
        let block = self.buffer.slice(block_start..block_end);
        let (range, text, delta): (Range<usize>, String, isize) = if down {
            let other_end = self.buffer.line_end(last + 1);
            let other = self.buffer.slice(self.buffer.line_start(last + 1)..other_end);
            (block_start..other_end, format!("{other}\n{block}"), other.chars().count() as isize + 1)
        } else {
            let other_start = self.buffer.line_start(first - 1);
            let other = self.buffer.slice(other_start..self.buffer.line_end(first - 1));
            (other_start..block_end, format!("{block}\n{other}"), -(other.chars().count() as isize + 1))
        };
        let mut after = self.buffer.selections.clone();
        for s in after.list.iter_mut() {
            *s = Selection::new((s.anchor as isize + delta) as usize, (s.head as isize + delta) as usize);
        }
        self.buffer.break_undo_group();
        self.buffer.apply(vec![Replace { range, text }], Some(after), EditKind::Other);
        self.buffer.break_undo_group();
    }

    pub fn duplicate_lines(&mut self, down: bool) {
        let lines = self.selected_lines();
        let (Some(&first), Some(&last)) = (lines.first(), lines.last()) else { return };
        let s = self.buffer.line_start(first);
        let e = self.buffer.line_end(last);
        let block = self.buffer.slice(s..e);
        let len = block.chars().count() as isize + 1;
        let mut after = self.buffer.selections.clone();
        if down {
            for sel in after.list.iter_mut() {
                *sel = Selection::new((sel.anchor as isize + len) as usize, (sel.head as isize + len) as usize);
            }
        }
        self.buffer.break_undo_group();
        self.buffer.apply(vec![Replace { range: e..e, text: format!("\n{block}") }], Some(after), EditKind::Other);
    }

    /// Whether `r` is exactly one name token (an identifier, number, ...): occurrences of a
    /// selected name are then whole names, so `k` doesn't match inside `make-node`.
    fn is_whole_token(&self, r: &Range<usize>) -> bool {
        let i = lexer::token_index_at(&self.tokens, r.start);
        self.tokens.get(i).is_some_and(|t| t.start == r.start && t.end == r.end && matches!(t.kind, Kind::Symbol | Kind::Number | Kind::Boolean | Kind::HashKeyword | Kind::Char | Kind::LangName))
    }

    /// Cmd+D: add a selection at the next occurrence of the primary selection's text.
    pub fn select_next_occurrence(&mut self) {
        let p = self.buffer.selections.primary();
        if p.is_empty() {
            if let Some(r) = self.word_at(p.head) {
                self.buffer.selections.set_primary(Selection::new(r.start, r.end));
            }
            return;
        }
        self.sync();
        let needle: Vec<char> = self.buffer.slice(p.range()).chars().collect();
        let whole = self.is_whole_token(&p.range());
        let hay = &self.chars;
        let occupied = |s: usize| self.buffer.selections.list.iter().any(|x| x.start() == s);
        let n = hay.len();
        let fits = |i: usize| hay[i..i + needle.len()] == needle[..] && !occupied(i) && (!whole || self.is_whole_token(&(i..i + needle.len())));
        let find_from = |from: usize| (from..n.saturating_sub(needle.len() - 1)).find(|&i| fits(i));
        let start = self.buffer.selections.list.iter().map(|s| s.end()).max().unwrap_or(0);
        if let Some(i) = find_from(start).or_else(|| find_from(0)) {
            self.buffer.selections.add(Selection::new(i, i + needle.len()));
        }
    }

    /// Select every occurrence of the primary selection (or word).
    pub fn select_all_occurrences(&mut self) {
        let p = self.buffer.selections.primary();
        let r = if p.is_empty() { self.word_at(p.head) } else { Some(p.range()) };
        let Some(r) = r else { return };
        self.sync();
        let whole = self.is_whole_token(&r);
        let needle: Vec<char> = self.buffer.slice(r).chars().collect();
        if needle.is_empty() {
            return;
        }
        let mut list = Vec::new();
        let mut i = 0;
        while i + needle.len() <= self.chars.len() {
            if self.chars[i..i + needle.len()] == needle[..] && (!whole || self.is_whole_token(&(i..i + needle.len()))) {
                list.push(Selection::new(i, i + needle.len()));
                i += needle.len();
            } else {
                i += 1;
            }
        }
        if !list.is_empty() {
            self.buffer.selections = Selections { list, primary: 0 };
        }
    }

    /// Add a cursor on the line above/below the primary cursor at the same column.
    pub fn add_cursor_vertical(&mut self, down: bool) {
        let p = self.buffer.selections.primary();
        let line = self.buffer.line_of(p.head);
        let col = self.buffer.col_of(p.head);
        let target = if down { line + 1 } else { line.wrapping_sub(1) };
        if target >= self.buffer.line_count() {
            return;
        }
        let pos = self.buffer.line_start(target) + col.min(self.buffer.line_len(target));
        self.buffer.selections.add(Selection::caret(pos));
    }

    /// Replace `ranges` (e.g. all occurrences of a binding) with `new_name`.
    pub fn rename(&mut self, ranges: &[Range<usize>], new_name: &str) {
        let edits = ranges.iter().map(|r| Replace { range: r.clone(), text: new_name.to_string() }).collect();
        self.buffer.break_undo_group();
        self.buffer.apply(edits, None, EditKind::Other);
        self.buffer.break_undo_group();
    }

    /// Text of the selections (joined with newlines) or, with no selection, the full lines.
    pub fn copy_text(&self) -> (String, bool) {
        let sels = &self.buffer.selections;
        if sels.list.iter().all(|s| s.is_empty()) {
            let mut out = String::new();
            for s in &sels.list {
                let l = self.buffer.line_of(s.head);
                out.push_str(&self.buffer.line_text(l));
                out.push('\n');
            }
            (out, true)
        } else {
            let parts: Vec<String> = sels.list.iter().filter(|s| !s.is_empty()).map(|s| self.buffer.slice(s.range())).collect();
            (parts.join("\n"), false)
        }
    }

    pub fn cut(&mut self) -> String {
        let (text, whole_lines) = self.copy_text();
        if whole_lines {
            self.delete_lines();
        } else {
            let edits = self.buffer.selections.list.iter().map(|s| Replace { range: s.range(), text: String::new() }).collect();
            self.buffer.break_undo_group();
            self.buffer.apply(edits, None, EditKind::Other);
            self.collapse_after_edit();
        }
        text
    }

    /// Top-level definitions for the outline / "(define ...)" menu, cached per version.
    pub fn outline(&mut self) -> std::sync::Arc<Vec<OutlineItem>> {
        self.sync();
        let v = self.buffer.version();
        if let Some((cv, o)) = &self.outline_cache
            && *cv == v
        {
            return o.clone();
        }
        let o = std::sync::Arc::new(self.compute_outline());
        self.outline_cache = Some((v, o.clone()));
        o
    }

    fn compute_outline(&mut self) -> Vec<OutlineItem> {
        let toks = &self.tokens;
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let t = toks[i];
            if t.kind == Kind::Open && t.depth == 0 {
                self.outline_form(i, &mut out, 0);
                i = t.partner.map(|p| p as usize + 1).unwrap_or(i + 1);
            } else {
                i += 1;
            }
        }
        out
    }

    fn outline_form(&self, open: usize, out: &mut Vec<OutlineItem>, level: usize) {
        let toks = &self.tokens;
        let text = |t: &Token| -> String { self.chars[t.start..t.end].iter().collect() };
        let head_i = (open + 1..toks.len()).find(|&k| !toks[k].is_trivia());
        let Some(hi) = head_i else { return };
        if toks[hi].kind != Kind::Symbol {
            return;
        }
        let head = text(&toks[hi]);
        let Some(ni) = (hi + 1..toks.len()).find(|&k| !toks[k].is_trivia()) else { return };
        let name_tok = if toks[ni].kind == Kind::Open { (ni + 1..toks.len()).find(|&k| !toks[k].is_trivia()) } else { Some(ni) };
        let kind = match head.as_str() {
            "define" | "define/contract" | "define/public" | "define/private" | "define/override" => {
                if toks[ni].kind == Kind::Open { OutlineKind::Function } else { OutlineKind::Value }
            }
            "define-values" => OutlineKind::Value,
            "define-syntax" | "define-syntax-rule" | "define-simple-macro" | "define-syntax-parser" => OutlineKind::Macro,
            "struct" | "define-struct" | "define-record-procedures" | "define-type" | "define-datatype" => OutlineKind::Struct,
            "module" | "module+" | "module*" => OutlineKind::Module,
            "check-expect" | "check-within" | "check-error" | "test-case" | "check-equal?" | "check-satisfied" | "check-random" | "check-member-of" | "check-range" => {
                OutlineKind::Test
            }
            _ if head.starts_with("define") => OutlineKind::Value,
            _ => return,
        };
        let Some(nt) = name_tok.and_then(|k| toks.get(k)) else { return };
        let mut name = if kind == OutlineKind::Test {
            let close = toks[open].partner.map(|p| toks[p as usize].end).unwrap_or(toks[open].end);
            let s: String = self.chars[toks[open].start..close].iter().collect();
            let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
            if one_line.chars().count() > 48 { format!("{}…", one_line.chars().take(47).collect::<String>()) } else { one_line }
        } else {
            text(nt)
        };
        if kind == OutlineKind::Module && name == "test" {
            name = "test".into();
        }
        out.push(OutlineItem { name, kind, pos: toks[open].start, name_pos: nt.start, level });
        if kind == OutlineKind::Module {
            // Nested definitions in submodules.
            if let Some(close) = toks[open].partner {
                let mut k = hi + 1;
                while k < close as usize {
                    if toks[k].kind == Kind::Open && toks[k].depth == toks[open].depth + 1 {
                        self.outline_form(k, out, level + 1);
                        k = toks[k].partner.map(|p| p as usize + 1).unwrap_or(k + 1);
                    } else {
                        k += 1;
                    }
                }
            }
        }
    }

    /// The first `n` chars of the document (cheap, for `#lang` detection).
    pub fn head(&self, n: usize) -> String {
        let r = self.buffer.rope();
        r.slice(..n.min(r.len_chars())).to_string()
    }

    /// The symbol at the head of the list enclosing `pos` (for signature help).
    pub fn enclosing_call(&mut self, pos: usize) -> Option<(String, usize)> {
        self.sync();
        let mut probe = pos;
        for _ in 0..3 {
            let o = sexp::enclosing_open(&self.tokens, probe)?;
            let h = (o + 1..self.tokens.len()).find(|&k| !self.tokens[k].is_trivia())?;
            let t = self.tokens[h];
            if t.kind == Kind::Symbol && t.class != Class::Symbol && t.end <= pos {
                return Some((self.chars[t.start..t.end].iter().collect(), t.start));
            }
            probe = self.tokens[o].start;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineKind {
    Function,
    Value,
    Macro,
    Struct,
    Module,
    Test,
}

#[derive(Clone, Debug)]
pub struct OutlineItem {
    pub name: String,
    pub kind: OutlineKind,
    pub pos: usize,
    pub name_pos: usize,
    pub level: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str, caret: usize) -> Document {
        let mut d = Document::new(text);
        d.buffer.selections = Selections::single(Selection::caret(caret));
        d
    }

    #[test]
    fn reformat_is_one_undo_step_and_keeps_the_cursor() {
        let src = "(define (set=? set1 set2) (cond [(empty? set1) (cond [(empty? set2) true] [else false])] [(empty? set2) (cond [(empty? set1) true] [else false])]))\n";
        // Caret just after "else" in the last clause.
        let caret = src.rfind("else").unwrap() + 4;
        let mut d = doc(src, caret);
        assert_eq!(d.reformat(102), 0);
        assert_eq!(
            d.text(),
            "(define (set=? set1 set2)\n  (cond [(empty? set1)\n         (cond [(empty? set2) true]\n               [else false])]\n        [(empty? set2)\n         (cond [(empty? set1) true]\n               [else false])]))\n"
        );
        let at = d.buffer.selections.primary().head;
        assert_eq!(&d.text()[at - 5..at + 1], "[else ");
        d.buffer.undo();
        assert_eq!(d.text(), src);
    }

    #[test]
    fn auto_pairs_and_skips_closers() {
        let mut d = doc("", 0);
        d.type_text("(", Typing::default());
        assert_eq!(d.text(), "()");
        d.type_text("a", Typing::default());
        d.type_text(")", Typing::default());
        assert_eq!(d.text(), "(a)");
        assert_eq!(d.buffer.selections.primary().head, 3);
    }

    #[test]
    fn close_bracket_matches_opener() {
        let mut d = doc("[a", 2);
        d.type_text(")", Typing::default());
        assert_eq!(d.text(), "[a]");
    }

    #[test]
    fn close_bracket_as_typed_when_matching_is_off() {
        let typing = Typing { match_closers: false, ..Typing::default() };
        // [empty? set false] -> typing ) after "set" keeps the ).
        let mut d = doc("[empty? set false]", 11);
        d.type_text(")", typing);
        assert_eq!(d.text(), "[empty? set) false]");
        // Typing over the same closer still steps past it.
        let mut d = doc("(a)", 2);
        d.type_text(")", typing);
        assert_eq!(d.text(), "(a)");
        assert_eq!(d.buffer.selections.primary().head, 3);
    }

    #[test]
    fn literal_typing_inserts_closers_as_given() {
        let mut d = doc("(a)", 2);
        d.type_text(")", Typing::LITERAL);
        assert_eq!(d.text(), "(a))");
    }

    #[test]
    fn wraps_selection() {
        let mut d = doc("abc", 0);
        d.buffer.selections = Selections::single(Selection::new(0, 3));
        d.type_text("(", Typing::default());
        assert_eq!(d.text(), "(abc)");
        // The wrapped text stays selected, caret at its end, whichever way it was selected.
        assert_eq!(d.buffer.selections.primary(), Selection::new(1, 4));
        d.buffer.selections = Selections::single(Selection::new(4, 1));
        d.type_text("[", Typing::default());
        assert_eq!(d.text(), "([abc])");
        assert_eq!(d.buffer.selections.primary(), Selection::new(2, 5));
        d.type_text("\"", Typing::default());
        assert_eq!(d.text(), "([\"abc\"])");
        assert_eq!(d.buffer.selections.primary(), Selection::new(3, 6));
        // Typing replaces it.
        d.type_text("x", Typing::default());
        assert_eq!(d.text(), "([\"x\"])");
    }

    #[test]
    fn string_quotes_pair_and_skip() {
        let mut d = doc("", 0);
        d.type_text("\"", Typing::default());
        assert_eq!(d.text(), "\"\"");
        d.type_text("x", Typing::default());
        d.type_text("\"", Typing::default());
        assert_eq!(d.text(), "\"x\"");
        assert_eq!(d.buffer.selections.primary().head, 3);
    }

    #[test]
    fn backspace_deletes_empty_pair() {
        let mut d = doc("()", 1);
        d.backspace(Motion::Left);
        assert_eq!(d.text(), "");
    }

    #[test]
    fn newline_indents() {
        let mut d = doc("(define (f x)", 13);
        d.newline(true);
        assert_eq!(d.text(), "(define (f x)\n  ");
        assert_eq!(d.buffer.selections.primary().head, 16);
    }

    #[test]
    fn reindent_all_and_undo() {
        let mut d = doc("(define (f x)\n(+ x\n1))", 0);
        d.reindent_all();
        assert_eq!(d.text(), "(define (f x)\n  (+ x\n     1))");
        d.buffer.undo();
        assert_eq!(d.text(), "(define (f x)\n(+ x\n1))");
    }

    #[test]
    fn toggle_comment_roundtrip() {
        let mut d = doc("(a)\n  (b)", 0);
        d.buffer.selections = Selections::single(Selection::new(0, 9));
        d.toggle_comment();
        assert_eq!(d.text(), "; (a)\n;   (b)");
        d.buffer.selections = Selections::single(Selection::new(0, 13));
        d.toggle_comment();
        assert_eq!(d.text(), "(a)\n  (b)");
    }

    #[test]
    fn move_and_duplicate_lines() {
        let mut d = doc("a\nb\nc", 0);
        d.move_lines(true);
        assert_eq!(d.text(), "b\na\nc");
        assert_eq!(d.buffer.selections.primary().head, 2);
        d.duplicate_lines(true);
        assert_eq!(d.text(), "b\na\na\nc");
    }

    #[test]
    fn select_next_occurrence_and_multi_type() {
        let mut d = doc("foo bar foo", 1);
        d.select_next_occurrence();
        d.select_next_occurrence();
        assert_eq!(d.buffer.selections.list.len(), 2);
        d.type_text("x", Typing::default());
        assert_eq!(d.text(), "x bar x");
    }

    #[test]
    fn outline_finds_definitions() {
        let mut d = doc("#lang racket\n(define (f x) x)\n(define y 2)\n(struct p (a))\n(module+ test (define z 1))", 0);
        let o = d.outline();
        let names: Vec<_> = o.iter().map(|i| (i.name.as_str(), i.kind)).collect();
        assert_eq!(names, vec![("f", OutlineKind::Function), ("y", OutlineKind::Value), ("p", OutlineKind::Struct), ("test", OutlineKind::Module), ("z", OutlineKind::Value)]);
    }

    /// A selected name matches whole names only; other text matches anywhere.
    #[test]
    fn occurrences_of_a_name_are_whole_names() {
        let mut d = Document::new("(define (f k t) (make-node k (node-key t)))");
        let k = d.buffer.text().find(" k ").unwrap() + 1;
        d.buffer.selections = Selections::single(Selection::new(k, k + 1));
        d.select_all_occurrences();
        let text = d.buffer.text();
        let picked: Vec<&str> = d.buffer.selections.list.iter().map(|s| &text[s.start()..s.end()]).collect();
        assert_eq!(picked, vec!["k", "k"], "not the k in make-node or node-key");
        let ke = text.find("ke").unwrap();
        d.buffer.selections = Selections::single(Selection::new(ke, ke + 2));
        d.select_all_occurrences();
        assert_eq!(d.buffer.selections.list.len(), 2, "a partial selection still matches inside names");
    }

    #[test]
    fn sexp_motions_and_transpose() {
        let mut d = doc("(a b)", 1);
        assert_eq!(d.motion_target(1, Motion::SexpRight), 2);
        d.buffer.selections = Selections::single(Selection::caret(2));
        d.transpose_sexp();
        assert_eq!(d.text(), "(b a)");
    }

    #[test]
    fn incremental_sync_matches_full_lex() {
        let mut d = doc("(define (f x)\n  (+ x 1))", 0);
        d.buffer.selections = Selections { list: vec![Selection::caret(1), Selection::caret(16)], primary: 0 };
        for t in ["'", "(", "\"", "x", ";"] {
            d.type_text(t, Typing::default());
            d.sync();
            assert_eq!(d.tokens_cached(), &lexer::lex(&d.text())[..]);
        }
        d.buffer.undo();
        d.sync();
        assert_eq!(d.tokens_cached(), &lexer::lex(&d.text())[..]);
        d.reindent_all();
        d.sync();
        assert_eq!(d.tokens_cached(), &lexer::lex(&d.text())[..]);
    }

    #[test]
    fn enclosing_call_head() {
        let mut d = doc("(map (lambda (x) x) lst)", 21);
        assert_eq!(d.enclosing_call(21).map(|x| x.0).as_deref(), Some("map"));
    }
}

/// Reindent All must reproduce DrRacket's own indentation. The files in `tests/indent` were
/// indented by DrRacket (regenerate with `racket tools/drracket-indent.rkt tests/indent/*.rkt`).
#[cfg(test)]
mod drracket_indentation {
    use super::Document;

    /// `expected` with the indentation of every reindentable line replaced by `pad`.
    fn misindent(expected: &str, pad: &str) -> String {
        let mut d = Document::new(expected);
        let lines: Vec<String> = expected.split('\n').map(str::to_string).collect();
        let mut out = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if d.desired_indent(i).is_some() && !line.trim().is_empty() {
                out.push(format!("{pad}{}", line.trim_start()));
            } else {
                out.push(line.clone());
            }
        }
        out.join("\n")
    }

    #[test]
    fn reindent_all_matches_drracket() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/indent");
        let mut files = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|x| x != "rkt") {
                continue;
            }
            files += 1;
            // DrRacket leaves blank lines alone; Dr. Rocket empties them.
            let expected: String = std::fs::read_to_string(&path).unwrap().split('\n').map(|l| if l.trim().is_empty() { "" } else { l }).collect::<Vec<_>>().join("\n");
            for pad in ["", "       "] {
                let mut d = Document::new(&misindent(&expected, pad));
                d.reindent_all();
                let got = d.text();
                if got != expected {
                    let line = got.lines().zip(expected.lines()).position(|(a, b)| a != b).unwrap_or(0);
                    panic!(
                        "{} (pad {:?}) differs from DrRacket at line {}:\n  ours:     {:?}\n  drracket: {:?}",
                        path.display(),
                        pad,
                        line + 1,
                        got.lines().nth(line).unwrap_or(""),
                        expected.lines().nth(line).unwrap_or("")
                    );
                }
            }
        }
        assert!(files >= 2);
    }
}
