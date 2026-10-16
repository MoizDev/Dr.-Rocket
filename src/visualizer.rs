//! The visualizer: replays a recorded run (see `racket/visualize.rkt`) as an animated diagram.
//!
//! Each step shows the global definitions and the call stack as frames on the left, and every
//! value reachable from them on the right: structs that point to structs of the same type are
//! laid out as trees, cons cells as chains, everything else as boxes. Objects keep their
//! identity across steps, so when the program builds or rebuilds a structure the boxes glide
//! to their new places; new objects fade in and changed ones flash.

use crate::racket::protocol::{VizObject, VizRef, VizStep};
use crate::theme::{Theme, mix, with_alpha};
use crate::ui::{self, Tokens, ic};
use egui::epaint::{CubicBezierShape, PathShape};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use std::collections::{HashMap, HashSet, VecDeque};
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
}

impl Trace {
    pub fn push(&mut self, mut step: VizStep) {
        let idx = self.steps.len();
        for o in step.objects.drain(..) {
            self.versions.entry(o.id).or_default().push((idx, o));
        }
        self.steps.push(step);
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
    anim: HashMap<Key, Anim>,
    layout: Option<(usize, Layout)>,
    images: HashMap<u64, Arc<[u8]>>,
    /// The step the editor highlight was last set for.
    pub highlighted: Option<usize>,
}

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
            anim: HashMap::new(),
            layout: None,
            images: HashMap::new(),
            highlighted: None,
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
    pub fn step(&self) -> Option<&VizStep> {
        self.trace.steps.get(self.current)
    }
}

// ------------------------------------------------------------------------------- layout

const FRAME_W: f32 = 230.0;
const HEAP_X: f32 = FRAME_W + 70.0;
const ROW_H: f32 = 24.0;
const HEADER_H: f32 = 26.0;
const FONT: f32 = 12.5;
const LEVEL_GAP: f32 = 46.0;
const SIBLING_GAP: f32 = 22.0;
const STUB_W: f32 = 30.0;
const CELL_H: f32 = 32.0;
const MAX_OBJECTS: usize = 300;

#[derive(Clone)]
struct FrameBox {
    title: String,
    rows: Vec<(String, VizRef)>,
    returns: Option<VizRef>,
    current: bool,
    global: bool,
    rect: Rect,
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
}

/// Width of `n` characters of the box font at zoom 1.
fn text_w(char_w: f32, s: &str) -> f32 {
    s.chars().count() as f32 * char_w
}

fn prim_text(r: &VizRef) -> Option<&str> {
    match r {
        VizRef::Prim { v, .. } => Some(v),
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

fn tree_node_size(o: &VizObject, children: &[usize], cw: f32) -> Vec2 {
    let shown: Vec<&(String, VizRef)> = o.fields.iter().enumerate().filter(|(k, _)| !children.contains(k)).map(|(_, f)| f).collect();
    let mut w = text_w(cw, o.name.as_deref().unwrap_or("")) + 24.0;
    for (n, r) in &shown {
        w = w.max(text_w(cw, n) + text_w(cw, prim_text(r).unwrap_or("")).max(14.0) + 36.0);
    }
    w = w.max(children.len() as f32 * 46.0);
    let ports = if children.is_empty() { 0.0 } else { 20.0 };
    vec2(w.clamp(80.0, 220.0), HEADER_H + shown.len() as f32 * ROW_H + ports + 4.0)
}

fn frame_height(rows: usize, returns: bool) -> f32 {
    HEADER_H + (rows + returns as usize).max(1) as f32 * ROW_H + 8.0
}

fn layout(trace: &Trace, step_idx: usize, cw: f32) -> Layout {
    let mut out = Layout::default();
    let Some(step) = trace.steps.get(step_idx) else { return out };

    // Frames: Global, then the call stack, oldest first.
    let mut y = 0.0;
    let mut roots: Vec<&VizRef> = Vec::new();
    let mut push_frame = |out: &mut Layout, title: String, rows: Vec<(String, VizRef)>, returns: Option<VizRef>, current: bool, global: bool| {
        let h = frame_height(rows.len(), returns.is_some());
        out.frames.push(FrameBox { title, rows, returns, current, global, rect: Rect::from_min_size(pos2(0.0, y), vec2(FRAME_W, h)) });
        y += h + 16.0;
    };
    push_frame(&mut out, "Global".into(), step.globals.clone(), None, step.frames.is_empty(), true);
    let n = step.frames.len();
    for (k, f) in step.frames.iter().enumerate() {
        let newest = k + 1 == n;
        let returns = if newest && step.kind == "return" { step.value.clone() } else { None };
        let args: Vec<&str> = f.vars.iter().map(|v| v.0.as_str()).collect();
        push_frame(&mut out, format!("{} ({})", f.name, args.join(" ")), f.vars.clone(), returns, newest, false);
    }
    for f in &out.frames {
        roots.extend(f.rows.iter().map(|r| &r.1));
        roots.extend(f.returns.iter());
    }
    if step.kind == "question" || step.kind == "set" {
        roots.extend(step.value.iter());
    }

    // Reachable objects in breadth-first order (stable placement across steps).
    let mut order: Vec<u64> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut queue: VecDeque<u64> = roots.iter().filter_map(|r| obj_id(r)).collect();
    while let Some(id) = queue.pop_front() {
        if order.len() >= MAX_OBJECTS || !seen.insert(id) {
            continue;
        }
        let Some(o) = trace.object(id, step_idx) else { continue };
        order.push(id);
        queue.extend(refs_of(o).into_iter().filter_map(obj_id));
    }
    let obj = |id: u64| trace.object(id, step_idx);

    // Tree types: struct fields that point at structs of the same type are child fields.
    let mut child_fields: HashMap<String, Vec<usize>> = HashMap::new();
    for &id in &order {
        let Some(o) = obj(id) else { continue };
        if o.k != "struct" {
            continue;
        }
        let name = o.name.clone().unwrap_or_default();
        for (k, (_, r)) in o.fields.iter().enumerate() {
            if let Some(c) = obj_id(r).and_then(obj)
                && c.k == "struct"
                && c.name.as_deref() == Some(name.as_str())
            {
                let e = child_fields.entry(name.clone()).or_default();
                if !e.contains(&k) {
                    e.push(k);
                }
            }
        }
    }
    for v in child_fields.values_mut() {
        v.sort();
    }
    let tree_children = |o: &VizObject| -> Option<Vec<usize>> {
        if o.k != "struct" {
            return None;
        }
        child_fields.get(o.name.as_deref().unwrap_or("")).cloned()
    };

    let mut placed: HashSet<u64> = HashSet::new();
    let mut cursor_y = 0.0f32;
    let mut plain_x = HEAP_X;
    let mut plain_row_h = 0.0f32;
    let flush_plain = |cursor_y: &mut f32, plain_x: &mut f32, plain_row_h: &mut f32| {
        if *plain_row_h > 0.0 {
            *cursor_y += *plain_row_h + 30.0;
        }
        *plain_x = HEAP_X;
        *plain_row_h = 0.0;
    };

    for &id in &order {
        if placed.contains(&id) {
            continue;
        }
        let Some(o) = obj(id) else { continue };
        if tree_children(o).is_some() {
            flush_plain(&mut cursor_y, &mut plain_x, &mut plain_row_h);
            // Claim this subtree (nodes not already drawn elsewhere), then lay it out:
            // leaves left to right, parents centered over their children.
            struct TNode {
                id: u64,
                size: Vec2,
                kids: Vec<Option<usize>>, // index into nodes, or a stub
                width: f32,
                x: f32,
                depth: usize,
            }
            let mut nodes: Vec<TNode> = Vec::new();
            fn claim(
                id: u64,
                depth: usize,
                obj: &dyn Fn(u64) -> Option<VizObject>,
                tree_children: &dyn Fn(&VizObject) -> Option<Vec<usize>>,
                placed: &mut HashSet<u64>,
                nodes: &mut Vec<TNode>,
                cw: f32,
            ) -> usize {
                placed.insert(id);
                let o = obj(id).unwrap_or_default();
                let children = tree_children(&o).unwrap_or_default();
                let me = nodes.len();
                nodes.push(TNode { id, size: tree_node_size(&o, &children, cw), kids: Vec::new(), width: 0.0, x: 0.0, depth });
                let mut kids = Vec::new();
                for &k in &children {
                    let child = o.fields.get(k).and_then(|f| obj_id(&f.1));
                    let same_type = child.and_then(obj).is_some_and(|c| c.k == "struct" && c.name == o.name);
                    match child {
                        Some(c) if same_type && !placed.contains(&c) && depth < 40 => kids.push(Some(claim(c, depth + 1, obj, tree_children, placed, nodes, cw))),
                        _ => kids.push(None),
                    }
                }
                nodes[me].kids = kids;
                me
            }
            let obj_owned = |id: u64| trace.object(id, step_idx).cloned();
            let tc = |o: &VizObject| tree_children(o);
            claim(id, 0, &obj_owned, &tc, &mut placed, &mut nodes, cw);
            // Subtree widths (children are always after their parent: walk backwards).
            for i in (0..nodes.len()).rev() {
                let kids_w: f32 = nodes[i].kids.iter().map(|k| k.map(|c| nodes[c].width).unwrap_or(STUB_W)).sum::<f32>()
                    + SIBLING_GAP * nodes[i].kids.len().saturating_sub(1) as f32;
                nodes[i].width = nodes[i].size.x.max(kids_w);
            }
            // Positions, top-down.
            let mut stack = vec![(0usize, 0.0f32)];
            while let Some((i, left)) = stack.pop() {
                let kids_w: f32 = nodes[i].kids.iter().map(|k| k.map(|c| nodes[c].width).unwrap_or(STUB_W)).sum::<f32>()
                    + SIBLING_GAP * nodes[i].kids.len().saturating_sub(1) as f32;
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
            let level_h = nodes.iter().map(|n| n.size.y).fold(0.0, f32::max) + LEVEL_GAP;
            let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
            for n in &nodes {
                let o = trace.object(n.id, step_idx).cloned().unwrap_or_default();
                let children = tree_children(&o).unwrap_or_default();
                let min = pos2(HEAP_X + n.x - n.size.x / 2.0, cursor_y + n.depth as f32 * level_h);
                out.objects.insert(n.id, ObjBox { obj: o, role: Role::TreeNode { children }, rect: Rect::from_min_size(min, n.size) });
                out.order.push(n.id);
            }
            cursor_y += (max_depth + 1) as f32 * level_h + 10.0;
        } else if o.k == "cons" {
            flush_plain(&mut cursor_y, &mut plain_x, &mut plain_row_h);
            // A chain of cells, left to right, until the list ends or meets a drawn cell.
            let mut x = HEAP_X;
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
            if plain_x > HEAP_X && plain_x + size.x > HEAP_X + 720.0 {
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
    fn value(&self, r: &VizRef, at: Pos2, align: Align2, arrows: &mut Vec<Arrow>, from_current: bool) {
        match r {
            VizRef::Prim { v, k } => {
                let text = if k == "fn" { format!("ƒ {v}") } else { v.clone() };
                self.ui.painter().text(at, align, text, self.mono(), self.fade(self.prim_color(k)));
            }
            VizRef::Object { id } => {
                let dot = if align == Align2::CENTER_CENTER { at } else { at + vec2(-4.0 * self.zoom, 0.0) };
                self.ui.painter().circle_filled(dot, 3.5 * self.zoom, self.fade(self.theme.text_accent));
                arrows.push(Arrow { from: dot, to: *id, strong: from_current, tree: false });
            }
        }
    }
}

struct Arrow {
    from: Pos2,
    to: u64,
    strong: bool,
    tree: bool,
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

const ANIM_SECS: f64 = 0.35;

/// The diagram for the current step. Returns whether an animation is still running.
pub fn draw(view: &mut VisualizerView, ui: &mut Ui, rect: Rect, theme: &Theme) -> bool {
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
        for (k, f) in new.frames.iter().enumerate() {
            set(Key::Frame(k), f.rect.min);
        }
        for (id, o) in &new.objects {
            set(Key::Object(*id), o.rect.min);
        }
        view.anim = anim;
        view.layout = Some((step_idx, new));
    }
    let Some((laid_step, lay)) = view.layout.take() else { return false };

    // Pan (drag) and zoom (pinch / ⌘-scroll).
    let resp = ui.interact(rect, egui::Id::unique("viz-canvas"), Sense::drag());
    if resp.dragged() {
        view.pan += resp.drag_delta();
    }
    if resp.hovered() {
        let z = ui.input(|i| i.zoom_delta());
        if z != 1.0 {
            let pointer = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let old = view.zoom;
            view.zoom = (view.zoom * z).clamp(0.3, 2.5);
            let anchor = pointer - rect.min.to_vec2() - view.pan;
            view.pan -= anchor.to_vec2() * (view.zoom / old - 1.0);
        } else {
            let scroll = ui.input(|i| i.smooth_scroll_delta);
            view.pan += scroll;
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

    // Frames.
    for (k, f) in lay.frames.iter().enumerate() {
        let (min, alpha) = pos_of(Key::Frame(k), f.rect.min);
        let pp = Pen { ui: &child_ui, cw, theme, tk, origin, zoom: view.zoom, alpha };
        let r = pp.r(Rect::from_min_size(min, f.rect.size()));
        let border = if f.current { theme.text_accent } else { tk.subtle_border };
        let bg = if f.global { with_alpha(pp.box_bg(), 200) } else { pp.box_bg() };
        child_ui.painter().rect(r, CornerRadius::same((8.0 * view.zoom) as u8), pp.fade(bg), Stroke::new(if f.current { 1.5 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
        let title_color = if f.global { theme.text_muted } else { theme.syntax.function.color };
        child_ui.painter().text(r.min + vec2(10.0, HEADER_H / 2.0) * view.zoom, Align2::LEFT_CENTER, &f.title, pp.label(12.5), pp.fade(title_color));
        child_ui.painter().hline(r.x_range(), r.top() + HEADER_H * view.zoom, Stroke::new(1.0, pp.fade(tk.subtle_border)));
        let mut y = HEADER_H + ROW_H / 2.0 + 4.0;
        if f.rows.is_empty() && f.returns.is_none() {
            child_ui.painter().text(r.min + vec2(10.0, y) * view.zoom, Align2::LEFT_CENTER, "no definitions yet", pp.label(11.5), pp.fade(theme.text_placeholder));
        }
        for (name, v) in &f.rows {
            child_ui.painter().text(r.min + vec2(10.0, y) * view.zoom, Align2::LEFT_CENTER, name, pp.mono(), pp.fade(theme.syntax.variable.color));
            pp.value(v, pos2(r.right() - 12.0 * view.zoom, r.top() + y * view.zoom), Align2::RIGHT_CENTER, &mut arrows, f.current);
            y += ROW_H;
        }
        if let Some(v) = &f.returns {
            child_ui.painter().text(r.min + vec2(10.0, y) * view.zoom, Align2::LEFT_CENTER, "returns", pp.label(12.0), pp.fade(theme.success));
            pp.value(v, pos2(r.right() - 12.0 * view.zoom, r.top() + y * view.zoom), Align2::RIGHT_CENTER, &mut arrows, true);
        }
    }

    // Objects.
    for id in &lay.order {
        let Some(ob) = lay.objects.get(id) else { continue };
        let (min, alpha) = pos_of(Key::Object(*id), ob.rect.min);
        let pp = Pen { ui: &child_ui, cw, theme, tk, origin, zoom: view.zoom, alpha };
        let r = pp.r(Rect::from_min_size(min, ob.rect.size()));
        boxes.insert(*id, r);
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
                child_ui.painter().rect(r, radius, pp.fade(pp.box_bg()), Stroke::new(if flash.is_some() { 1.5 } else { 1.0 }, pp.fade(border)), StrokeKind::Inside);
                child_ui.painter().text(pos2(r.center().x, r.top() + HEADER_H / 2.0 * z), Align2::CENTER_CENTER, o.name.as_deref().unwrap_or(""), pp.label(12.0), pp.fade(theme.syntax.keyword.color));
                let mut y = HEADER_H + ROW_H / 2.0;
                for (k, (name, v)) in o.fields.iter().enumerate() {
                    if children.contains(&k) {
                        continue;
                    }
                    child_ui.painter().text(r.min + vec2(10.0, y) * z, Align2::LEFT_CENTER, name, pp.mono(), pp.fade(theme.text_muted));
                    pp.value(v, pos2(r.right() - 10.0 * z, r.top() + y * z), Align2::RIGHT_CENTER, &mut arrows, false);
                    y += ROW_H;
                }
                // Child ports along the bottom edge.
                let n = children.len() as f32;
                for (j, &k) in children.iter().enumerate() {
                    let px = r.left() + r.width() * (j as f32 + 0.5) / n;
                    let py = r.bottom() - 10.0 * z;
                    let (name, v) = &o.fields[k];
                    child_ui.painter().text(pos2(px, py), Align2::CENTER_CENTER, name, ui::ui_font(10.0 * z), pp.fade(theme.text_placeholder));
                    match v {
                        VizRef::Object { id: c } => arrows.push(Arrow { from: pos2(px, r.bottom()), to: *c, strong: false, tree: true }),
                        VizRef::Prim { v, k } => {
                            // An empty child: a small stub under the port.
                            let stub = Rect::from_center_size(pos2(px, r.bottom() + 18.0 * z), vec2(STUB_W - 6.0, 16.0) * z);
                            child_ui.painter().line_segment([pos2(px, r.bottom()), stub.center_top()], Stroke::new(1.0, pp.fade(tk.subtle_border)));
                            child_ui.painter().rect_stroke(stub, CornerRadius::same((4.0 * z) as u8), Stroke::new(1.0, pp.fade(tk.subtle_border)), StrokeKind::Inside);
                            let short = if k == "empty" { "∅" } else { v.as_str() };
                            child_ui.painter().text(stub.center(), Align2::CENTER_CENTER, short, ui::mono(10.0 * z), pp.fade(theme.text_placeholder));
                        }
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
                        arrows.push(Arrow { from: rest_c, to: *next, strong: false, tree: false });
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
    }

    // Arrows, under the boxes.
    let mut shapes = Vec::new();
    for a in &arrows {
        let Some(target) = boxes.get(&a.to) else { continue };
        let color = if a.tree {
            mix(tk.subtle_border, theme.text, 0.25)
        } else if a.strong {
            theme.text_accent
        } else {
            with_alpha(theme.text_accent, 150)
        };
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
        shapes.push(Shape::CubicBezier(CubicBezierShape::from_points_stroke([a.from, c1, c2, end], false, Color32::TRANSPARENT, stroke)));
        let head = 6.0 * view.zoom;
        let side = vec2(-dir.y, dir.x) * head * 0.55;
        shapes.push(Shape::Path(PathShape::convex_polygon(vec![end, end - dir * head + side, end - dir * head - side], color, Stroke::NONE)));
    }
    child_ui.painter().set(arrows_slot, Shape::Vec(shapes));
    view.layout = Some((laid_step, lay));
    animating
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

// ------------------------------------------------------------------------------- explanation

/// A one-line, plain-language description of a step. `code` reads source text by range.
pub fn explain(view: &VisualizerView, step: &VizStep, code: &dyn Fn(usize, usize) -> String) -> String {
    let describe = |r: &VizRef| -> String {
        match r {
            VizRef::Prim { v, .. } => v.clone(),
            VizRef::Object { id } => match view.trace.object(*id, step.index) {
                Some(o) => match o.k.as_str() {
                    "struct" => format!("a {}", o.name.as_deref().unwrap_or("struct")),
                    "cons" => "a list".into(),
                    "vector" => format!("a vector of {}", o.len.unwrap_or(o.items.len())),
                    "image" => "an image".into(),
                    k => format!("a {k}"),
                },
                None => "a value".into(),
            },
        }
    };
    let src = || step.pos.map(|p| code(p, step.span)).unwrap_or_default();
    match step.kind.as_str() {
        "call" => {
            let frame = step.frames.last();
            let args: Vec<String> = frame.map(|f| f.vars.iter().map(|(n, v)| format!("{n} = {}", describe(v))).collect()).unwrap_or_default();
            let name = step.name.as_deref().unwrap_or("λ");
            if args.is_empty() { format!("Calling {name}") } else { format!("Calling {name} with {}", args.join(", ")) }
        }
        "return" => format!("{} returns {}", step.name.as_deref().unwrap_or("λ"), step.value.as_ref().map(describe).unwrap_or_else(|| "nothing".into())),
        "question" => {
            let answer = step.value.as_ref().map(describe).unwrap_or_default();
            let verdict = match answer.as_str() {
                "#true" => "so this branch is taken",
                "#false" => "so this branch is skipped",
                _ => "",
            };
            format!("{} is {answer}{}{verdict}", src(), if verdict.is_empty() { "" } else { ", " })
        }
        "define" => format!("Defined {}", step.name.as_deref().unwrap_or("")),
        "set" => format!("{} is now {}", step.name.as_deref().unwrap_or(""), step.value.as_ref().map(describe).unwrap_or_default()),
        "end" => {
            if step.failed == Some(true) {
                "The program stopped with an error (see Interactions).".into()
            } else {
                "The program finished.".into()
            }
        }
        k => k.to_string(),
    }
}

/// Icons for the toolbar (re-exported for the panel).
pub const ICON: &str = ic::ACCOUNT_TREE;
