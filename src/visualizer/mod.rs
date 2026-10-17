//! The visualizer: replays a recorded run (see `racket/visualize.rkt`) as an animated diagram.
//!
//! Each step shows the global definitions and the call stack as frames on the left, and every
//! value reachable from them on the right: structs that point to structs of the same type are
//! drawn as trees (a compact node per struct, children in field order), cons cells as chains,
//! everything else as boxes. Objects keep their identity across steps, so when the program
//! builds or rebuilds a structure the boxes glide to their new places; new objects pop in and
//! changed ones flash. What the running call can't reach is faded.

mod explain;
mod sexp;

pub use explain::{Piece, explain, inline_note, output_until, show_in, work_done};
pub use explain::{Seg, seg_text, waiting};

use crate::racket::protocol::{VizObject, VizRef, VizStep};
use crate::theme::{Theme, mix, with_alpha};
use crate::ui::{self, Tokens, ic};
use egui::epaint::{CubicBezierShape, PathShape};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ------------------------------------------------------------------------------- trace

/// A recorded run: steps, plus every version of every heap object.
#[derive(Default)]
pub struct Trace {
    pub steps: Vec<VizStep>,
    /// Object id -> (step it was sent at, contents), in step order.
    versions: HashMap<u64, Vec<(usize, VizObject)>>,
    pub finished: bool,
    pub truncated: bool,
    pub max_steps: usize,
    /// Struct type -> fields that have pointed at a struct of the same type (tree links).
    links: HashMap<String, BTreeSet<usize>>,
    /// Struct type -> fields that have held an empty value ('() or #false).
    empties: HashMap<String, BTreeSet<usize>>,
    /// Struct type -> fields that have held other plain data (a number, a string...).
    data: HashMap<String, BTreeSet<usize>>,
    /// The program's top-level results so far: (step, value).
    results: Vec<(usize, VizRef)>,
    /// Struct type -> its field names (for wording `node-key`, `node?`, `make-node`).
    pub shapes: HashMap<String, Vec<String>>,
    /// Calls on the stack at each step.
    depths: Vec<usize>,
    /// The program that ran, and its forms (for what a waiting call will do next).
    pub source: Vec<char>,
    pub forms: Vec<sexp::Node>,
}

impl Trace {
    pub fn set_source(&mut self, text: &str) {
        self.source = text.chars().collect();
        self.forms = sexp::read_all(&self.source);
    }

    pub fn push(&mut self, mut step: VizStep) {
        // Consecutive definitions of functions read as one step ("Defining f and g").
        let fn_define = |s: &VizStep| s.kind == "define" && s.name.as_deref().is_some_and(|names| names.split(", ").all(|n| s.globals.iter().any(|(g, v)| g == n && matches!(v, VizRef::Prim { k, .. } if k == "fn"))));
        if fn_define(&step) && self.steps.last().is_some_and(fn_define) {
            let prev = self.steps.pop().unwrap_or_default();
            step.name = Some(format!("{}, {}", prev.name.unwrap_or_default(), step.name.clone().unwrap_or_default()));
            step.evals = [prev.evals, step.evals].concat();
            step.output = prev.output + &step.output;
        }
        let idx = self.steps.len();
        step.index = idx;
        for o in step.objects.drain(..) {
            self.versions.entry(o.id).or_default().push((idx, o.clone()));
            self.learn_shape(&o, idx);
        }
        if step.kind == "result"
            && let Some(v) = &step.value
        {
            self.results.push((idx, v.clone()));
        }
        self.depths.truncate(idx);
        self.depths.push(step.live_frames().count());
        self.steps.push(step);
    }

    /// Calls on the stack at each step (the recursion graph).
    pub fn depths(&self) -> &[usize] {
        &self.depths
    }

    /// Step over: from a call, to where it returns (skipping everything it does); otherwise
    /// the next step.
    pub fn step_over(&self, k: usize) -> usize {
        let last = self.steps.len().saturating_sub(1);
        let Some(s) = self.steps.get(k) else { return last };
        if s.kind != "call" {
            return (k + 1).min(last);
        }
        let d = self.depths[k];
        (k + 1..self.steps.len()).find(|&j| self.depths[j] < d || (self.depths[j] == d && self.steps[j].kind == "return")).unwrap_or(last)
    }

    /// Step back over: the previous step of this call or its callers, skipping calls it made.
    pub fn step_back_over(&self, k: usize) -> usize {
        let d = self.depths.get(k).copied().unwrap_or(0);
        (0..k).rev().find(|&j| self.depths[j] <= d).unwrap_or(0)
    }

    /// Step out: to where the running call returns (the end, at the top level).
    pub fn step_out(&self, k: usize) -> usize {
        let last = self.steps.len().saturating_sub(1);
        let d = self.depths.get(k).copied().unwrap_or(0);
        if d == 0 {
            return last;
        }
        (k + 1..self.steps.len()).find(|&j| self.depths[j] < d || (self.depths[j] == d && self.steps[j].kind == "return")).unwrap_or(last)
    }

    /// The next step (after `k`, wrapping around) whose code starts on `line`.
    pub fn next_on_line(&self, k: usize, line_of: &dyn Fn(usize) -> usize, line: usize) -> Option<usize> {
        let n = self.steps.len();
        (1..=n).map(|i| (k + i) % n).find(|&j| self.steps[j].pos.is_some_and(|p| line_of(p) == line))
    }

    /// Note which fields of a struct type are tree links, empties or data (children are sent
    /// before the structs that hold them, so a linked child is already known).
    fn learn_shape(&mut self, o: &VizObject, idx: usize) {
        let Some(name) = o.name.clone().filter(|_| o.k == "struct") else { return };
        self.shapes.entry(name.clone()).or_insert_with(|| o.fields.iter().map(|(f, _)| f.clone()).collect());
        for (k, (_, r)) in o.fields.iter().enumerate() {
            match r {
                VizRef::Object { id } => {
                    if self.object(*id, idx).is_some_and(|c| c.k == "struct" && c.name.as_deref() == Some(name.as_str())) {
                        self.links.entry(name.clone()).or_default().insert(k);
                    }
                }
                VizRef::Prim { k: kind, v } => {
                    let empty = kind == "empty" || (kind == "bool" && v == "#false");
                    let set = if empty { &mut self.empties } else { &mut self.data };
                    set.entry(name.clone()).or_default().insert(k);
                }
            }
        }
    }

    /// The child fields of a tree node, in field order: fields that link to the same struct
    /// type, plus fields of a linking type that have only ever been empty (a `right` that is
    /// always '() is still a child slot). `None` for anything that isn't part of a tree.
    pub fn tree_children(&self, o: &VizObject) -> Option<Vec<usize>> {
        if o.k != "struct" {
            return None;
        }
        let name = o.name.as_deref()?;
        let links = self.links.get(name).filter(|l| !l.is_empty())?;
        let data = self.data.get(name);
        let mut kids: BTreeSet<usize> = links.clone();
        for k in self.empties.get(name).into_iter().flatten() {
            if !data.is_some_and(|d| d.contains(k)) {
                kids.insert(*k);
            }
        }
        Some(kids.into_iter().collect())
    }

    /// The latest top-level result at or before `step`.
    pub fn result_at(&self, step: usize) -> Option<&VizRef> {
        self.results.iter().rev().find(|(s, _)| *s <= step).map(|(_, v)| v)
    }

    /// An object's contents as of `step`.
    pub fn object(&self, id: u64, step: usize) -> Option<&VizObject> {
        let v = self.versions.get(&id)?;
        let k = v.partition_point(|(s, _)| *s <= step);
        (k > 0).then(|| &v[k - 1].1)
    }

    /// How an object changed at `step`: `Some(true)` new, `Some(false)` modified.
    fn change_at(&self, id: u64, step: usize) -> Option<bool> {
        let v = self.versions.get(&id)?;
        let k = v.iter().position(|(s, _)| *s == step)?;
        Some(k == 0)
    }
}

// ------------------------------------------------------------------------------- view state

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Key {
    Object(u64),
    Frame(usize),
}

#[derive(Clone, Copy)]
struct Anim {
    from: Pos2,
    to: Pos2,
    start: f64,
    born: f64,
}

pub struct VisualizerView {
    pub trace: Trace,
    pub current: usize,
    pub playing: bool,
    /// Steps per second while playing.
    pub speed: f32,
    last_advance: f64,
    zoom: f32,
    pan: Vec2,
    /// Keep the whole diagram in view (until the user pans or zooms).
    pub auto_fit: bool,
    anim: HashMap<Key, Anim>,
    layout: Option<(usize, Layout)>,
    images: HashMap<u64, Arc<[u8]>>,
    /// The step the editor highlight was last set for.
    pub highlighted: Option<usize>,
    /// Height of the explanation area under the diagram (dragged by the divider).
    pub explain_h: f32,
    /// The explanation's "Show the work" section is open.
    pub show_work: bool,
    /// The object chip under the pointer in the explanation (last frame).
    pub text_hover: Option<u64>,
    /// The object under the pointer in the diagram (last frame).
    pub diagram_hover: Option<u64>,
}

/// Colours for the objects an explanation mentions, in order of mention.
pub const OBJ_COLORS: [Color32; 6] = [
    Color32::from_rgb(0x3b, 0x82, 0xf6),
    Color32::from_rgb(0xf5, 0x9e, 0x0b),
    Color32::from_rgb(0x22, 0xc5, 0x5e),
    Color32::from_rgb(0xa8, 0x55, 0xf7),
    Color32::from_rgb(0xec, 0x48, 0x99),
    Color32::from_rgb(0x14, 0xb8, 0xa6),
];

impl Default for VisualizerView {
    fn default() -> Self {
        Self {
            trace: Trace::default(),
            current: 0,
            playing: false,
            speed: 2.0,
            last_advance: 0.0,
            zoom: 1.0,
            pan: vec2(16.0, 16.0),
            auto_fit: true,
            anim: HashMap::new(),
            layout: None,
            images: HashMap::new(),
            highlighted: None,
            explain_h: 170.0,
            show_work: false,
            text_hover: None,
            diagram_hover: None,
        }
    }
}

impl VisualizerView {
    pub fn last_advance(&self) -> f64 {
        self.last_advance
    }
    pub fn set_last_advance(&mut self, t: f64) {
        self.last_advance = t;
    }
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Zoom by `factor` keeping `anchor` (relative to the canvas) in place; stops auto-fit.
    pub fn zoom_by(&mut self, factor: f32, anchor: Vec2) {
        let old = self.zoom;
        self.zoom = (self.zoom * factor).clamp(0.2, 2.5);
        self.pan -= (anchor - self.pan) * (self.zoom / old - 1.0);
        self.auto_fit = false;
    }

    pub fn step(&self) -> Option<&VizStep> {
        self.trace.steps.get(self.current)
    }
}

// ------------------------------------------------------------------------------- layout

const FRAME_W: f32 = 240.0;
const ROW_H: f32 = 26.0;
const HEADER_H: f32 = 32.0;
/// Call cards: the title, each variable, the question being asked (a decision diamond), and
/// the footer with what it does once the call it's waiting on returns.
const CARD_HEAD: f32 = 38.0;
const CARD_ROW: f32 = 24.0;
const CARD_PAD: f32 = 10.0;
const ASK_H: f32 = 32.0;
const FOOTER_H: f32 = 36.0;
/// Content's inset from the card's left edge.
const CARD_INSET: f32 = 16.0;
/// The top level without data of its own: a capsule.
const CAPSULE_H: f32 = 32.0;
/// The slot in the footer for the awaited value, in characters of the box font.
const SLOT_CHARS: f32 = 3.0;
const FONT: f32 = 13.5;
/// Vertical gap between frames, room for the connector and its label.
const FRAME_GAP: f32 = 52.0;
/// Tree nodes: height, gap between levels (room for the edge labels) and between siblings.
const NODE_H: f32 = 34.0;
const LEVEL_GAP: f32 = 52.0;
const SIBLING_GAP: f32 = 18.0;
/// Room reserved under a node for an empty child.
const STUB_W: f32 = 26.0;
const CELL_H: f32 = 32.0;
const MAX_OBJECTS: usize = 300;

#[derive(Clone)]
struct FrameBox {
    /// Animation identity: 0 for the top level, else 1 + index into the step's frames.
    key: usize,
    /// For calls: the function name and the argument values, shown as `(name 3 node 4)`.
    name: String,
    args: Vec<String>,
    rows: Vec<(String, VizRef)>,
    returns: Option<VizRef>,
    current: bool,
    top: bool,
    /// This call replaced its caller (a tail call): Racket doesn't keep the caller around.
    tail: bool,
    /// What it will do when the call it's waiting on returns: `then (bst-insert 37 ▢)`.
    then: Option<Vec<Seg>>,
    /// The awaited value, as it comes back (on the step where the call below returns).
    filled: Option<VizRef>,
    /// The question the running call is asking, and its answer (on a question step).
    asks: Option<(String, Option<bool>)>,
    rect: Rect,
}

impl FrameBox {
    fn capsule(&self) -> bool {
        self.top && self.rows.is_empty() && self.then.is_none() && self.asks.is_none()
    }

    fn rows(&self) -> usize {
        let rows = self.rows.len() + self.returns.is_some() as usize;
        if self.top { rows } else { rows.max(1) }
    }

    fn state(&self, kind: &str) -> FrameState {
        match (self.current, self.returns.is_some()) {
            (true, true) => FrameState::Returning,
            (true, false) if self.top && kind == "end" => FrameState::Done,
            (true, false) => FrameState::Running,
            (false, _) => FrameState::Waiting,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FrameState {
    Running,
    Waiting,
    Returning,
    Done,
}

#[derive(Clone)]
enum Role {
    /// A struct in a tree; `children` are the indices of its child fields.
    TreeNode { children: Vec<usize> },
    /// A cons cell in a chain.
    Cell,
    Plain,
}

#[derive(Clone)]
struct ObjBox {
    obj: VizObject,
    role: Role,
    rect: Rect,
}

#[derive(Clone, Default)]
struct Layout {
    frames: Vec<FrameBox>,
    objects: HashMap<u64, ObjBox>,
    /// Draw order.
    order: Vec<u64>,
    /// What the running call can reach (its variables, what it returns, the value just
    /// computed); everything else is drawn faded. `None`: nothing is faded.
    focus: Option<HashSet<u64>>,
}

/// Width of `n` characters of the box font at zoom 1.
fn text_w(char_w: f32, s: &str) -> f32 {
    s.chars().count() as f32 * char_w
}

/// A primitive as students read it: the empty list is `empty`, not `'()`.
pub fn prim_display<'a>(v: &'a str, k: &str) -> &'a str {
    if k == "empty" { "empty" } else { v }
}

fn prim_text(r: &VizRef) -> Option<&str> {
    match r {
        VizRef::Prim { v, k } => Some(prim_display(v, k)),
        VizRef::Object { .. } => None,
    }
}

fn refs_of(o: &VizObject) -> Vec<&VizRef> {
    let mut v: Vec<&VizRef> = Vec::new();
    v.extend(o.first.iter());
    v.extend(o.rest.iter());
    v.extend(o.fields.iter().map(|f| &f.1));
    v.extend(o.items.iter());
    for (a, b) in &o.pairs {
        v.push(a);
        v.push(b);
    }
    v
}

fn obj_id(r: &VizRef) -> Option<u64> {
    match r {
        VizRef::Object { id } => Some(*id),
        VizRef::Prim { .. } => None,
    }
}

/// Size of a plain (non-tree, non-cell) object box.
fn plain_size(o: &VizObject, cw: f32) -> Vec2 {
    match o.k.as_str() {
        "struct" | "box" => {
            let rows: Vec<(String, String)> = if o.k == "box" {
                vec![("value".into(), o.items.first().and_then(prim_text).unwrap_or("").into())]
            } else {
                o.fields.iter().map(|(n, r)| (n.clone(), prim_text(r).unwrap_or("").to_string())).collect()
            };
            let w = rows.iter().map(|(n, v)| text_w(cw, n) + text_w(cw, v).max(14.0) + 36.0).fold(text_w(cw, o.name.as_deref().unwrap_or("box")) + 24.0, f32::max);
            vec2(w.clamp(90.0, 260.0), HEADER_H + rows.len() as f32 * ROW_H + 6.0)
        }
        "vector" => {
            let w: f32 = o.items.iter().map(|r| cell_w(cw, r)).sum::<f32>().max(40.0);
            vec2(w + 8.0, CELL_H + 22.0)
        }
        "hash" => {
            let w = o.pairs.iter().map(|(k, v)| text_w(cw, prim_text(k).unwrap_or("•")) + text_w(cw, prim_text(v).unwrap_or("•")) + 40.0).fold(90.0, f32::max);
            vec2(w.min(300.0), HEADER_H + o.pairs.len().max(1) as f32 * ROW_H + 6.0)
        }
        "image" => {
            let s = vec2(o.width as f32, o.height as f32);
            let scale = (140.0 / s.x.max(s.y).max(1.0)).min(1.0);
            s * scale + vec2(12.0, 12.0)
        }
        _ => vec2((text_w(cw, o.text.as_deref().unwrap_or("?")) + 20.0).clamp(60.0, 300.0), 30.0),
    }
}

fn cell_w(cw: f32, r: &VizRef) -> f32 {
    match prim_text(r) {
        Some(t) => (text_w(cw, t) + 14.0).clamp(30.0, 140.0),
        None => 30.0,
    }
}

/// What a tree node shows: its data fields (the non-child ones), `34` or `"ann" 34`.
fn node_label(o: &VizObject, children: &[usize]) -> String {
    let data: Vec<&str> = o.fields.iter().enumerate().filter(|(k, _)| !children.contains(k)).map(|(_, (_, r))| prim_text(r).unwrap_or("•")).collect();
    if data.is_empty() { o.name.clone().unwrap_or_default() } else { data.join(" ") }
}

fn tree_node_size(o: &VizObject, children: &[usize], cw: f32) -> Vec2 {
    let w = (text_w(cw, &node_label(o, children)) + 28.0).max(NODE_H + 10.0).max(children.len() as f32 * 22.0);
    vec2(w.min(220.0), NODE_H)
}

/// A short name for a value, used for frames, explanations and chips: `12`, `node 34`,
/// `(list 3 4)`. Objects are named after their type and data, as a student would say them.
pub fn label(trace: &Trace, r: &VizRef, step: usize) -> String {
    let id = match r {
        VizRef::Prim { v, k } => return prim_display(v, k).to_string(),
        VizRef::Object { id } => *id,
    };
    let Some(o) = trace.object(id, step) else { return "…".into() };
    match o.k.as_str() {
        "struct" => {
            let name = o.name.clone().unwrap_or_else(|| "struct".into());
            let kids = trace.tree_children(o).unwrap_or_default();
            // Its data, without the links and empties: `node 34`, `posn 3 4`.
            let data: Vec<&str> = o.fields.iter().enumerate().filter(|(k, _)| !kids.contains(k)).filter(|(_, (_, r))| !matches!(r, VizRef::Prim { k, .. } if k == "empty")).filter_map(|(_, (_, r))| prim_text(r)).take(2).collect();
            if data.is_empty() { name } else { format!("{name} {}", data.join(" ")) }
        }
        "cons" => show_in(trace, r, step, 22),
        "vector" => format!("a vector of {}", o.items.len()),
        "box" => "a box".into(),
        "hash" => "a hash table".into(),
        "image" => "an image".into(),
        _ => o.text.clone().unwrap_or_else(|| "…".into()),
    }
}

fn layout(trace: &Trace, step_idx: usize, cw: f32) -> Layout {
    let mut out = Layout::default();
    let Some(step) = trace.steps.get(step_idx) else { return out };

    // Frames: the top level, then the calls on the stack, oldest first. Calls replaced by a
    // tail call are gone in Racket, so they aren't drawn; the call that replaced them says so.
    let mut y = 0.0;
    let mut push = |out: &mut Layout, mut f: FrameBox| {
        let (w, h) = if f.capsule() {
            (text_w(cw, &f.name) + 130.0, CAPSULE_H)
        } else {
            // Title and state tag; rows; the question; the footer.
            let title_w = text_w(cw, &format!("({} {})", f.name, f.args.join(" "))) + CARD_INSET + 104.0;
            let row_w = f.rows.iter().map(|(n, v)| text_w(cw, n) + text_w(cw, &label(trace, v, step_idx)).min(text_w(cw, &"x".repeat(18))) + CARD_INSET + 64.0).fold(0.0, f32::max);
            let ask_w = f.asks.as_ref().map_or(0.0, |(q, _)| text_w(cw, q) + CARD_INSET + 90.0);
            let then_w = f.then.as_ref().map_or(0.0, |t| {
                let slot = f.filled.as_ref().map_or(SLOT_CHARS, |v| label(trace, v, step_idx).chars().count() as f32 + 1.5);
                text_w(cw, &seg_text(t)) + (slot - 1.0) * cw + CARD_INSET + 84.0
            });
            let w = title_w.max(row_w).max(ask_w).max(then_w).clamp(FRAME_W, 440.0);
            let rows = f.rows();
            let body = if rows == 0 { 0.0 } else { rows as f32 * CARD_ROW + CARD_PAD };
            let h = CARD_HEAD + body + if f.asks.is_some() { ASK_H } else { 0.0 } + if f.then.is_some() { FOOTER_H } else { 0.0 };
            (w, h.max(CARD_HEAD + CARD_PAD))
        };
        f.rect = Rect::from_min_size(pos2(0.0, y), vec2(w, h));
        y += h + FRAME_GAP;
        out.frames.push(f);
    };
    // Functions are visible in the code; the top level shows the data the program defines,
    // and the program's result once there is one.
    let mut top_rows: Vec<(String, VizRef)> = step.globals.iter().filter(|(_, v)| !matches!(v, VizRef::Prim { k, .. } if k == "fn")).cloned().collect();
    if let Some(r) = trace.result_at(step_idx) {
        top_rows.push(("result".into(), r.clone()));
    }
    let live_count = step.live_frames().count();
    let then = waiting(trace, step, None);
    // The value coming back into a waiting call's slot: on a return, for the caller.
    let lands = |live_index: usize| (step.kind == "return" && live_index + 1 == live_count).then(|| step.value.clone()).flatten();
    let filled = lands(0);
    // The question the running code is asking (top level or a call).
    let asks = (step.kind == "question")
        .then(|| {
            let p = step.pos?;
            let q: String = trace.source.get(p..p + step.span)?.iter().collect();
            let answer = match &step.value {
                Some(VizRef::Prim { v, .. }) => Some(v != "#false"),
                _ => None,
            };
            Some((q.split_whitespace().collect::<Vec<_>>().join(" "), answer))
        })
        .flatten();
    push(&mut out, FrameBox { key: 0, name: "Top level".into(), args: Vec::new(), rows: top_rows, returns: None, current: live_count == 0, top: true, tail: false, then, filled, asks: if live_count == 0 { asks.clone() } else { None }, rect: Rect::NOTHING });
    let args_of = |f: &crate::racket::protocol::VizFrame| -> Vec<String> {
        let n = f.nargs.min(f.vars.len());
        f.vars[..n].iter().map(|(_, v)| label(trace, v, step_idx)).collect()
    };
    let mut seen_live = 0;
    let mut tail = false;
    for (k, f) in step.frames.iter().enumerate() {
        if f.replaced {
            tail = true;
            continue;
        }
        seen_live += 1;
        let newest = seen_live == live_count;
        let returns = if newest && step.kind == "return" { step.value.clone() } else { None };
        let then = if newest { None } else { waiting(trace, step, Some(k)) };
        let filled = if newest { None } else { lands(seen_live) };
        push(&mut out, FrameBox { key: k + 1, name: f.name.clone(), args: args_of(f), rows: f.vars.clone(), returns, current: newest, top: false, tail, then, filled, asks: if newest { asks.clone() } else { None }, rect: Rect::NOTHING });
        tail = false;
    }
    // One column, centred on a common axis (the connectors run straight where they can).
    let column = out.frames.iter().map(|f| f.rect.width()).fold(0.0, f32::max);
    for f in &mut out.frames {
        f.rect = f.rect.translate(vec2(((column - f.rect.width()) / 2.0).round(), 0.0));
    }
    let heap_x = out.frames.iter().map(|f| f.rect.width()).fold(FRAME_W, f32::max) + 80.0;
    let mut roots: Vec<&VizRef> = Vec::new();
    for f in &out.frames {
        roots.extend(f.rows.iter().map(|r| &r.1));
        roots.extend(f.returns.iter());
    }
    if matches!(step.kind.as_str(), "question" | "set" | "result") {
        roots.extend(step.value.iter());
    }

    // Reachable objects in breadth-first order (stable placement across steps).
    let reach = |from: &[&VizRef]| -> Vec<u64> {
        let mut order: Vec<u64> = Vec::new();
        let mut seen: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<u64> = from.iter().filter_map(|r| obj_id(r)).collect();
        while let Some(id) = queue.pop_front() {
            if order.len() >= MAX_OBJECTS || !seen.insert(id) {
                continue;
            }
            let Some(o) = trace.object(id, step_idx) else { continue };
            order.push(id);
            queue.extend(refs_of(o).into_iter().filter_map(obj_id));
        }
        order
    };
    let order = reach(&roots);
    // Focus: with calls on the stack, what the running one can reach stays bright.
    if let Some(cur) = out.frames.iter().find(|f| f.current && !f.top) {
        let mut from: Vec<&VizRef> = cur.rows.iter().map(|r| &r.1).chain(cur.returns.iter()).collect();
        if matches!(step.kind.as_str(), "question" | "set") {
            from.extend(step.value.iter());
        }
        out.focus = Some(reach(&from).into_iter().collect());
    }
    let obj = |id: u64| trace.object(id, step_idx);

    let mut placed: HashSet<u64> = HashSet::new();
    let mut cursor_y = 0.0f32;
    let mut plain_x = heap_x;
    let mut plain_row_h = 0.0f32;
    let flush_plain = |cursor_y: &mut f32, plain_x: &mut f32, plain_row_h: &mut f32| {
        if *plain_row_h > 0.0 {
            *cursor_y += *plain_row_h + 30.0;
        }
        *plain_x = heap_x;
        *plain_row_h = 0.0;
    };

    for &id in &order {
        if placed.contains(&id) {
            continue;
        }
        let Some(o) = obj(id) else { continue };
        if trace.tree_children(o).is_some() {
            flush_plain(&mut cursor_y, &mut plain_x, &mut plain_row_h);
            // Claim this subtree (nodes not already drawn elsewhere), then lay it out: leaves
            // left to right, parents centered over their children, empty children as stubs.
            struct TNode {
                id: u64,
                size: Vec2,
                kids: Vec<Option<usize>>, // index into nodes, or a stub
                width: f32,
                x: f32,
                depth: usize,
            }
            let mut nodes: Vec<TNode> = Vec::new();
            fn claim(id: u64, depth: usize, trace: &Trace, step: usize, placed: &mut HashSet<u64>, nodes: &mut Vec<TNode>, cw: f32) -> usize {
                placed.insert(id);
                let o = trace.object(id, step).cloned().unwrap_or_default();
                let children = trace.tree_children(&o).unwrap_or_default();
                let me = nodes.len();
                nodes.push(TNode { id, size: tree_node_size(&o, &children, cw), kids: Vec::new(), width: 0.0, x: 0.0, depth });
                let mut kids = Vec::new();
                for &k in &children {
                    let child = o.fields.get(k).and_then(|f| obj_id(&f.1));
                    let same_type = child.and_then(|c| trace.object(c, step)).is_some_and(|c| c.k == "struct" && c.name == o.name);
                    match child {
                        Some(c) if same_type && !placed.contains(&c) && depth < 40 => kids.push(Some(claim(c, depth + 1, trace, step, placed, nodes, cw))),
                        _ => kids.push(None),
                    }
                }
                nodes[me].kids = kids;
                me
            }
            claim(id, 0, trace, step_idx, &mut placed, &mut nodes, cw);
            // Subtree widths (children are always after their parent: walk backwards).
            for i in (0..nodes.len()).rev() {
                let kids_w: f32 = nodes[i].kids.iter().map(|k| k.map(|c| nodes[c].width).unwrap_or(STUB_W)).sum::<f32>() + SIBLING_GAP * nodes[i].kids.len().saturating_sub(1) as f32;
                nodes[i].width = nodes[i].size.x.max(kids_w);
            }
            // Positions, top-down.
            let mut stack = vec![(0usize, 0.0f32)];
            while let Some((i, left)) = stack.pop() {
                let kids_w: f32 = nodes[i].kids.iter().map(|k| k.map(|c| nodes[c].width).unwrap_or(STUB_W)).sum::<f32>() + SIBLING_GAP * nodes[i].kids.len().saturating_sub(1) as f32;
                let mut x = left + (nodes[i].width - kids_w) / 2.0;
                nodes[i].x = left + nodes[i].width / 2.0;
                let kids = nodes[i].kids.clone();
                for k in kids {
                    let w = k.map(|c| nodes[c].width).unwrap_or(STUB_W);
                    if let Some(c) = k {
                        stack.push((c, x));
                    }
                    x += w + SIBLING_GAP;
                }
            }
            let level_h = NODE_H + LEVEL_GAP;
            let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
            for n in &nodes {
                let o = trace.object(n.id, step_idx).cloned().unwrap_or_default();
                let children = trace.tree_children(&o).unwrap_or_default();
                let min = pos2(heap_x + n.x - n.size.x / 2.0, cursor_y + n.depth as f32 * level_h);
                out.objects.insert(n.id, ObjBox { obj: o, role: Role::TreeNode { children }, rect: Rect::from_min_size(min, n.size) });
                out.order.push(n.id);
            }
            // Room for the empty-child stubs under the deepest level.
            cursor_y += max_depth as f32 * level_h + NODE_H + 40.0;
        } else if o.k == "cons" {
            flush_plain(&mut cursor_y, &mut plain_x, &mut plain_row_h);
            // A chain of cells, left to right, until the list ends or meets a drawn cell.
            let mut x = heap_x;
            let mut cur = Some(id);
            let mut count = 0;
            while let Some(c) = cur {
                if placed.contains(&c) || count >= 40 {
                    break;
                }
                let Some(cell) = obj(c) else { break };
                if cell.k != "cons" {
                    break;
                }
                placed.insert(c);
                let w = cell.first.as_ref().map(|r| cell_w(cw, r)).unwrap_or(30.0) + 26.0;
                out.objects.insert(c, ObjBox { obj: cell.clone(), role: Role::Cell, rect: Rect::from_min_size(pos2(x, cursor_y), vec2(w, CELL_H)) });
                out.order.push(c);
                x += w + 24.0;
                count += 1;
                cur = cell.rest.as_ref().and_then(obj_id);
            }
            cursor_y += CELL_H + 34.0;
        } else {
            let size = plain_size(o, cw);
            if plain_x > heap_x && plain_x + size.x > heap_x + 720.0 {
                flush_plain(&mut cursor_y, &mut plain_x, &mut plain_row_h);
            }
            placed.insert(id);
            out.objects.insert(id, ObjBox { obj: o.clone(), role: Role::Plain, rect: Rect::from_min_size(pos2(plain_x, cursor_y), size) });
            out.order.push(id);
            plain_x += size.x + 28.0;
            plain_row_h = plain_row_h.max(size.y);
        }
    }
    out
}

// ------------------------------------------------------------------------------- drawing

struct Pen<'a> {
    ui: &'a Ui,
    cw: f32,
    theme: &'a Theme,
    tk: Tokens,
    origin: Pos2,
    zoom: f32,
    alpha: f32,
}

impl Pen<'_> {
    fn p(&self, c: Pos2) -> Pos2 {
        self.origin + c.to_vec2() * self.zoom
    }
    fn r(&self, r: Rect) -> Rect {
        Rect::from_min_max(self.p(r.min), self.p(r.max))
    }
    fn mono(&self) -> FontId {
        ui::mono(FONT * self.zoom)
    }
    fn label(&self, size: f32) -> FontId {
        ui::medium(size * self.zoom)
    }
    fn fade(&self, c: Color32) -> Color32 {
        if self.alpha >= 1.0 { c } else { c.gamma_multiply(self.alpha) }
    }
    fn prim_color(&self, k: &str) -> Color32 {
        let s = &self.theme.syntax;
        match k {
            "num" => s.number.color,
            "str" | "char" => s.string.color,
            "sym" => s.symbol.color,
            "bool" | "empty" | "void" => s.constant.color,
            "fn" => s.function.color,
            _ => self.theme.text_muted,
        }
    }
    fn box_bg(&self) -> Color32 {
        mix(self.tk.island, self.theme.text, 0.035)
    }
    /// A value slot: primitives as text, objects as a dot (the arrow is drawn later).
    /// Returns where it starts on the left.
    fn value(&self, r: &VizRef, at: Pos2, align: Align2, arrows: &mut Vec<Arrow>, from_current: bool) -> f32 {
        match r {
            VizRef::Prim { v, k } => {
                let text = if k == "fn" { format!("ƒ {v}") } else { prim_display(v, k).to_string() };
                self.ui.painter().text(at, align, text, self.mono(), self.fade(self.prim_color(k))).left()
            }
            VizRef::Object { id } => {
                let dot = if align == Align2::CENTER_CENTER { at } else { at + vec2(-4.0 * self.zoom, 0.0) };
                self.ui.painter().circle_filled(dot, 3.5 * self.zoom, self.fade(self.theme.text_accent));
                arrows.push(Arrow { from: dot, to: *id, strong: from_current, tree: false, label: None, alpha: self.alpha });
                dot.x - 3.5 * self.zoom
            }
        }
    }
    /// A frame row's value, right-aligned at `at`: primitives as text; objects as their name
    /// (`node 34`, muted) and a dot the arrow leaves from.
    /// Returns where it starts on the left.
    fn slot(&self, r: &VizRef, trace: &Trace, step: usize, at: Pos2, arrows: &mut Vec<Arrow>, from_current: bool) -> f32 {
        let left = self.value(r, at, Align2::RIGHT_CENTER, arrows, from_current);
        if let VizRef::Object { .. } = r {
            let name = label(trace, r, step);
            let name = if name.chars().count() > 18 { format!("{}…", name.chars().take(17).collect::<String>()) } else { name };
            return self.ui.painter().text(at - vec2(14.0 * self.zoom, 0.0), Align2::RIGHT_CENTER, name, self.mono(), self.fade(self.theme.text_muted)).left();
        }
        left
    }
}

/// Program text in a card, coloured like the editor: brackets muted, the function after an
/// open bracket in the function colour, numbers as numbers. Code after the slot (not run yet)
/// is a little quieter. `open`: the text before ended with an open bracket.
fn code_job(job: &mut egui::text::LayoutJob, text: &str, font: &FontId, open: &mut bool, fade: impl Fn(Color32) -> Color32, theme: &Theme, later: bool) {
    let quiet = |c: Color32| if later { mix(c, theme.text_muted, 0.45) } else { c };
    let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: fade(quiet(c)), ..Default::default() };
    let mut word = String::new();
    let flush = |word: &mut String, job: &mut egui::text::LayoutJob, open: &mut bool| {
        if word.is_empty() {
            return;
        }
        let c = if *open {
            theme.syntax.function.color
        } else if word.parse::<f64>().is_ok() {
            theme.syntax.number.color
        } else {
            theme.text
        };
        job.append(word, 0.0, fmt(c));
        word.clear();
        *open = false;
    };
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' | ')' | ']' | '}' | ' ' => {
                flush(&mut word, job, open);
                job.append(&ch.to_string(), 0.0, fmt(theme.text_muted));
                if ch != ' ' {
                    *open = matches!(ch, '(' | '[' | '{');
                }
            }
            c => word.push(c),
        }
    }
    flush(&mut word, job, open);
}

struct Arrow {
    from: Pos2,
    to: u64,
    strong: bool,
    tree: bool,
    /// A tree edge's field name (`left`, `right`).
    label: Option<String>,
    /// The source's fade (focus, fade-in).
    alpha: f32,
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

const ANIM_SECS: f64 = 0.35;

/// What the panel tells the diagram, and what the diagram reports back.
#[derive(Default)]
pub struct DrawIo {
    /// Objects the explanation mentions, with the colour of their chips: they get a ring.
    pub marks: HashMap<u64, Color32>,
    /// An object being hovered in the explanation: highlighted here.
    pub hovered_in_text: Option<u64>,
    /// Out: the object under the pointer in the diagram.
    pub hovered: Option<u64>,
    /// Out: an animation is still running.
    pub animating: bool,
}

/// The diagram for the current step.
pub fn draw(view: &mut VisualizerView, ui: &mut Ui, rect: Rect, theme: &Theme, io: &mut DrawIo) {
    let tk = Tokens::of(theme);
    let now = ui.input(|i| i.time);
    let cw = ui.ctx().fonts_mut(|f| f.glyph_width(&ui::mono(FONT), 'M'));
    let step_idx = view.current;
    if view.layout.as_ref().map(|l| l.0) != Some(step_idx) {
        let new = layout(&view.trace, step_idx, cw);
        // Start animations from wherever things are drawn right now.
        let mut anim = HashMap::new();
        let mut set = |key: Key, to: Pos2| {
            let a = match view.anim.get(&key) {
                Some(a) => {
                    let t = ease(((now - a.start) / ANIM_SECS) as f32);
                    Anim { from: a.from + (a.to - a.from) * t, to, start: now, born: a.born }
                }
                None => Anim { from: to, to, start: now, born: now },
            };
            anim.insert(key, a);
        };
        for f in &new.frames {
            set(Key::Frame(f.key), f.rect.min);
        }
        for (id, o) in &new.objects {
            set(Key::Object(*id), o.rect.min);
        }
        view.anim = anim;
        view.layout = Some((step_idx, new));
    }
    let Some((laid_step, lay)) = view.layout.take() else { return };

    // Pan (drag, trackpad) and zoom (pinch / ⌘-scroll). Clicking the diagram takes keyboard
    // focus from the editor so ← → and Space drive the visualizer.
    let resp = ui.interact(rect, egui::Id::unique("viz-canvas"), Sense::click_and_drag());
    if resp.clicked() || resp.drag_started() {
        ui.ctx().memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
    }
    if resp.dragged() {
        view.pan += resp.drag_delta();
        view.auto_fit = false;
    }
    if resp.hovered() {
        let z = ui.input(|i| i.zoom_delta());
        if z != 1.0 {
            let pointer = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let old = view.zoom;
            view.zoom = (view.zoom * z).clamp(0.2, 2.5);
            let anchor = pointer - rect.min.to_vec2() - view.pan;
            view.pan -= anchor.to_vec2() * (view.zoom / old - 1.0);
            view.auto_fit = false;
        } else {
            // Wheel / trackpad only: egui also turns arrow keys into scrolling.
            let wheel: Vec2 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { unit, delta, .. } => Some(match unit {
                            egui::MouseWheelUnit::Point => *delta,
                            egui::MouseWheelUnit::Line => *delta * 24.0,
                            egui::MouseWheelUnit::Page => *delta * rect.height(),
                        }),
                        _ => None,
                    })
                    .fold(Vec2::ZERO, |a, b| a + b)
            });
            if wheel != Vec2::ZERO {
                view.pan += wheel;
                view.auto_fit = false;
            }
        }
    }
    // Auto-fit: ease the zoom and pan toward showing the whole diagram.
    let mut fitting = false;
    if view.auto_fit {
        let bounds = lay.frames.iter().map(|f| f.rect).chain(lay.objects.values().map(|o| o.rect.expand2(vec2(0.0, 24.0)))).fold(Rect::NOTHING, |a, b| a.union(b));
        if bounds.is_positive() {
            let margin = 18.0;
            let fit = ((rect.width() - 2.0 * margin) / bounds.width()).min((rect.height() - 2.0 * margin) / bounds.height()).clamp(0.25, 1.0);
            // Centered horizontally, top-aligned.
            let target_pan = vec2(((rect.width() - bounds.width() * fit) / 2.0).max(margin), margin) - bounds.min.to_vec2() * fit;
            let k = 0.18;
            let dz = fit - view.zoom;
            let dp = target_pan - view.pan;
            if dz.abs() > 0.002 || dp.length() > 0.5 {
                view.zoom += dz * k;
                view.pan += dp * k;
                fitting = true;
            } else {
                view.zoom = fit;
                view.pan = target_pan;
            }
        }
    }

    let pos_of = |key: Key, fallback: Pos2| -> (Pos2, f32) {
        match view.anim.get(&key) {
            Some(a) => {
                let t = ease(((now - a.start) / ANIM_SECS) as f32);
                let alpha = ((now - a.born) / ANIM_SECS).clamp(0.0, 1.0) as f32;
                (a.from + (a.to - a.from) * t, alpha)
            }
            None => (fallback, 1.0),
        }
    };
    let animating = view.anim.values().any(|a| now - a.start < ANIM_SECS || now - a.born < ANIM_SECS);
    let origin = rect.min + view.pan;
    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    child_ui.set_clip_rect(rect);
    // Arrows go under the boxes: reserve their slot now, fill it once the boxes are drawn.
    let arrows_slot = child_ui.painter().add(Shape::Noop);
    let mut arrows: Vec<Arrow> = Vec::new();
    let mut boxes: HashMap<u64, Rect> = HashMap::new();
    let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p) && resp.hovered());

    // Frames, with connectors from each caller to its callee labelled with the call.
    let z = view.zoom;
    let kind = view.trace.steps.get(step_idx).map(|s| s.kind.clone()).unwrap_or_default();
    // The previous card, and its slot for the awaited value (the connector leaves from there).
    let mut prev: Option<(Rect, Option<Rect>)> = None;
    for f in &lay.frames {
        let (min, alpha) = pos_of(Key::Frame(f.key), f.rect.min);
        // Callers waiting on the running call recede a little.
        let alpha = if f.current || f.top { alpha } else { alpha * 0.75 };
        let pp = Pen { ui: &child_ui, cw, theme, tk, origin, zoom: z, alpha };
        let r = pp.r(Rect::from_min_size(min, f.rect.size()));
        let painter = child_ui.painter();
        let state = f.state(&kind);
        if let Some((p, slot)) = prev {
            // From the caller's slot (or its bottom) to this card; a value coming back
            // points up into the slot.
            let returning = f.returns.is_some();
            let color = pp.fade(if returning { theme.success } else { mix(tk.subtle_border, theme.text_accent, 0.6) });
            let head = 6.0 * z;
            let from = slot.map_or(pos2(p.center().x, p.bottom()), |s| pos2(s.center().x, s.bottom() + 2.0 * z));
            let to = pos2(r.center().x, r.top());
            let (a, b) = if returning { (from + vec2(0.0, head), to) } else { (from, to - vec2(0.0, head)) };
            let bend = (b.y - a.y) * 0.5;
            let curve: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    let u = 1.0 - t;
                    let (c1, c2) = (a + vec2(0.0, bend), b - vec2(0.0, bend));
                    pos2(
                        u * u * u * a.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.x,
                        u * u * u * a.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.y,
                    )
                })
                .collect();
            if f.tail {
                painter.extend(Shape::dashed_line(&curve, Stroke::new(1.3, color), 5.0 * z, 4.0 * z));
            } else {
                painter.add(Shape::line(curve.clone(), Stroke::new(1.3, color)));
            }
            let tip = if returning { vec![from, from + vec2(-head * 0.6, head), from + vec2(head * 0.6, head)] } else { vec![to, to + vec2(-head * 0.6, -head), to + vec2(head * 0.6, -head)] };
            painter.add(Shape::convex_polygon(tip, color, Stroke::NONE));
            // A tail call says so on its connector, in a pill on the curve (so it never needs
            // room beside it). What a call returns needs no tag: it's in the returning card,
            // and lands in the caller's slot.
            if f.tail && !returning {
                let mid = curve[curve.len() / 2];
                let g = painter.layout_no_wrap("tail call".into(), ui::medium(11.0 * z), pp.fade(theme.text_muted));
                let pill = Rect::from_center_size(mid, g.size() + vec2(14.0 * z, 6.0 * z));
                painter.rect(pill, CornerRadius::same((pill.height() / 2.0) as u8), pp.fade(theme.background), Stroke::new(1.0, pp.fade(tk.subtle_border)), StrokeKind::Inside);
                painter.galley(pill.center() - g.size() / 2.0, g, theme.text_muted);
            }
        }
        let radius = if f.capsule() { CornerRadius::same((r.height() / 2.0).min(255.0) as u8) } else { CornerRadius::same((12.0 * z) as u8) };
        let accent = match state {
            FrameState::Running => theme.text_accent,
            FrameState::Returning | FrameState::Done => theme.success,
            FrameState::Waiting => theme.text_muted,
        };
        // Depth: a soft shadow under every card; the running one glows in its colour.
        let shadow = if f.current {
            egui::epaint::Shadow { offset: [0, 2], blur: (18.0 * z).min(255.0) as u8, spread: 0, color: pp.fade(with_alpha(accent, if theme.dark { 70 } else { 55 })) }
        } else {
            egui::epaint::Shadow { offset: [0, (3.0 * z) as i8], blur: (12.0 * z).min(255.0) as u8, spread: 0, color: pp.fade(Color32::from_black_alpha(if theme.dark { 70 } else { 22 })) }
        };
        painter.add(shadow.as_shape(r, radius));
        let border = if f.current { accent } else { tk.subtle_border };
        let bg = if f.top { mix(pp.box_bg(), theme.background, 0.3) } else { pp.box_bg() };
        painter.rect_filled(r, radius, pp.fade(bg));

        // State: a pill for running / returning / done; waiting is quieter (an hourglass).
        let tag_at = |cy: f32| -> f32 {
            match state {
                FrameState::Waiting => {
                    let g = painter.layout_no_wrap("waiting".into(), ui::medium(11.0 * z), pp.fade(theme.text_muted));
                    let x = r.right() - 14.0 * z - g.size().x;
                    let left = painter.text(pos2(x - 4.0 * z, cy), Align2::RIGHT_CENTER, ic::HOURGLASS_EMPTY, ui::icon_font(13.0 * z), pp.fade(theme.text_muted)).left();
                    painter.galley(pos2(x, cy - g.size().y / 2.0), g, theme.text_muted);
                    left
                }
                _ => {
                    let tag = match state {
                        FrameState::Running => "RUNNING",
                        FrameState::Returning => "RETURNING",
                        _ => "DONE",
                    };
                    let g = painter.layout_no_wrap(tag.into(), ui::semibold(9.5 * z), pp.fade(accent));
                    let tr = Rect::from_min_size(pos2(r.right() - 12.0 * z - g.size().x - 14.0 * z, cy - 9.0 * z), vec2(g.size().x + 14.0 * z, 18.0 * z));
                    painter.rect_filled(tr, CornerRadius::same((9.0 * z) as u8), pp.fade(with_alpha(accent, 36)));
                    painter.galley(pos2(tr.left() + 7.0 * z, tr.center().y - g.size().y / 2.0), g, accent);
                    tr.left()
                }
            }
        };

        // The top level with nothing of its own: a capsule.
        if f.capsule() {
            let cy = r.center().y;
            let tag_left = tag_at(cy);
            let icon = painter.text(pos2(r.left() + 14.0 * z, cy), Align2::LEFT_CENTER, ic::HOME, ui::icon_font(14.0 * z), pp.fade(theme.text_muted));
            let mut job = egui::text::LayoutJob::simple_singleline(f.name.clone(), pp.label(12.5), pp.fade(theme.text_muted));
            job.wrap = egui::text::TextWrapping::truncate_at_width((tag_left - 8.0 * z - icon.right() - 6.0 * z).max(20.0));
            let g = painter.layout_job(job);
            painter.galley(pos2(icon.right() + 6.0 * z, cy - g.size().y / 2.0), g, theme.text_muted);
            painter.rect_stroke(r, radius, Stroke::new(if f.current { 1.6 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
            prev = Some((r, None));
            continue;
        }

        // Header: `(name arg …)` (or the top level), and the state.
        let left = r.left() + CARD_INSET * z;
        let title_y = r.top() + CARD_HEAD / 2.0 * z;
        let tag_left = tag_at(title_y);
        let title_room = (tag_left - 10.0 * z - left).max(20.0);
        if f.top {
            let icon = painter.text(pos2(left, title_y), Align2::LEFT_CENTER, ic::HOME, ui::icon_font(14.0 * z), pp.fade(theme.text_muted));
            let mut job = egui::text::LayoutJob::simple_singleline(f.name.clone(), pp.label(12.5), pp.fade(theme.text_muted));
            job.wrap = egui::text::TextWrapping::truncate_at_width((title_room - icon.width() - 6.0 * z).max(20.0));
            let g = painter.layout_job(job);
            painter.galley(pos2(icon.right() + 6.0 * z, title_y - g.size().y / 2.0), g, theme.text_muted);
        } else {
            let mut job = egui::text::LayoutJob::default();
            let font = ui::mono(14.5 * z);
            let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: pp.fade(c), ..Default::default() };
            job.append("(", 0.0, fmt(theme.text_muted));
            job.append(&f.name, 0.0, fmt(theme.syntax.function.color));
            for a in &f.args {
                job.append(" ", 0.0, fmt(theme.text));
                job.append(a, 0.0, fmt(theme.text));
            }
            job.append(")", 0.0, fmt(theme.text_muted));
            job.wrap = egui::text::TextWrapping::truncate_at_width(title_room);
            let g = painter.layout_job(job);
            painter.galley(pos2(left, title_y - g.size().y / 2.0), g, theme.text);
        }

        // Rows: `name ····· value`, right under the title.
        let mut y = CARD_HEAD - 2.0 + CARD_ROW / 2.0;
        let leader = |name_right: f32, value_left: f32, y: f32| {
            let (x0, x1) = (name_right + 8.0 * z, value_left - 8.0 * z);
            if x1 - x0 > 10.0 * z {
                painter.extend(Shape::dotted_line(&[pos2(x0, y), pos2(x1, y)], pp.fade(with_alpha(theme.text_muted, 60)), 5.0 * z, 0.8 * z));
            }
        };
        let right = r.right() - 14.0 * z;
        for (name, v) in &f.rows {
            let result = f.top && name == "result" && Some(v) == view.trace.result_at(step_idx);
            let color = if result { theme.success } else { theme.syntax.variable.color };
            let at_y = r.top() + y * z;
            let nr = painter.text(pos2(left, at_y), Align2::LEFT_CENTER, name, if result { pp.label(12.5) } else { pp.mono() }, pp.fade(color));
            let vl = pp.slot(v, &view.trace, step_idx, pos2(right, at_y), &mut arrows, f.current || result);
            leader(nr.right(), vl, at_y);
            y += CARD_ROW;
        }
        if let Some(v) = &f.returns {
            let at_y = r.top() + y * z;
            let nr = painter.text(pos2(left, at_y), Align2::LEFT_CENTER, "returns", pp.label(12.5), pp.fade(theme.success));
            let vl = pp.slot(v, &view.trace, step_idx, pos2(right, at_y), &mut arrows, true);
            leader(nr.right(), vl, at_y);
        }
        if !f.top && f.rows.is_empty() && f.returns.is_none() {
            painter.text(pos2(left, r.top() + y * z), Align2::LEFT_CENTER, "no arguments", pp.label(12.0), pp.fade(theme.text_placeholder));
        }

        // The question being asked: a decision diamond, the question, and its answer.
        let footer_h = if f.then.is_some() { FOOTER_H * z } else { 0.0 };
        if let Some((q, answer)) = &f.asks {
            let band = Rect::from_min_max(pos2(r.left(), r.bottom() - footer_h - ASK_H * z), pos2(r.right(), r.bottom() - footer_h));
            let cy = band.center().y - 2.0 * z;
            let kc = theme.syntax.keyword.color;
            let d = 8.0 * z;
            let c = pos2(left + d, cy);
            let diamond = vec![pos2(c.x, c.y - d), pos2(c.x + d, c.y), pos2(c.x, c.y + d), pos2(c.x - d, c.y)];
            painter.add(Shape::convex_polygon(diamond, pp.fade(with_alpha(kc, 45)), Stroke::new(1.2, pp.fade(kc))));
            painter.text(c, Align2::CENTER_CENTER, "?", ui::semibold(9.5 * z), pp.fade(kc));
            let (ans, ac) = match answer {
                Some(true) => ("Yes", theme.success),
                Some(false) => ("No", theme.error),
                None => ("", theme.text_muted),
            };
            let ar = painter.text(pos2(right, cy), Align2::RIGHT_CENTER, ans, ui::semibold(12.5 * z), pp.fade(ac));
            let mut job = egui::text::LayoutJob::default();
            let mut open = false;
            code_job(&mut job, q, &ui::mono(FONT * z), &mut open, |c| pp.fade(c), theme, false);
            let x = c.x + d + 8.0 * z;
            job.wrap = egui::text::TextWrapping::truncate_at_width((ar.left() - 10.0 * z - x).max(20.0));
            let g = painter.layout_job(job);
            painter.galley(pos2(x, cy - g.size().y / 2.0), g, theme.text);
        }

        // Footer: what it does once the call below returns, with a slot for that value.
        let mut slot_rect = None;
        if let Some(segs) = &f.then {
            let foot = Rect::from_min_max(pos2(r.left(), r.bottom() - footer_h), r.max);
            let rr = (12.0 * z) as u8;
            painter.rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: rr, se: rr }, pp.fade(mix(pp.box_bg(), theme.background, 0.6)));
            painter.hline(r.x_range(), foot.top(), Stroke::new(1.0, pp.fade(tk.subtle_border)));
            let cy = foot.center().y;
            let then = painter.text(pos2(left, cy), Align2::LEFT_CENTER, "THEN", ui::semibold(9.5 * z), pp.fade(theme.text_muted));
            let mut x = then.right() + 10.0 * z;
            let font = ui::mono(FONT * z);
            let mut after_hole = false;
            let mut prev_open = false;
            for sg in segs {
                if x >= right {
                    break;
                }
                match sg {
                    Seg::Code(t) => {
                        let mut job = egui::text::LayoutJob::default();
                        code_job(&mut job, t, &font, &mut prev_open, |c| pp.fade(c), theme, after_hole);
                        job.wrap = egui::text::TextWrapping::truncate_at_width(right - x);
                        let g = painter.layout_job(job);
                        let w = g.size().x;
                        painter.galley(pos2(x, cy - g.size().y / 2.0), g, theme.text);
                        x += w;
                    }
                    Seg::Value(t) => {
                        let mut job = egui::text::LayoutJob::simple_singleline(t.clone(), font.clone(), pp.fade(theme.text));
                        job.wrap = egui::text::TextWrapping::truncate_at_width(right - x - 8.0 * z);
                        let g = painter.layout_job(job);
                        let chip = Rect::from_min_size(pos2(x, cy - 10.0 * z), vec2(g.size().x + 8.0 * z, 20.0 * z));
                        painter.rect_filled(chip, CornerRadius::same((5.0 * z) as u8), pp.fade(with_alpha(theme.text, 16)));
                        painter.galley(pos2(x + 4.0 * z, cy - g.size().y / 2.0), g, theme.text);
                        x = chip.right();
                        prev_open = false;
                    }
                    Seg::Hole => {
                        after_hole = true;
                        prev_open = false;
                        let text = f.filled.as_ref().map(|v| self::label(&view.trace, v, step_idx));
                        let g = text.map(|t| painter.layout_no_wrap(t, font.clone(), pp.fade(theme.success)));
                        let w = g.as_ref().map_or(SLOT_CHARS * cw * z, |g| g.size().x + 14.0 * z).min((right - x).max(16.0 * z));
                        let sr = Rect::from_min_size(pos2(x + 1.0 * z, cy - 10.0 * z), vec2(w, 20.0 * z));
                        let round = CornerRadius::same((10.0 * z) as u8);
                        match g {
                            // The answer, landing where it's used.
                            Some(g) => {
                                painter.rect(sr, round, pp.fade(with_alpha(theme.success, 46)), Stroke::new(1.0, pp.fade(with_alpha(theme.success, 160))), StrokeKind::Inside);
                                painter.galley(pos2(sr.left() + 7.0 * z, cy - g.size().y / 2.0), g, theme.success);
                            }
                            // Still waiting: an empty slot.
                            None => {
                                let c = theme.text_accent;
                                painter.rect(sr, round, pp.fade(with_alpha(c, 40)), Stroke::new(1.0, pp.fade(with_alpha(c, 110))), StrokeKind::Inside);
                                for d in [-1.0, 0.0, 1.0] {
                                    painter.circle_filled(pos2(sr.center().x + d * 5.0 * z, cy), 1.5 * z, pp.fade(c));
                                }
                            }
                        }
                        slot_rect = Some(sr);
                        x = sr.right() + 1.0 * z;
                    }
                }
            }
        }
        painter.rect_stroke(r, radius, Stroke::new(if f.current { 1.6 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
        prev = Some((r, slot_rect));
    }

    // Objects.
    let mut hovered: Option<u64> = None;
    for id in &lay.order {
        let Some(ob) = lay.objects.get(id) else { continue };
        let (min, born) = pos_of(Key::Object(*id), ob.rect.min);
        let dim = lay.focus.as_ref().is_some_and(|f| !f.contains(id));
        let alpha = born * if dim { 0.45 } else { 1.0 };
        let pp = Pen { ui: &child_ui, cw, theme, tk, origin, zoom: view.zoom, alpha };
        // New objects pop in: they grow from slightly smaller as they fade in.
        let r = pp.r(Rect::from_min_size(min, ob.rect.size()));
        let r = Rect::from_center_size(r.center(), r.size() * (0.82 + 0.18 * ease(born)));
        boxes.insert(*id, r);
        if pointer.is_some_and(|p| r.contains(p)) {
            hovered = Some(*id);
        }
        let flash = match view.trace.change_at(*id, step_idx) {
            Some(true) => Some(theme.success),
            Some(false) => Some(theme.warning),
            None => None,
        };
        let border = flash.unwrap_or(tk.subtle_border);
        let o = &ob.obj;
        let z = view.zoom;
        let radius = CornerRadius::same((6.0 * z) as u8);
        match &ob.role {
            Role::TreeNode { children } => {
                let pill = CornerRadius::same(((r.height() / 2.0).min(14.0 * z)) as u8);
                let fill = if flash == Some(theme.success) { mix(pp.box_bg(), theme.success, 0.16) } else { pp.box_bg() };
                child_ui.painter().rect(r, pill, pp.fade(fill), Stroke::new(if flash.is_some() { 1.6 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
                let text = node_label(o, children);
                let color = if o.fields.iter().enumerate().any(|(k, (_, v))| !children.contains(&k) && matches!(v, VizRef::Prim { k, .. } if k == "num")) { theme.syntax.number.color } else { theme.text };
                child_ui.painter().text(r.center(), Align2::CENTER_CENTER, text, pp.mono(), pp.fade(color));
                // Children along the bottom edge, in field order; edges are labelled with the
                // field name so left and right read as left and right.
                let n = children.len() as f32;
                for (j, &k) in children.iter().enumerate() {
                    let px = r.left() + r.width() * (j as f32 + 0.5) / n;
                    let (name, v) = &o.fields[k];
                    match v {
                        VizRef::Object { id: c } => arrows.push(Arrow { from: pos2(px, r.bottom()), to: *c, strong: false, tree: true, label: Some(name.clone()), alpha }),
                        VizRef::Prim { .. } => {
                            // An empty child: a faint ∅ under its side.
                            let spread = (j as f32 - (n - 1.0) / 2.0) * 16.0 * z;
                            let c = pos2(px + spread, r.bottom() + 26.0 * z);
                            child_ui.painter().line_segment([pos2(px, r.bottom()), c - vec2(0.0, 7.0 * z)], Stroke::new(1.0, pp.fade(with_alpha(tk.subtle_border, 200))));
                            child_ui.painter().text(c, Align2::CENTER_CENTER, "∅", ui::mono(11.0 * z), pp.fade(theme.text_placeholder));
                        }
                    }
                }
                // Data fields that are objects (a node holding a list, say) point to the right.
                for (k, (_, v)) in o.fields.iter().enumerate() {
                    if !children.contains(&k)
                        && let VizRef::Object { id: c } = v
                    {
                        arrows.push(Arrow { from: r.right_center(), to: *c, strong: false, tree: false, label: None, alpha });
                    }
                }
            }
            Role::Cell => {
                let first_w = o.first.as_ref().map(|f| cell_w(pp.cw, f)).unwrap_or(30.0) * z;
                let split = r.left() + first_w;
                child_ui.painter().rect(r, radius, pp.fade(pp.box_bg()), Stroke::new(if flash.is_some() { 1.5 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
                child_ui.painter().vline(split, r.y_range(), Stroke::new(1.0, pp.fade(tk.subtle_border)));
                if let Some(f) = &o.first {
                    pp.value(f, pos2((r.left() + split) / 2.0, r.center().y), Align2::CENTER_CENTER, &mut arrows, false);
                }
                let rest_c = pos2((split + r.right()) / 2.0, r.center().y);
                match &o.rest {
                    Some(VizRef::Object { id: next }) => {
                        child_ui.painter().circle_filled(rest_c, 3.5 * z, pp.fade(theme.text_accent));
                        arrows.push(Arrow { from: rest_c, to: *next, strong: false, tree: false, label: None, alpha });
                    }
                    Some(VizRef::Prim { .. }) | None => {
                        // The end of the list: a slash, as in box-and-pointer diagrams.
                        let c = Rect::from_min_max(pos2(split, r.top()), r.max).shrink(6.0 * z);
                        child_ui.painter().line_segment([c.left_bottom(), c.right_top()], Stroke::new(1.2, pp.fade(theme.text_placeholder)));
                    }
                }
            }
            Role::Plain => draw_plain(&pp, &child_ui, o, r, radius, border, flash.is_some(), &mut arrows, &mut view.images),
        }
        // Mentioned in the explanation: a ring in the chip's colour (stronger when hovered).
        if let Some(c) = io.marks.get(id) {
            let strong = io.hovered_in_text == Some(*id) || hovered == Some(*id);
            let ring = r.expand(if strong { 4.0 } else { 3.0 } * z);
            let rr = CornerRadius::same(((ring.height() / 2.0).min(17.0 * z)) as u8);
            child_ui.painter().rect_stroke(ring, rr, Stroke::new(if strong { 2.5 } else { 1.6 }, *c), StrokeKind::Inside);
        }
    }

    // Arrows, under the boxes.
    let mut shapes = Vec::new();
    for a in &arrows {
        let Some(target) = boxes.get(&a.to) else { continue };
        let color = if a.tree {
            mix(tk.subtle_border, theme.text, 0.3)
        } else if a.strong {
            theme.text_accent
        } else {
            with_alpha(theme.text_accent, 150)
        };
        let color = if a.alpha < 1.0 { color.gamma_multiply(a.alpha) } else { color };
        let stroke = Stroke::new(if a.strong { 1.6 } else { 1.2 }, color);
        let (end, dir) = if a.tree || target.top() > a.from.y + 8.0 && (target.center().x - a.from.x).abs() < target.width() {
            (target.center_top(), vec2(0.0, 1.0))
        } else if target.left() >= a.from.x {
            (target.left_center(), vec2(1.0, 0.0))
        } else if target.right() <= a.from.x {
            (target.right_center(), vec2(-1.0, 0.0))
        } else {
            (target.center_top(), vec2(0.0, 1.0))
        };
        let d = (end - a.from).length().max(1.0);
        let pull = (d * 0.4).clamp(12.0, 120.0);
        let c1 = if a.tree { a.from + vec2(0.0, pull) } else { a.from + vec2(pull, 0.0) };
        let c2 = end - dir * pull;
        let curve = CubicBezierShape::from_points_stroke([a.from, c1, c2, end], false, Color32::TRANSPARENT, stroke);
        if let Some(text) = &a.label {
            // The field name, beside the edge just below where it leaves the node.
            let at = curve.sample(0.4);
            let g = child_ui.painter().layout_no_wrap(text.clone(), ui::ui_font(10.0 * view.zoom), color.gamma_multiply(0.9));
            let x = if end.x < a.from.x - 1.0 { at.x - 9.0 * view.zoom - g.size().x } else { at.x + 9.0 * view.zoom };
            shapes.push(Shape::galley(pos2(x, at.y - g.size().y / 2.0), g, color));
        }
        shapes.push(Shape::CubicBezier(curve));
        let head = 6.0 * view.zoom;
        let side = vec2(-dir.y, dir.x) * head * 0.55;
        shapes.push(Shape::Path(PathShape::convex_polygon(vec![end, end - dir * head + side, end - dir * head - side], color, Stroke::NONE)));
    }
    child_ui.painter().set(arrows_slot, Shape::Vec(shapes));

    // Hover card: a tree node's full contents.
    if let Some(id) = hovered
        && let Some(ob) = lay.objects.get(&id)
        && matches!(ob.role, Role::TreeNode { .. })
    {
        resp.clone().on_hover_ui_at_pointer(|ui| {
            ui.label(egui::RichText::new(ob.obj.name.as_deref().unwrap_or("struct")).font(ui::semibold(12.5)).color(theme.syntax.keyword.color));
            for (name, v) in &ob.obj.fields {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).font(ui::mono(12.5)).color(theme.text_muted));
                    let shown = match v {
                        VizRef::Prim { k, .. } if k == "empty" => "empty".to_string(),
                        _ => label(&view.trace, v, step_idx),
                    };
                    ui.label(egui::RichText::new(shown).font(ui::mono(12.5)).color(theme.text));
                });
            }
        });
    }
    io.hovered = hovered;
    io.animating = animating || fitting;
    view.layout = Some((laid_step, lay));
}

#[allow(clippy::too_many_arguments)]
fn draw_plain(pp: &Pen, ui: &Ui, o: &VizObject, r: Rect, radius: CornerRadius, border: Color32, flash: bool, arrows: &mut Vec<Arrow>, images: &mut HashMap<u64, Arc<[u8]>>) {
    let z = pp.zoom;
    let theme = pp.theme;
    let stroke = Stroke::new(if flash { 1.5 } else { 1.0 }, pp.fade(border));
    match o.k.as_str() {
        "struct" | "box" | "hash" => {
            ui.painter().rect(r, radius, pp.fade(pp.box_bg()), stroke, StrokeKind::Inside);
            let title = match o.k.as_str() {
                "struct" => o.name.clone().unwrap_or_default(),
                "box" => "box".into(),
                _ => "hash".into(),
            };
            ui.painter().text(r.min + vec2(10.0, HEADER_H / 2.0) * z, Align2::LEFT_CENTER, title, pp.label(12.0), pp.fade(theme.syntax.keyword.color));
            ui.painter().hline(r.x_range(), r.top() + HEADER_H * z, Stroke::new(1.0, pp.fade(pp.tk.subtle_border)));
            let mut y = HEADER_H + ROW_H / 2.0 + 2.0;
            let rows: Vec<(String, VizRef)> = match o.k.as_str() {
                "struct" => o.fields.clone(),
                "box" => o.items.iter().map(|v| ("value".to_string(), v.clone())).collect(),
                _ => o.pairs.iter().map(|(k, v)| (prim_text(k).unwrap_or("•").to_string(), v.clone())).collect(),
            };
            for (name, v) in &rows {
                ui.painter().text(r.min + vec2(10.0, y) * z, Align2::LEFT_CENTER, name, pp.mono(), pp.fade(theme.text_muted));
                pp.value(v, pos2(r.right() - 10.0 * z, r.top() + y * z), Align2::RIGHT_CENTER, arrows, false);
                y += ROW_H;
            }
        }
        "vector" => {
            let body = Rect::from_min_max(pos2(r.left(), r.top() + 18.0 * z), r.max);
            ui.painter().rect(body, radius, pp.fade(pp.box_bg()), stroke, StrokeKind::Inside);
            let mut x = body.left() + 4.0 * z;
            for (k, v) in o.items.iter().enumerate() {
                let w = cell_w(pp.cw, v) * z;
                ui.painter().text(pos2(x + w / 2.0, r.top() + 8.0 * z), Align2::CENTER_CENTER, k.to_string(), ui::mono(10.0 * z), pp.fade(theme.text_placeholder));
                if k > 0 {
                    ui.painter().vline(x, body.y_range().shrink(4.0 * z), Stroke::new(1.0, pp.fade(pp.tk.subtle_border)));
                }
                pp.value(v, pos2(x + w / 2.0, body.center().y), Align2::CENTER_CENTER, arrows, false);
                x += w;
            }
        }
        "image" => {
            ui.painter().rect(r, radius, pp.fade(Color32::WHITE), stroke, StrokeKind::Inside);
            if let Some(png) = &o.png {
                let bytes = images.entry(o.id).or_insert_with(|| {
                    use base64::Engine;
                    Arc::from(base64::engine::general_purpose::STANDARD.decode(png).unwrap_or_default())
                });
                egui::Image::from_bytes(format!("bytes://viz/{}-{}.png", o.id, png.len()), egui::load::Bytes::Shared(bytes.clone()))
                    .tint(Color32::WHITE.gamma_multiply(pp.alpha))
                    .paint_at(ui, r.shrink(6.0 * z));
            }
        }
        _ => {
            ui.painter().rect(r, radius, pp.fade(pp.box_bg()), stroke, StrokeKind::Inside);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, o.text.as_deref().unwrap_or("?"), pp.mono(), pp.fade(theme.text));
        }
    }
}

/// Icons for the toolbar (re-exported for the panel).
pub const ICON: &str = ic::ACCOUNT_TREE;

#[cfg(test)]
mod tests {
    use super::explain::show_value;
    use super::*;
    use crate::racket::protocol::VizFrame;

    fn prim(v: &str, k: &str) -> VizRef {
        VizRef::Prim { v: v.into(), k: k.into() }
    }
    fn cell(id: u64, first: VizRef, rest: VizRef) -> VizObject {
        VizObject { id, k: "cons".into(), first: Some(first), rest: Some(rest), ..Default::default() }
    }
    fn frame(name: &str, vars: Vec<(&str, VizRef)>, replaced: bool) -> VizFrame {
        VizFrame { name: name.into(), nargs: vars.len(), vars: vars.into_iter().map(|(n, v)| (n.to_string(), v)).collect(), replaced, pos: None, span: 0, done: vec![] }
    }
    /// (rvrs '(1 2) '()) -> (rvrs '(2) '(1)): the replaced call is gone, as in Racket, and
    /// the call that replaced it says so.
    #[test]
    fn tail_calls_replace_their_caller() {
        let mut view = VisualizerView::default();
        let list12 = vec![cell(1, prim("1", "num"), VizRef::Object { id: 2 }), cell(2, prim("2", "num"), prim("'()", "empty"))];
        view.trace.push(VizStep { kind: "call".into(), name: Some("rvrs".into()), frames: vec![frame("rvrs", vec![("lst", VizRef::Object { id: 1 }), ("acc", prim("'()", "empty"))], false)], objects: list12, ..Default::default() });
        let acc = vec![cell(3, prim("1", "num"), prim("'()", "empty"))];
        view.trace.push(VizStep {
            kind: "call".into(),
            name: Some("rvrs".into()),
            frames: vec![frame("rvrs", vec![("lst", VizRef::Object { id: 1 }), ("acc", prim("'()", "empty"))], true), frame("rvrs", vec![("lst", VizRef::Object { id: 2 }), ("acc", VizRef::Object { id: 3 })], false)],
            objects: acc,
            ..Default::default()
        });
        let lay = layout(&view.trace, 1, 8.0);
        let frames: Vec<_> = lay.frames.iter().map(|f| (f.name.clone(), f.args.join(" "), f.tail)).collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1], ("rvrs".into(), "(list 2) (list 1)".into(), true));
    }

    /// f calls g twice; step over a call lands on its return, step out leaves the call.
    #[test]
    fn stepping_over_and_out_of_calls() {
        let mut view = VisualizerView::default();
        let f = frame("f", vec![], false);
        let g = frame("g", vec![], false);
        let steps = [("call", vec![f.clone()]), ("call", vec![f.clone(), g.clone()]), ("question", vec![f.clone(), g.clone()]), ("return", vec![f.clone(), g.clone()]), ("call", vec![f.clone(), g.clone()]), ("return", vec![f.clone(), g]), ("return", vec![f]), ("end", vec![])];
        for (kind, frames) in steps {
            view.trace.push(VizStep { kind: kind.into(), frames, ..Default::default() });
        }
        let t = &view.trace;
        assert_eq!(t.depths(), &[1, 2, 2, 2, 2, 2, 1, 0]);
        assert_eq!(t.step_over(1), 3, "the first call to g, over to its return");
        assert_eq!(t.step_over(2), 3, "not a call: just the next step");
        assert_eq!(t.step_out(2), 3, "out of g: where it returns");
        assert_eq!(t.step_out(4), 5);
        assert_eq!(t.step_out(7), 7);
        assert_eq!(t.step_back_over(4), 3);
        assert_eq!(t.step_back_over(6), 0, "back over g's calls to f's call");
    }

    #[test]
    fn top_level_hides_functions() {
        let mut view = VisualizerView::default();
        view.trace.push(VizStep { kind: "define".into(), globals: vec![("f".into(), prim("f", "fn")), ("x".into(), prim("5", "num"))], ..Default::default() });
        let lay = layout(&view.trace, 0, 8.0);
        assert_eq!(lay.frames[0].rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["x"]);
    }

    /// A BST whose `right` was only ever empty is still a tree: both fields are child slots.
    #[test]
    fn trees_keep_children_that_were_always_empty() {
        let node = |id: u64, key: &str, left: VizRef| VizObject { id, k: "struct".into(), name: Some("node".into()), fields: vec![("key".into(), prim(key, "num")), ("left".into(), left), ("right".into(), prim("'()", "empty"))], ..Default::default() };
        let mut view = VisualizerView::default();
        // Children are sent before their parents.
        view.trace.push(VizStep { kind: "result".into(), value: Some(VizRef::Object { id: 2 }), objects: vec![node(1, "18", prim("'()", "empty")), node(2, "34", VizRef::Object { id: 1 })], ..Default::default() });
        let n34 = view.trace.object(2, 0).cloned().unwrap();
        assert_eq!(view.trace.tree_children(&n34), Some(vec![1, 2]));
        assert_eq!(label(&view.trace, &VizRef::Object { id: 2 }, 0), "node 34");
        let lay = layout(&view.trace, 0, 8.0);
        // The result shows at the top level, and 18 sits below 34.
        assert_eq!(lay.frames[0].rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["result"]);
        assert!(lay.objects[&1].rect.top() > lay.objects[&2].rect.bottom());
    }

    #[test]
    fn shows_structs_and_improper_pairs() {
        let mut view = VisualizerView::default();
        let node = VizObject { id: 1, k: "struct".into(), name: Some("node".into()), fields: vec![("left".into(), prim("'()", "empty")), ("key".into(), prim("5", "num")), ("right".into(), prim("'()", "empty"))], ..Default::default() };
        let pair = cell(2, prim("1", "num"), prim("2", "num"));
        view.trace.push(VizStep { objects: vec![node, pair], ..Default::default() });
        assert_eq!(show_value(&view, &VizRef::Object { id: 1 }, 0, 60), "(make-node empty 5 empty)");
        assert_eq!(show_value(&view, &VizRef::Object { id: 2 }, 0, 60), "(cons 1 2)");
    }
}
