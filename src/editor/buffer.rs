//! Text buffer: a rope plus multi-cursor selections and grouped undo/redo.
//!
//! All positions are **char** offsets into the rope (ropey's native unit), which is also the
//! unit Racket uses for `syntax-position`, so backend positions map over directly.

use ropey::Rope;
use std::ops::Range;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    /// Where the selection started (does not move when extending).
    pub anchor: usize,
    /// Where the cursor is.
    pub head: usize,
}

impl Selection {
    pub fn caret(pos: usize) -> Self {
        Self { anchor: pos, head: pos }
    }
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    /// Map this selection through an edit replacing `range` with `len` chars.
    fn map(&self, range: &Range<usize>, len: usize) -> Self {
        Self { anchor: map_pos(self.anchor, range, len, false), head: map_pos(self.head, range, len, true) }
    }
}

/// Map a position through an edit. Positions inside a replaced range move to its end when
/// `stick_end`, else to its start.
pub fn map_pos(pos: usize, range: &Range<usize>, len: usize, stick_end: bool) -> usize {
    if pos < range.start {
        pos
    } else if pos >= range.end && !(pos == range.end && range.is_empty() && !stick_end) {
        pos - (range.end - range.start) + len
    } else if stick_end {
        range.start + len
    } else {
        range.start
    }
}

/// A set of non-overlapping selections, sorted by position. `primary` indexes the one the
/// viewport follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selections {
    pub list: Vec<Selection>,
    pub primary: usize,
}

impl Selections {
    pub fn single(sel: Selection) -> Self {
        Self { list: vec![sel], primary: 0 }
    }
    pub fn primary(&self) -> Selection {
        self.list[self.primary]
    }
    pub fn set_primary(&mut self, sel: Selection) {
        self.list[self.primary] = sel;
        self.normalize();
    }
    pub fn add(&mut self, sel: Selection) {
        self.list.push(sel);
        self.primary = self.list.len() - 1;
        self.normalize();
    }
    /// Sort and merge overlapping selections, keeping track of the primary one.
    pub fn normalize(&mut self) {
        let primary = self.list[self.primary];
        self.list.sort_by_key(|s| (s.start(), s.end()));
        let mut out: Vec<Selection> = Vec::with_capacity(self.list.len());
        let mut primary_idx = 0;
        for s in self.list.drain(..) {
            if let Some(last) = out.last_mut()
                && (s.start() < last.end() || (s.start() == last.end() && (s.is_empty() || last.is_empty()) && s.start() == last.start()))
            {
                let (start, end) = (last.start().min(s.start()), last.end().max(s.end()));
                let forward = last.head >= last.anchor;
                *last = if forward { Selection::new(start, end) } else { Selection::new(end, start) };
                if s == primary {
                    primary_idx = out.len() - 1;
                }
                continue;
            }
            if s == primary {
                primary_idx = out.len();
            }
            out.push(s);
        }
        self.list = out;
        self.primary = primary_idx.min(self.list.len() - 1);
    }
}

#[derive(Clone, Debug)]
struct Change {
    /// Range in the document *before* this change.
    range: Range<usize>,
    removed: String,
    inserted: String,
}

#[derive(Clone, Debug)]
struct Transaction {
    changes: Vec<Change>,
    before: Selections,
    after: Selections,
    kind: EditKind,
    at: Instant,
}

/// Used to group consecutive edits into one undo step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    Other,
}

/// One replacement in a batch edit (ranges refer to the document before the batch).
#[derive(Clone, Debug)]
pub struct Replace {
    pub range: Range<usize>,
    pub text: String,
}

pub struct Buffer {
    rope: Rope,
    pub selections: Selections,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// Bumped on every content change.
    version: u64,
    saved_version: u64,
    /// When set, the next edit starts a new undo group even if it would otherwise merge.
    break_undo: bool,
    /// Changes since the last call to `take_changes` (for incremental consumers).
    pending: Vec<(Range<usize>, usize)>,
    /// While > 0, all edits merge into a single undo step.
    group_depth: usize,
    /// Whether the open group already has a transaction on the undo stack.
    group_started: bool,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            selections: Selections::single(Selection::caret(0)),
            undo: Vec::new(),
            redo: Vec::new(),
            version: 0,
            saved_version: 0,
            break_undo: false,
            pending: Vec::new(),
            group_depth: 0,
            group_started: false,
        }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }
    pub fn text(&self) -> String {
        self.rope.to_string()
    }
    pub fn len(&self) -> usize {
        self.rope.len_chars()
    }
    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }
    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }
    pub fn char_at(&self, pos: usize) -> Option<char> {
        (pos < self.len()).then(|| self.rope.char(pos))
    }
    pub fn slice(&self, r: Range<usize>) -> String {
        let r = r.start.min(self.len())..r.end.min(self.len());
        self.rope.slice(r).to_string()
    }
    pub fn line_of(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.len()))
    }
    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.line_count()))
    }
    /// End of the line's content, excluding the line break.
    pub fn line_end(&self, line: usize) -> usize {
        if line + 1 >= self.line_count() {
            return self.len();
        }
        let next = self.rope.line_to_char(line + 1);
        let mut end = next;
        if end > 0 && self.rope.char(end - 1) == '\n' {
            end -= 1;
            if end > 0 && self.rope.char(end - 1) == '\r' {
                end -= 1;
            }
        }
        end
    }
    pub fn line_text(&self, line: usize) -> String {
        self.slice(self.line_start(line)..self.line_end(line))
    }
    pub fn line_len(&self, line: usize) -> usize {
        self.line_end(line) - self.line_start(line)
    }
    pub fn col_of(&self, pos: usize) -> usize {
        pos - self.line_start(self.line_of(pos))
    }
    /// Leading whitespace width (in chars) of a line.
    pub fn indentation(&self, line: usize) -> usize {
        self.rope.line(line).chars().take_while(|c| *c == ' ' || *c == '\t').count()
    }

    /// Mark an undo boundary: the next edit starts a fresh undo step.
    pub fn break_undo_group(&mut self) {
        self.break_undo = true;
    }

    /// Apply non-overlapping replacements atomically (one undo step), then set selections.
    /// Ranges are relative to the document before the edit. `after` maps the old selections
    /// when `None`.
    pub fn apply(&mut self, mut edits: Vec<Replace>, after: Option<Selections>, kind: EditKind) {
        edits.retain(|e| !(e.range.is_empty() && e.text.is_empty()));
        if edits.is_empty() {
            if let Some(a) = after {
                self.selections = a;
            }
            return;
        }
        edits.sort_by_key(|e| e.range.start);
        let before = self.selections.clone();
        let mut changes = Vec::with_capacity(edits.len());
        // Apply back to front so earlier ranges stay valid.
        for e in edits.iter().rev() {
            let r = e.range.start.min(self.len())..e.range.end.min(self.len());
            let removed = self.rope.slice(r.clone()).to_string();
            self.rope.remove(r.clone());
            self.rope.insert(r.start, &e.text);
            changes.push(Change { range: r, removed, inserted: e.text.clone() });
        }
        changes.reverse();
        // Re-express changes sequentially: each relative to the document after the previous ones.
        let mut shift: isize = 0;
        for c in changes.iter_mut() {
            let len = c.range.end - c.range.start;
            let start = (c.range.start as isize + shift) as usize;
            c.range = start..start + len;
            shift += c.inserted.chars().count() as isize - len as isize;
        }
        let after = after.unwrap_or_else(|| {
            let mut sels = before.clone();
            for s in sels.list.iter_mut() {
                for c in &changes {
                    *s = s.map(&c.range, c.inserted.chars().count());
                }
            }
            sels.normalize();
            sels
        });
        for c in &changes {
            self.pending.push((c.range.clone(), c.inserted.chars().count()));
        }
        self.selections = after.clone();
        self.version += 1;
        self.redo.clear();

        let now = Instant::now();
        let in_group = self.group_depth > 0 && self.group_started;
        let merge = in_group
            || !self.break_undo
            && kind != EditKind::Other
            && self.undo.last().is_some_and(|t| {
                t.kind == kind && now.duration_since(t.at) < Duration::from_millis(1200) && t.after == before
            });
        if let Some(t) = self.undo.last_mut().filter(|_| merge) {
            // Changes are sequential, so the new edit simply appends to the previous step.
            t.changes.extend(changes);
            t.after = after;
            t.at = now;
        } else {
            self.undo.push(Transaction { changes, before, after, kind, at: now });
            if self.undo.len() > 10_000 {
                self.undo.remove(0);
            }
        }
        self.break_undo = false;
        if self.group_depth > 0 {
            self.group_started = true;
        }
    }

    /// Start an undo group: every edit until the matching `end_group` is one undo step.
    pub fn begin_group(&mut self) {
        if self.group_depth == 0 {
            self.group_started = false;
            self.break_undo = true;
        }
        self.group_depth += 1;
    }

    pub fn end_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
        if self.group_depth == 0 {
            self.break_undo = true;
        }
    }

    /// History navigation and leaving an editing mode end any outstanding transaction.
    /// A later matching `end_group` is harmless; subsequent edits start fresh history.
    pub fn close_groups(&mut self) {
        self.group_depth = 0;
        self.group_started = false;
        self.break_undo = true;
    }

    /// Replace each selection with `text` (typing / paste).
    pub fn insert_at_selections(&mut self, text: &str, kind: EditKind) {
        let edits = self.selections.list.iter().map(|s| Replace { range: s.range(), text: text.to_string() }).collect();
        self.apply(edits, None, kind);
        let mut carets = self.selections.clone();
        for s in carets.list.iter_mut() {
            *s = Selection::caret(s.head);
        }
        self.replace_selections_after_edit(carets);
    }

    /// Replace the selections produced by the edit just applied (keeps undo grouping intact).
    pub fn replace_selections_after_edit(&mut self, sels: Selections) {
        if let Some(t) = self.undo.last_mut() {
            t.after = sels.clone();
        }
        self.selections = sels;
    }

    pub fn set_text(&mut self, text: &str) {
        let all = 0..self.len();
        self.apply(vec![Replace { range: all, text: text.to_string() }], Some(Selections::single(Selection::caret(0))), EditKind::Other);
    }

    pub fn undo(&mut self) -> bool {
        self.close_groups();
        let Some(t) = self.undo.pop() else { return false };
        // Changes within a transaction are sequential: each is expressed against the document
        // after the previous ones. Undo in reverse order.
        for c in t.changes.iter().rev() {
            let r = c.range.start..c.range.start + c.inserted.chars().count();
            self.rope.remove(r.clone());
            self.rope.insert(r.start, &c.removed);
            self.pending.push((r, c.removed.chars().count()));
        }
        self.selections = t.before.clone();
        self.version += 1;
        self.redo.push(t);
        self.break_undo = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        self.close_groups();
        let Some(t) = self.redo.pop() else { return false };
        for c in &t.changes {
            let r = c.range.start..c.range.start + c.removed.chars().count();
            self.rope.remove(r.clone());
            self.rope.insert(r.start, &c.inserted);
            self.pending.push((r, c.inserted.chars().count()));
        }
        self.selections = t.after.clone();
        self.version += 1;
        self.undo.push(t);
        self.break_undo = true;
        true
    }

    /// Edits since the last call: (old range, inserted length). Used to shift markers.
    pub fn take_changes(&mut self) -> Vec<(Range<usize>, usize)> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_navigation_closes_nested_and_empty_groups() {
        for history in [false, true] {
            for redo in [false, true] {
                let mut b = Buffer::new("");
                if history { b.insert_at_selections("old", EditKind::Other); }
                b.begin_group();
                b.begin_group();
                b.insert_at_selections("a", EditKind::Insert);
                assert!(b.undo());
                if redo { assert!(b.redo()); }
                b.insert_at_selections("b", EditKind::Insert);
                b.end_group();
                b.end_group();
                assert!(b.undo());
                assert_eq!(b.text(), format!("{}{}", if history { "old" } else { "" }, if redo { "a" } else { "" }));
                if redo { assert!(b.undo()); }
                if history { assert!(b.undo()); }
                assert_eq!(b.text(), "");
                assert!(!b.undo());
            }
        }
        let mut b = Buffer::new("");
        b.begin_group();
        assert!(!b.undo());
        b.insert_at_selections("a", EditKind::Insert);
        b.close_groups();
        b.insert_at_selections("b", EditKind::Insert);
        assert!(b.undo());
        assert_eq!(b.text(), "a");
    }

    #[test]
    fn multi_cursor_insert_and_undo() {
        let mut b = Buffer::new("ab\ncd\n");
        b.selections = Selections { list: vec![Selection::caret(0), Selection::caret(3)], primary: 0 };
        b.insert_at_selections("(", EditKind::Insert);
        assert_eq!(b.text(), "(ab\n(cd\n");
        assert_eq!(b.selections.list, vec![Selection::caret(1), Selection::caret(5)]);
        assert!(b.undo());
        assert_eq!(b.text(), "ab\ncd\n");
        assert!(b.redo());
        assert_eq!(b.text(), "(ab\n(cd\n");
    }

    #[test]
    fn typing_merges_into_one_undo_step() {
        let mut b = Buffer::new("");
        for ch in ["d", "e", "f"] {
            b.insert_at_selections(ch, EditKind::Insert);
        }
        assert_eq!(b.text(), "def");
        b.undo();
        assert_eq!(b.text(), "");
    }

    #[test]
    fn break_undo_separates_groups() {
        let mut b = Buffer::new("");
        b.insert_at_selections("a", EditKind::Insert);
        b.break_undo_group();
        b.insert_at_selections("b", EditKind::Insert);
        b.undo();
        assert_eq!(b.text(), "a");
    }

    #[test]
    fn selections_merge_when_overlapping() {
        let mut s = Selections { list: vec![Selection::new(0, 4), Selection::new(2, 6), Selection::caret(9)], primary: 1 };
        s.normalize();
        assert_eq!(s.list, vec![Selection::new(0, 6), Selection::caret(9)]);
        assert_eq!(s.primary, 0);
    }

    #[test]
    fn line_helpers() {
        let b = Buffer::new("  (a\r\nb");
        assert_eq!(b.line_end(0), 4);
        assert_eq!(b.indentation(0), 2);
        assert_eq!(b.line_text(1), "b");
    }

    #[test]
    fn map_pos_behaviour() {
        assert_eq!(map_pos(5, &(2..4), 0, true), 3);
        assert_eq!(map_pos(3, &(2..4), 1, true), 3);
        assert_eq!(map_pos(1, &(2..4), 1, true), 1);
    }
}
