//! Explanations: each step of a run in plain language.
//!
//! Code is put into words wherever a student would say it differently: `(node-key t)` is
//! "t's key", `(empty? t)` asks "Is t empty?", `(< k (node-key t))` asks "Is k less than t's
//! key?". Struct accessors, predicates and constructors come from the program's own structs.
//! Values are chips: plain values as text, objects by name (`node 34`) in a colour that the
//! diagram repeats as a ring around the same object. Each step says what happened in a
//! headline and a few short lines; the arithmetic behind it is in `work`.

use super::sexp::{self, Node};
use super::{Trace, VisualizerView, label};
use crate::racket::protocol::{VizFrame, VizRef, VizStep};
use std::collections::HashMap;

/// A piece of an explanation line.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// A name from the program (a variable or a function), in code style.
    Name(String),
    /// Program text that isn't put into words, in a code chip.
    Code(String),
    /// Code the step worked out along the way (`(random 100)`), in the colour it's marked
    /// with in the editor.
    Work(String),
    /// A plain value (a number, a string, `empty`), in a value chip.
    Value(String),
    /// An object, by name (`node 34`), in a chip of its own colour.
    Obj { id: u64, label: String },
    /// The answer to a question.
    Answer(bool),
}

/// A step in plain language.
#[derive(Clone, Debug, Default)]
pub struct Explanation {
    /// The step's code (shown beside the step kind).
    pub code: String,
    pub headline: Vec<Piece>,
    pub lines: Vec<Vec<Piece>>,
    /// How the values were worked out, one operation per line ("Show the work").
    pub work: Vec<Vec<Piece>>,
}

impl Explanation {
    /// The objects mentioned, in order of first mention (each gets a colour).
    pub fn objects(&self) -> Vec<u64> {
        let mut out = Vec::new();
        for p in self.headline.iter().chain(self.lines.iter().flatten()).chain(self.work.iter().flatten()) {
            if let Piece::Obj { id, .. } = p
                && !out.contains(id)
            {
                out.push(*id);
            }
        }
        out
    }
}

/// A one-line note for the code a step is about, written beside it in the editor:
/// `k is 12 and t's key is 34  → Yes`, `k = 12, t = node 18`, `→ returns node 34`.
pub fn inline_note(step: &VizStep, ex: &Explanation) -> Option<String> {
    match step.kind.as_str() {
        "question" => {
            let answer = ex.headline.iter().find_map(|p| if let Piece::Answer(a) = p { Some(*a) } else { None })?;
            let values = ex.lines.first().map(|l| plain(l)).filter(|s| s.starts_with("Here ")).map(|s| s["Here ".len()..].to_string());
            let a = if answer { "Yes" } else { "No" };
            Some(match values {
                Some(v) => format!("{v}  → {a}"),
                None => format!("→ {a}"),
            })
        }
        "call" => {
            let args: Vec<String> = ex
                .lines
                .iter()
                .filter_map(|l| match l.as_slice() {
                    [Piece::Name(n), Piece::Text(is), v, ..] if is == " is " => Some(format!("{n} = {}", plain(std::slice::from_ref(v)))),
                    _ => None,
                })
                .collect();
            (!args.is_empty()).then(|| args.join(", "))
        }
        "return" => Some(format!("→ {}", plain(ex.headline.get(1..)?).trim_start())),
        _ => None,
    }
}

/// Code a step worked out on the way (a primitive operation like `(random 100)`), shown in
/// a second colour in the editor so it's clear it ran.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    pub range: std::ops::Range<usize>,
    /// `(random 100) → 37`, written after the line: for work outside the step's own code
    /// (whose values the step's note already gives) and not inside other work.
    pub note: Option<String>,
}

/// Everything the step worked out since the previous one, other than its own code, in the
/// order it ran. `code` reads source text by (position, length).
pub fn work_done(trace: &Trace, step: &VizStep, code: &dyn Fn(usize, usize) -> String) -> Vec<Work> {
    let own = step.pos.map(|p| p..p + step.span.max(1));
    // The same code can run more than once between steps: the last run is the one that counts.
    let mut done: Vec<(std::ops::Range<usize>, Option<&VizRef>)> = Vec::new();
    for e in &step.evals {
        let Some(p) = e.pos else { continue };
        let r = p..p + e.span.max(1);
        if Some(&r) == own.as_ref() {
            continue;
        }
        done.retain(|(d, _)| *d != r);
        done.push((r, e.value.as_ref()));
    }
    let inside = |a: &std::ops::Range<usize>, b: &std::ops::Range<usize>| a != b && b.start <= a.start && a.end <= b.end;
    done.iter()
        .map(|(r, v)| {
            let shown = own.as_ref().is_none_or(|o| !inside(r, o) && r != o) && !done.iter().any(|(d, _)| inside(r, d));
            let note = shown.then_some(*v).flatten().map(|v| {
                let text = one_line(&code(r.start, r.end - r.start));
                let text = if text.chars().count() > 32 { format!("{}…", text.chars().take(31).collect::<String>()) } else { text };
                format!("{text} → {}", label(trace, v, step.index))
            });
            Work { range: r.clone(), note }
        })
        .collect()
}

/// A piece of what a waiting call will do next: `(bst-insert 37 •••)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Code(String),
    /// A value already worked out (or a variable's value), shown in place of its code.
    Value(String),
    /// The call it's waiting on.
    Hole,
}

/// Forms that aren't waiting to use a value the way a function application is: a call's
/// result there is the answer itself (`cond`, `if`, a body), so they end the context.
const NOT_APPLICATIONS: &[&str] = &[
    "define", "define-struct", "define-values", "cond", "if", "else", "and", "or", "when", "unless", "local", "let", "let*", "letrec", "lambda", "λ", "begin", "begin0", "set!",
    "case", "match", "quote", "quasiquote", "check-expect", "check-within", "check-error", "check-random", "check-satisfied", "check-member-of", "check-range", "time",
];

/// What a call on the stack will do once the call it's waiting on returns: the function
/// applications around that call, with what's already worked out shown as values and the
/// call itself as a hole: `(bst-insert 37 •••)`, `(* 3 •••)`, `(make-node 34 ••• (node-right t))`.
/// `k` is the frame (an index into the step's frames), or `None` for the top level. `None`
/// when there's nothing left to do (the call's answer is the frame's answer).
pub fn waiting(trace: &Trace, step: &VizStep, k: Option<usize>) -> Option<Vec<Seg>> {
    let callee = step.frames.get(k.map_or(0, |k| k + 1))?;
    let site = callee.pos?;
    let site = (site, site + callee.span);
    let (done, vars): (&[crate::racket::protocol::VizEval], &[(String, VizRef)]) = match k {
        Some(k) => (&step.frames[k].done, &step.frames[k].vars),
        None => (&step.top_done, &[]),
    };
    let path = sexp::path_to(&trace.forms, site.0, site.1)?;
    let application = |n: &Node| trace.source.get(n.start) == Some(&'(') && n.head().is_some_and(|h| !NOT_APPLICATIONS.contains(&h));
    let mut top = path.len() - 1;
    while top > 0 && application(path[top - 1]) {
        top -= 1;
    }
    if top == path.len() - 1 {
        return None;
    }
    let mut out = Vec::new();
    seg(trace, step.index, path[top], site, done, vars, false, &mut out);
    // Adjacent code reads as one piece.
    let mut merged: Vec<Seg> = Vec::new();
    for s in out {
        match (merged.last_mut(), s) {
            (Some(Seg::Code(a)), Seg::Code(b)) => a.push_str(&b),
            (_, s) => merged.push(s),
        }
    }
    Some(merged)
}

/// `n` as segments: left of the hole everything has been worked out (values); the hole; and
/// to its right, code that hasn't run yet.
#[allow(clippy::too_many_arguments)]
fn seg(trace: &Trace, step: usize, n: &Node, site: (usize, usize), done: &[crate::racket::protocol::VizEval], vars: &[(String, VizRef)], head: bool, out: &mut Vec<Seg>) {
    if (n.start, n.end) == site {
        out.push(Seg::Hole);
        return;
    }
    let before = n.end <= site.0;
    if before && !head {
        let value = done.iter().rev().find(|d| d.pos == Some(n.start) && d.span == n.end - n.start).and_then(|d| d.value.as_ref());
        let value = value.or_else(|| n.atom().and_then(|a| vars.iter().rev().find(|(v, _)| v == a)).map(|(_, v)| v));
        if let Some(v) = value {
            out.push(Seg::Value(label(trace, v, step)));
            return;
        }
    }
    match n.atom() {
        Some(a) => out.push(Seg::Code(a.to_string())),
        None => {
            let open = trace.source.get(n.start).copied().unwrap_or('(');
            let close = match open {
                '[' => ']',
                '{' => '}',
                _ => ')',
            };
            out.push(Seg::Code(open.to_string()));
            for (i, item) in n.items().iter().enumerate() {
                if i > 0 {
                    out.push(Seg::Code(" ".into()));
                }
                seg(trace, step, item, site, done, vars, i == 0, out);
            }
            out.push(Seg::Code(close.to_string()));
        }
    }
}

/// Segments as one line of text, the hole as `•••`.
pub fn seg_text(segs: &[Seg]) -> String {
    segs.iter()
        .map(|s| match s {
            Seg::Code(t) | Seg::Value(t) => t.as_str(),
            Seg::Hole => "•••",
        })
        .collect()
}

/// Pieces as plain text.
pub fn plain(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) | Piece::Name(t) | Piece::Code(t) | Piece::Work(t) | Piece::Value(t) => t.clone(),
            Piece::Obj { label, .. } => label.clone(),
            Piece::Answer(yes) => if *yes { "Yes".into() } else { "No".into() },
        })
        .collect()
}

fn text(s: impl Into<String>) -> Piece {
    Piece::Text(s.into())
}

/// One-line program text (whitespace collapsed).
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Everything wording needs about one step: the program text, the values the step knows
/// (operations by source location, variables by name), and the program's structs.
struct Ctx<'a> {
    trace: &'a Trace,
    step: &'a VizStep,
    code: &'a dyn Fn(usize, usize) -> String,
    /// Operation results since the previous step, by (position, span).
    evals: HashMap<(usize, usize), &'a VizRef>,
    /// The running call's variables, then the globals.
    vars: HashMap<String, VizRef>,
    /// `node-key` -> ("node", "key").
    accessors: HashMap<String, (String, String)>,
}

impl<'a> Ctx<'a> {
    fn new(trace: &'a Trace, step: &'a VizStep, code: &'a dyn Fn(usize, usize) -> String) -> Self {
        let evals = step.evals.iter().filter_map(|e| Some(((e.pos?, e.span), e.value.as_ref()?))).collect();
        let mut vars: HashMap<String, VizRef> = step.globals.iter().cloned().collect();
        if let Some(f) = step.live_frames().last() {
            vars.extend(f.vars.iter().cloned());
        }
        let mut accessors = HashMap::new();
        for (s, fields) in &trace.shapes {
            for f in fields {
                accessors.insert(format!("{s}-{f}"), (s.clone(), f.clone()));
            }
        }
        Ctx { trace, step, code, evals, vars, accessors }
    }

    /// Read the program text at `pos`; node positions are offsets from `pos`.
    fn read(&self, pos: usize, span: usize) -> Option<Node> {
        sexp::read(&(self.code)(pos, span))
    }

    fn value(&self, r: &VizRef) -> Piece {
        match r {
            VizRef::Prim { v, k } => Piece::Value(super::prim_display(v, k).to_string()),
            VizRef::Object { id } => Piece::Obj { id: *id, label: label(self.trace, r, self.step.index) },
        }
    }

    /// The value of a piece of code, when the step knows it: an operation it recorded, or a
    /// variable. `base` is the source position the node's offsets count from.
    fn value_of(&self, n: &Node, base: usize) -> Option<VizRef> {
        if let Some(r) = self.evals.get(&(base + n.start, n.end - n.start)) {
            return Some((*r).clone());
        }
        n.atom().and_then(|a| self.vars.get(a).cloned())
    }

    fn struct_of_predicate(&self, head: &str) -> Option<String> {
        let s = head.strip_suffix('?')?;
        self.trace.shapes.contains_key(s).then(|| s.to_string())
    }

    fn struct_of_constructor(&self, head: &str) -> Option<String> {
        let s = head.strip_prefix("make-")?;
        self.trace.shapes.contains_key(s).then(|| s.to_string())
    }
}

/// A one-argument predicate as an adjective phrase: `empty?` -> "empty".
fn predicate_phrase(ctx: &Ctx, head: &str) -> Option<String> {
    if let Some(s) = ctx.struct_of_predicate(head) {
        return Some(format!("a {s}"));
    }
    Some(
        match head {
            "empty?" | "null?" => "empty",
            "cons?" | "pair?" => "a non-empty list",
            "zero?" => "zero",
            "positive?" => "positive",
            "negative?" => "negative",
            "even?" => "even",
            "odd?" => "odd",
            "number?" => "a number",
            "string?" => "a string",
            "symbol?" => "a symbol",
            "boolean?" => "a boolean",
            "list?" => "a list",
            "image?" => "an image",
            "integer?" => "an integer",
            "false?" => "false",
            _ => return None,
        }
        .to_string(),
    )
}

/// A comparison as words: `<` -> "less than".
fn comparison(head: &str) -> Option<&'static str> {
    Some(match head {
        "<" => "less than",
        ">" => "greater than",
        "<=" => "at most",
        ">=" => "at least",
        "=" | "equal?" | "string=?" | "symbol=?" | "eq?" | "eqv?" | "char=?" => "equal to",
        "string<?" => "alphabetically before",
        "string>?" => "alphabetically after",
        _ => return None,
    })
}

/// Code in words, naming things rather than evaluating them: "t's key", "n - 1".
fn words(ctx: &Ctx, n: &Node) -> Vec<Piece> {
    if let Some(a) = n.atom() {
        return vec![match a {
            "empty" | "'()" | "null" => Piece::Value("empty".into()),
            "#true" | "#t" | "true" => Piece::Value(if ctx.trace.as_ids { "true" } else { "#true" }.into()),
            "#false" | "#f" | "false" => Piece::Value(if ctx.trace.as_ids { "false" } else { "#false" }.into()),
            _ if a.starts_with(|c: char| c.is_ascii_digit() || c == '"' || c == '\'' || c == '#') || a.parse::<f64>().is_ok() => Piece::Value(a.to_string()),
            _ => Piece::Name(a.to_string()),
        }];
    }
    let head = n.head().unwrap_or("");
    let args = n.args();
    let arg = |k: usize| args.get(k).map(|a| words(ctx, a)).unwrap_or_default();
    let mut out: Vec<Piece> = Vec::new();
    if let Some((_, field)) = ctx.accessors.get(head)
        && args.len() == 1
    {
        out.extend(arg(0));
        out.push(text(format!("'s {field}")));
        return out;
    }
    if let Some(s) = ctx.struct_of_constructor(head) {
        return vec![text(format!("a new {s}"))];
    }
    if args.len() == 1
        && let Some(p) = predicate_phrase(ctx, head)
    {
        out.extend(arg(0));
        out.push(text(format!(" is {p}")));
        return out;
    }
    let binary = |op: &str| -> Vec<Piece> {
        let mut v = Vec::new();
        for (k, _) in args.iter().enumerate() {
            if k > 0 {
                v.push(text(format!(" {op} ")));
            }
            v.extend(arg(k));
        }
        v
    };
    match (head, args.len()) {
        ("<" | ">" | "<=" | ">=" | "=" | "+" | "-" | "*" | "/", n) if n >= 2 => return binary(head),
        ("-", 1) => {
            out.push(text("minus "));
            out.extend(arg(0));
            return out;
        }
        ("add1", 1) => {
            out.extend(arg(0));
            out.push(text(" + 1"));
            return out;
        }
        ("sub1", 1) => {
            out.extend(arg(0));
            out.push(text(" - 1"));
            return out;
        }
        ("first" | "car", 1) => {
            out.push(text("the first item of "));
            out.extend(arg(0));
            return out;
        }
        ("rest" | "cdr", 1) => {
            out.push(text("the rest of "));
            out.extend(arg(0));
            return out;
        }
        ("length", 1) => {
            out.push(text("the length of "));
            out.extend(arg(0));
            return out;
        }
        ("random", 1) => {
            out.push(text("a random number below "));
            out.extend(arg(0));
            return out;
        }
        ("not", 1) => {
            out.push(text("not ("));
            out.extend(arg(0));
            out.push(text(")"));
            return out;
        }
        ("cons", 2) => {
            out.extend(arg(0));
            out.push(text(" in front of "));
            out.extend(arg(1));
            return out;
        }
        _ => {}
    }
    if let Some(c) = comparison(head)
        && args.len() == 2
    {
        out.extend(arg(0));
        out.push(text(format!(" {c} ")));
        out.extend(arg(1));
        return out;
    }
    vec![Piece::Code(node_text(n))]
}

/// A node written back as text (atoms as they are, lists re-joined).
fn node_text(n: &Node) -> String {
    match n.atom() {
        Some(a) => a.to_string(),
        None => format!("({})", n.items().iter().map(node_text).collect::<Vec<_>>().join(" ")),
    }
}

/// A question as a question: "Is t empty?", "Is k less than t's key?".
fn question(ctx: &Ctx, n: &Node) -> Vec<Piece> {
    let head = n.head().unwrap_or("");
    let args = n.args();
    let mut out = vec![text("Is ")];
    if args.len() == 1
        && let Some(p) = predicate_phrase(ctx, head)
    {
        out.extend(words(ctx, &args[0]));
        out.push(text(format!(" {p}?")));
        return out;
    }
    if args.len() == 2
        && let Some(c) = comparison(head)
    {
        out.extend(words(ctx, &args[0]));
        out.push(text(format!(" {c} ")));
        out.extend(words(ctx, &args[1]));
        out.push(text("?"));
        return out;
    }
    out.extend(words(ctx, n));
    out.push(text(" true?"));
    out
}

/// "k is 12 and t's key is 34": the values a piece of code used, skipping literals and
/// anything whose value the step doesn't know.
fn values_used(ctx: &Ctx, n: &Node, base: usize) -> Vec<Piece> {
    let parts: Vec<&Node> = if n.atom().is_some() { vec![n] } else { n.args().iter().collect() };
    let mut clauses: Vec<Vec<Piece>> = Vec::new();
    for p in parts {
        let literal = matches!(words(ctx, p).as_slice(), [Piece::Value(_)]);
        if literal {
            continue;
        }
        if let Some(v) = ctx.value_of(p, base) {
            let mut c = words(ctx, p);
            c.push(text(" is "));
            c.push(ctx.value(&v));
            clauses.push(c);
        }
    }
    join_clauses(clauses)
}

fn join_clauses(clauses: Vec<Vec<Piece>>) -> Vec<Piece> {
    let n = clauses.len();
    let mut out = Vec::new();
    for (k, c) in clauses.into_iter().enumerate() {
        if k > 0 {
            out.push(text(if k + 1 == n { " and " } else { ", " }));
        }
        out.extend(c);
    }
    out
}

/// What the step's operations worked out, one per line: "t's key is 34". Skips the step's
/// own code, constructors (described where they matter) and `random` (shown up front).
fn work_lines(ctx: &Ctx, skip: Option<(usize, usize)>) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    for e in &ctx.step.evals {
        let (Some(pos), Some(v)) = (e.pos, &e.value) else { continue };
        if Some((pos, e.span)) == skip {
            continue;
        }
        let Some(n) = ctx.read(pos, e.span) else { continue };
        let head = n.head().unwrap_or("");
        if ctx.struct_of_constructor(head).is_some() || head == "random" {
            continue;
        }
        let mut line = words(ctx, &n);
        if matches!(line.as_slice(), [Piece::Code(_)]) && n.atom().is_none() {
            // Not something we can say in words: the operation with its values.
            let args: Vec<String> = e.args.iter().map(|a| label(ctx.trace, a, ctx.step.index)).collect();
            line = vec![Piece::Code(format!("({head} {})", args.join(" ")))];
        }
        line.push(text(" is "));
        line.push(ctx.value(v));
        out.push(line);
    }
    out
}

/// `(random 50)` picked 12: randomness is worth saying up front.
fn random_lines(ctx: &Ctx) -> Vec<Vec<Piece>> {
    ctx.step
        .evals
        .iter()
        .filter_map(|e| {
            let n = ctx.read(e.pos?, e.span)?;
            let v = e.value.as_ref()?;
            (n.head() == Some("random")).then(|| vec![Piece::Work(one_line(&node_text(&n))), text(" picked "), ctx.value(v)])
        })
        .collect()
}

/// "It builds a new node with key 34, left node 12 and right empty", for constructors whose
/// result is `value` (or all of them, when `value` is None).
fn built_lines(ctx: &Ctx, value: Option<&VizRef>) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    for e in &ctx.step.evals {
        let Some(pos) = e.pos else { continue };
        let Some(n) = ctx.read(pos, e.span) else { continue };
        let Some(s) = n.head().and_then(|h| ctx.struct_of_constructor(h)) else { continue };
        if value.is_some() && e.value.as_ref() != value {
            continue;
        }
        let fields = ctx.trace.shapes.get(&s).cloned().unwrap_or_default();
        let mut line = vec![text(format!("It builds a new {s} with "))];
        let clauses: Vec<Vec<Piece>> = fields.iter().zip(&e.args).map(|(f, a)| vec![Piece::Name(f.clone()), text(" "), ctx.value(a)]).collect();
        line.extend(join_clauses(clauses));
        out.push(line);
    }
    out
}

fn frame_name(f: &VizFrame) -> String {
    f.name.clone()
}

/// Explain a step for a student. `code` reads source text by (position, length).
pub fn explain(view: &VisualizerView, step: &VizStep, code: &dyn Fn(usize, usize) -> String) -> Explanation {
    let trace = &view.trace;
    let ctx = Ctx::new(trace, step, code);
    let name = step.name.clone().unwrap_or_else(|| "λ".into());
    let live: Vec<&VizFrame> = step.live_frames().collect();
    let own = step.pos.map(|p| (p, step.span));
    let mut ex = Explanation { code: step.pos.map(|p| one_line(&code(p, step.span))).unwrap_or_default(), ..Default::default() };
    match step.kind.as_str() {
        "call" => {
            let callee = live.last().copied();
            let caller = live.len().checked_sub(2).and_then(|k| live.get(k)).copied();
            // Who made the call: the call it replaced (a tail call), else the one under it.
            let pos = step.frames.iter().rposition(|f| !f.replaced);
            let replaced = pos.and_then(|k| k.checked_sub(1)).and_then(|k| step.frames.get(k)).filter(|f| f.replaced);
            let tail = replaced.is_some();
            let from = replaced.or(caller).map(frame_name);
            let recursive = from.as_deref() == Some(name.as_str());
            ex.headline = match (&from, recursive) {
                (Some(_), true) => vec![Piece::Name(name.clone()), text(if tail { " calls itself again (a tail call)" } else { " calls itself" })],
                (Some(c), false) => vec![Piece::Name(c.clone()), text(" calls "), Piece::Name(name.clone())],
                (None, _) => vec![text("The program calls "), Piece::Name(name.clone())],
            };
            ex.lines.extend(random_lines(&ctx));
            // Each argument: its value, and where it came from when that isn't obvious.
            if let Some(f) = callee {
                let site = f.pos.and_then(|p| ctx.read(p, f.span).map(|n| (p, n)));
                let n = f.nargs.min(f.vars.len());
                for (k, (param, v)) in f.vars[..n].iter().enumerate() {
                    let mut line = vec![Piece::Name(param.clone()), text(" is "), ctx.value(v)];
                    if let Some((_, call)) = &site
                        && let Some(a) = call.args().get(k)
                    {
                        let said = words(&ctx, a);
                        let trivial = matches!(said.as_slice(), [Piece::Value(_)]) || a.atom() == Some(param.as_str()) || matches!(said.as_slice(), [Piece::Code(_)]);
                        if !trivial {
                            // In a recursive call the names are the previous call's.
                            line.push(text(if recursive { ", which was " } else { ", which is " }));
                            line.extend(said);
                            if recursive {
                                line.push(text(" in the previous call"));
                            }
                        }
                    }
                    ex.lines.push(line);
                }
            }
            // What the caller will do with the answer, when there's more to do than pass it on.
            if !tail
                && let Some(c) = caller
                && let Some(k) = step.frames.iter().position(|f| std::ptr::eq(f, c))
                && let Some(w) = waiting(trace, step, Some(k))
            {
                ex.lines.push(vec![text("When it returns, "), Piece::Name(c.name.clone()), text(" goes on with "), Piece::Code(seg_text(&w))]);
            } else if !tail
                && caller.is_none()
                && let Some(w) = waiting(trace, step, None)
            {
                ex.lines.push(vec![text("When it returns, the top level goes on with "), Piece::Code(seg_text(&w))]);
            }
            if tail && !recursive {
                ex.lines.push(vec![text("A tail call: it takes over from "), Piece::Name(from.clone().unwrap_or_default()), text(", which would only pass its result on")]);
            } else if tail {
                ex.lines.push(vec![text("A tail call: the new call takes the old one's place, since the old one would only pass its result on")]);
            }
            ex.work = work_lines(&ctx, None);
        }
        "question" => {
            let answer = step.value.as_ref().and_then(|v| match v {
                VizRef::Prim { v, .. } => Some(!super::is_false(v)),
                VizRef::Object { .. } => None,
            });
            let q = step.pos.and_then(|p| ctx.read(p, step.span).map(|n| (p, n)));
            ex.headline = match &q {
                Some((_, n)) => question(&ctx, n),
                None => vec![text("A question")],
            };
            if let Some(a) = answer {
                ex.headline.push(text("  "));
                ex.headline.push(Piece::Answer(a));
            }
            if let Some((p, n)) = &q {
                let used = values_used(&ctx, n, *p);
                if !used.is_empty() {
                    let mut line = vec![text("Here ")];
                    line.extend(used);
                    ex.lines.push(line);
                }
            }
            // What follows from the answer depends on the form that asked.
            let before = step.pos.map(|p| code(p.saturating_sub(12), p.min(12))).unwrap_or_default();
            let before = before.trim_end();
            let then = match (answer, before) {
                (Some(true), b) if b.ends_with("(if") => "so if takes its then-branch",
                (Some(false), b) if b.ends_with("(if") => "so if takes its else-branch",
                (Some(true), b) if b.ends_with('[') || b.ends_with('(') => "so cond uses this clause's answer",
                (Some(false), b) if b.ends_with('[') || b.ends_with('(') => "so cond moves on to the next clause",
                _ => "",
            };
            if !then.is_empty() {
                let mut s = then.to_string();
                s[..1].make_ascii_uppercase();
                ex.lines.push(vec![text(s)]);
            }
            ex.work = work_lines(&ctx, own);
        }
        "return" => {
            let value = step.value.as_ref();
            ex.headline = vec![Piece::Name(name.clone()), text(" returns ")];
            ex.headline.push(value.map(|v| ctx.value(v)).unwrap_or_else(|| text("nothing")));
            ex.lines.extend(random_lines(&ctx));
            ex.lines.extend(built_lines(&ctx, value));
            let back_to = live.len().checked_sub(2).and_then(|k| live.get(k)).copied();
            // Where the answer goes: into the hole of what the caller was waiting to do.
            let k = back_to.and_then(|b| step.frames.iter().position(|f| std::ptr::eq(f, b)));
            let then = if back_to.is_some() { k.and_then(|k| waiting(trace, step, Some(k))) } else { waiting(trace, step, None) };
            let mut line = match back_to {
                Some(b) => vec![text("Back to "), Piece::Name(b.name.clone()), text(", which was waiting for this result")],
                None => vec![text("Back to the top level")],
            };
            if let Some(w) = then {
                line.push(text(if back_to.is_some() { " to finish " } else { ", to finish " }));
                line.push(Piece::Code(seg_text(&w)));
            }
            ex.lines.push(line);
            ex.work = work_lines(&ctx, own);
        }
        "define" => {
            let names: Vec<&str> = name.split(", ").collect();
            let is_fn = |n: &str| step.globals.iter().any(|(g, v)| g == n && matches!(v, VizRef::Prim { k, .. } if k == "fn"));
            if names.iter().all(|n| is_fn(n)) {
                ex.headline = vec![text(if names.len() == 1 { "Defining the function " } else { "Defining the functions " })];
                for (k, n) in names.iter().enumerate() {
                    if k > 0 {
                        ex.headline.push(text(if k + 1 == names.len() { " and " } else { ", " }));
                    }
                    ex.headline.push(Piece::Name(n.to_string()));
                }
                ex.lines.push(vec![text("A function's body runs each time it's called")]);
            } else {
                ex.headline = vec![text("Defining "), Piece::Name(name.clone())];
                for (n, v) in step.globals.iter().filter(|(n, _)| names.contains(&n.as_str())) {
                    ex.lines.push(vec![Piece::Name(n.clone()), text(" is "), ctx.value(v)]);
                }
                ex.lines.extend(built_lines(&ctx, None));
            }
            ex.work = work_lines(&ctx, None);
        }
        "set" => {
            ex.headline = vec![Piece::Name(name.clone()), text(" changes")];
            ex.lines.push(vec![Piece::Name(name.clone()), text(" is now "), step.value.as_ref().map(|v| ctx.value(v)).unwrap_or_else(|| text("nothing"))]);
            ex.work = work_lines(&ctx, None);
        }
        "result" => {
            ex.headline = vec![text("The program's result is ")];
            ex.headline.push(step.value.as_ref().map(|v| ctx.value(v)).unwrap_or_else(|| text("nothing")));
            ex.lines.push(vec![text("It's what the Interactions panel shows")]);
        }
        "end" => {
            if step.failed == Some(true) {
                ex.headline = vec![text("The program stopped with an error")];
                ex.lines.push(vec![text("See the Interactions panel for the message")]);
            } else {
                ex.headline = vec![text("The program finished")];
                if let Some(r) = trace.result_at(step.index) {
                    ex.lines.push(vec![text("Its result is "), ctx.value(r)]);
                }
            }
            ex.work = work_lines(&ctx, None);
        }
        k => ex.headline = vec![text(k)],
    }
    ex
}

/// A value written the way the language would show it, shortened to about `max` characters:
/// `(list 3 2 1)`, `(make-node '() 5 (make-node …))`, `"hi"`.
#[cfg(test)]
pub fn show_value(view: &VisualizerView, r: &VizRef, step: usize, max: usize) -> String {
    show_in(&view.trace, r, step, max)
}

/// [`show_value`] against a trace.
pub fn show_in(trace: &Trace, r: &VizRef, step: usize, max: usize) -> String {
    fn go(trace: &Trace, r: &VizRef, step: usize, depth: usize, out: &mut String, max: usize) {
        if out.chars().count() > max {
            return;
        }
        let id = match r {
            VizRef::Prim { v, k } => {
                out.push_str(super::prim_display(v, k));
                return;
            }
            VizRef::Object { id } => *id,
        };
        let Some(o) = trace.object(id, step) else {
            out.push('…');
            return;
        };
        if depth > 3 {
            out.push('…');
            return;
        }
        match o.k.as_str() {
            "cons" => {
                // A proper list prints as (list …); anything else as nested cons.
                let mut items = Vec::new();
                let mut cur = Some(o);
                let mut proper = false;
                while let Some(c) = cur {
                    if c.k != "cons" || items.len() > 50 {
                        break;
                    }
                    items.push(c.first.clone());
                    match &c.rest {
                        Some(VizRef::Object { id }) => cur = trace.object(*id, step),
                        Some(VizRef::Prim { k, .. }) if k == "empty" => {
                            proper = true;
                            cur = None;
                        }
                        _ => cur = None,
                    }
                }
                if !proper {
                    out.push_str("(cons ");
                    if let Some(f) = &o.first {
                        go(trace, f, step, depth + 1, out, max);
                    }
                    out.push(' ');
                    if let Some(rest) = &o.rest {
                        go(trace, rest, step, depth + 1, out, max);
                    }
                    out.push(')');
                    return;
                }
                out.push_str("(list");
                for item in items.iter().flatten() {
                    if out.chars().count() > max {
                        out.push_str(" …");
                        break;
                    }
                    out.push(' ');
                    go(trace, item, step, depth + 1, out, max);
                }
                out.push(')');
            }
            "struct" => {
                out.push_str(&format!("(make-{}", o.name.as_deref().unwrap_or("struct")));
                for (_, f) in &o.fields {
                    if out.chars().count() > max {
                        out.push_str(" …");
                        break;
                    }
                    out.push(' ');
                    go(trace, f, step, depth + 1, out, max);
                }
                out.push(')');
            }
            "vector" => {
                out.push_str("(vector");
                for f in &o.items {
                    out.push(' ');
                    go(trace, f, step, depth + 1, out, max);
                }
                out.push(')');
            }
            "box" => {
                out.push_str("(box ");
                if let Some(f) = o.items.first() {
                    go(trace, f, step, depth + 1, out, max);
                }
                out.push(')');
            }
            "image" => out.push_str("an image"),
            "hash" => out.push_str(&format!("a hash table of {} entries", o.pairs.len())),
            _ => out.push_str(o.text.as_deref().unwrap_or("…")),
        }
    }
    let mut out = String::new();
    go(trace, r, step, 0, &mut out, max);
    if out.chars().count() > max + 12 {
        let cut: String = out.chars().take(max).collect();
        format!("{cut}…")
    } else {
        out
    }
}

/// Everything the program printed up to and including `step`.
pub fn output_until(view: &VisualizerView, step: usize) -> String {
    view.trace.steps.iter().take(step + 1).map(|s| s.output.as_str()).collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::racket::protocol::{VizEval, VizObject};

    fn prim(v: &str, k: &str) -> VizRef {
        VizRef::Prim { v: v.into(), k: k.into() }
    }
    fn obj(id: u64) -> VizRef {
        VizRef::Object { id }
    }
    fn frame(name: &str, vars: Vec<(&str, VizRef)>, nargs: usize, pos: Option<usize>, span: usize) -> VizFrame {
        VizFrame { name: name.into(), nargs, vars: vars.into_iter().map(|(n, v)| (n.to_string(), v)).collect(), replaced: false, pos, span, done: vec![] }
    }
    fn node(id: u64, key: &str, left: VizRef, right: VizRef) -> VizObject {
        VizObject { id, k: "struct".into(), name: Some("node".into()), fields: vec![("key".into(), prim(key, "num")), ("left".into(), left), ("right".into(), right)], ..Default::default() }
    }
    fn lines(e: &Explanation) -> Vec<String> {
        e.lines.iter().map(|l| plain(l)).collect()
    }
    /// The source text of `program`, read by (position, length) like the editor buffer.
    fn reader(program: &'static str) -> impl Fn(usize, usize) -> String {
        move |p: usize, n: usize| program.chars().skip(p).take(n).collect::<String>()
    }

    /// Work done on the way is marked wherever it is, but values are noted only where the
    /// step's own note doesn't already give them: outside its code, and not inside other work.
    #[test]
    fn work_on_the_way_is_marked_and_noted_once() {
        let program = "(bst-insert (random 100) (big-tree (sub1 n)))\n(< k (node-key t))\n(+ (* 2 x) 1)";
        let code = reader(program);
        let at = |s: &str| program.find(s).unwrap();
        let ev = |s: &str, v: &str| VizEval { pos: Some(at(s)), span: s.len(), args: vec![], value: Some(prim(v, "num")) };
        let trace = Trace::default();
        let notes = |step: &VizStep| work_done(&trace, step, &code).into_iter().map(|w| (code(w.range.start, w.range.end - w.range.start), w.note)).collect::<Vec<_>>();
        // A call: `random` ran beside it, `sub1` inside it (its value is the call's argument).
        let call = VizStep { kind: "call".into(), pos: Some(at("(big-tree")), span: "(big-tree (sub1 n))".len(), evals: vec![ev("(random 100)", "37"), ev("(sub1 n)", "2")], ..Default::default() };
        assert_eq!(notes(&call), vec![("(random 100)".into(), Some("(random 100) → 37".into())), ("(sub1 n)".into(), None)]);
        // A question is itself an operation: it isn't marked twice.
        let question = VizStep { kind: "question".into(), pos: Some(at("(< k")), span: "(< k (node-key t))".len(), evals: vec![ev("(node-key t)", "34"), ev("(< k (node-key t))", "1")], ..Default::default() };
        assert_eq!(notes(&question), vec![("(node-key t)".into(), None)]);
        // The program's result has no code of its own: nested work is marked, the outer
        // value noted; code that ran twice is marked once, with its last value.
        let result = VizStep { kind: "result".into(), evals: vec![ev("(* 2 x)", "4"), ev("(* 2 x)", "6"), ev("(+ (* 2 x) 1)", "7")], ..Default::default() };
        assert_eq!(notes(&result), vec![("(* 2 x)".into(), None), ("(+ (* 2 x) 1)".into(), Some("(+ (* 2 x) 1) → 7".into()))]);
    }

    /// The user's BST insert, at its comparison: in words, with the values that decide it.
    #[test]
    fn questions_are_asked_in_words_with_their_values() {
        let program = "(cond [(empty? t) 0] [(< k (node-key t)) 1])";
        let code = reader(program);
        let q = program.find("(< k").unwrap();
        let key = program.find("(node-key t)").unwrap();
        let mut view = VisualizerView::default();
        view.trace.push(VizStep {
            kind: "question".into(),
            pos: Some(q),
            span: "(< k (node-key t))".len(),
            value: Some(prim("#true", "bool")),
            frames: vec![frame("bst-insert", vec![("k", prim("12", "num")), ("t", obj(1))], 2, None, 0)],
            evals: vec![VizEval { pos: Some(key), span: 12, args: vec![obj(1)], value: Some(prim("34", "num")) }],
            objects: vec![node(1, "34", prim("'()", "empty"), prim("'()", "empty"))],
            ..Default::default()
        });
        let e = explain(&view, &view.trace.steps[0], &code);
        assert_eq!(plain(&e.headline), "Is k less than t's key?  Yes");
        assert_eq!(lines(&e), vec!["Here k is 12 and t's key is 34", "So cond uses this clause's answer"]);
        // The first question, about emptiness.
        let mut view = VisualizerView::default();
        view.trace.push(VizStep {
            kind: "question".into(),
            pos: Some(program.find("(empty?").unwrap()),
            span: 11,
            value: Some(prim("#false", "bool")),
            frames: vec![frame("bst-insert", vec![("k", prim("12", "num")), ("t", obj(1))], 2, None, 0)],
            objects: vec![node(1, "34", prim("'()", "empty"), prim("'()", "empty"))],
            ..Default::default()
        });
        let e = explain(&view, &view.trace.steps[0], &code);
        assert_eq!(plain(&e.headline), "Is t empty?  No");
        assert_eq!(lines(&e), vec!["Here t is node 34", "So cond moves on to the next clause"]);
        assert_eq!(e.objects(), vec![1]);
    }

    /// A recursive call names its arguments by what they were in the previous call, and a
    /// return that builds a node says what it built (the copied path).
    #[test]
    fn calls_and_returns_say_where_values_come_from() {
        let program = "(bst-insert k (node-left t)) (make-node (node-key t) (bst-insert k (node-left t)) (node-right t))";
        let code = reader(program);
        let site = 0;
        let mut view = VisualizerView::default();
        let outer = frame("bst-insert", vec![("k", prim("12", "num")), ("t", obj(2))], 2, None, 0);
        let inner = frame("bst-insert", vec![("k", prim("12", "num")), ("t", obj(1))], 2, Some(site), 28);
        let tree = vec![node(1, "18", prim("'()", "empty"), prim("'()", "empty")), node(2, "34", obj(1), prim("'()", "empty"))];
        view.trace.push(VizStep { kind: "call".into(), name: Some("bst-insert".into()), frames: vec![outer.clone()], objects: tree, ..Default::default() });
        view.trace.push(VizStep { kind: "call".into(), name: Some("bst-insert".into()), frames: vec![outer.clone(), inner.clone()], ..Default::default() });
        let e = explain(&view, &view.trace.steps[1], &code);
        assert_eq!(plain(&e.headline), "bst-insert calls itself");
        assert_eq!(lines(&e), vec!["k is 12", "t is node 18, which was t's left in the previous call"]);

        // The outer call returns the new node 34 it built around the new left subtree.
        let make = program.find("(make-node").unwrap();
        view.trace.push(VizStep {
            kind: "return".into(),
            name: Some("bst-insert".into()),
            frames: vec![outer],
            value: Some(obj(4)),
            evals: vec![VizEval { pos: Some(make), span: program.len() - make, args: vec![prim("34", "num"), obj(3), prim("'()", "empty")], value: Some(obj(4)) }],
            objects: vec![node(3, "18", obj(5), prim("'()", "empty")), node(4, "34", obj(3), prim("'()", "empty"))],
            ..Default::default()
        });
        let e = explain(&view, &view.trace.steps[2], &code);
        assert_eq!(plain(&e.headline), "bst-insert returns node 34");
        assert_eq!(lines(&e), vec!["It builds a new node with key 34, left node 18 and right empty", "Back to the top level"]);
    }

    #[test]
    fn code_reads_as_words() {
        let mut view = VisualizerView::default();
        view.trace.push(VizStep { objects: vec![node(1, "1", prim("'()", "empty"), prim("'()", "empty"))], ..Default::default() });
        let code = |_: usize, _: usize| String::new();
        let ctx = Ctx::new(&view.trace, &view.trace.steps[0], &code);
        let say = |s: &str| plain(&words(&ctx, &sexp::read(s).unwrap()));
        assert_eq!(say("(node-key (node-left t))"), "t's left's key");
        assert_eq!(say("(sub1 n)"), "n - 1");
        assert_eq!(say("(first (rest lst))"), "the first item of the rest of lst");
        assert_eq!(say("(node? x)"), "x is a node");
        assert_eq!(say("(make-node 5 empty empty)"), "a new node");
        assert_eq!(say("(+ a b 1)"), "a + b + 1");
        assert_eq!(say("(foo x)"), "(foo x)");
    }
}
