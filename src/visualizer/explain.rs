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

/// Pieces as plain text.
#[cfg(test)]
pub fn plain(pieces: &[Piece]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) | Piece::Name(t) | Piece::Code(t) | Piece::Value(t) => t.clone(),
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
            "#true" | "#t" | "true" => Piece::Value("#true".into()),
            "#false" | "#f" | "false" => Piece::Value("#false".into()),
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
            (n.head() == Some("random")).then(|| vec![Piece::Code(one_line(&node_text(&n))), text(" picked "), ctx.value(v)])
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
            if tail && !recursive {
                ex.lines.push(vec![text("A tail call: it takes over from "), Piece::Name(from.clone().unwrap_or_default()), text(", which would only pass its result on")]);
            } else if tail {
                ex.lines.push(vec![text("A tail call: the new call takes the old one's place, since the old one would only pass its result on")]);
            }
            ex.work = work_lines(&ctx, None);
        }
        "question" => {
            let answer = step.value.as_ref().and_then(|v| match v {
                VizRef::Prim { v, .. } => Some(v == "#true"),
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
            ex.lines.push(match back_to {
                Some(b) => vec![text("Back to "), Piece::Name(b.name.clone()), text(", which was waiting for this result")],
                None => vec![text("Back to the top level")],
            });
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
        VizFrame { name: name.into(), nargs, vars: vars.into_iter().map(|(n, v)| (n.to_string(), v)).collect(), replaced: false, pos, span }
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
