//! Explanations: each step of a run in plain language.

use super::{Trace, VisualizerView};
use crate::racket::protocol::{VizRef, VizStep};


/// A piece of an explanation line.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// Program text, shown in a code chip.
    Code(String),
    /// A value, shown in a value chip.
    Value(String),
}

/// A step in plain language: the code it's about, a headline, and lines with the values.
#[derive(Clone, Debug, Default)]
pub struct Explanation {
    pub code: String,
    pub headline: String,
    pub lines: Vec<Vec<Piece>>,
}

/// A value written the way the language would show it, shortened to about `max` characters:
/// `(list 3 2 1)`, `(make-node '() 5 (make-node …))`, `"hi"`.
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
            VizRef::Prim { v, .. } => {
                out.push_str(v);
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

/// One-line program text (whitespace collapsed).
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `(name arg …)` for a frame, with its argument values.
fn call_text(trace: &Trace, f: &crate::racket::protocol::VizFrame, step: usize) -> String {
    let n = f.nargs.min(f.vars.len());
    let args: Vec<String> = f.vars[..n].iter().map(|(_, v)| show_in(trace, v, step, 16)).collect();
    if args.is_empty() { format!("({})", f.name) } else { format!("({} {})", f.name, args.join(" ")) }
}

/// The operator of a call's source text: `(sub1 n)` -> `sub1`.
fn head_of(text: &str) -> Option<String> {
    let t = text.trim_start().strip_prefix(['(', '['])?;
    let end = t.find(|c: char| c.is_whitespace() || c == ')' || c == ']').unwrap_or(t.len());
    (end > 0).then(|| t[..end].to_string())
}

/// Explain a step for a student. `code` reads source text by range.
pub fn explain(view: &VisualizerView, step: &VizStep, code: &dyn Fn(usize, usize) -> String) -> Explanation {
    use Piece::{Code, Text, Value};
    let trace = &view.trace;
    let show = |r: &VizRef| show_value(view, r, step.index, 60);
    let src = |pos: Option<usize>, span: usize| pos.map(|p| one_line(&code(p, span))).unwrap_or_default();
    let name = step.name.clone().unwrap_or_else(|| "λ".into());
    let prev = step.index.checked_sub(1).and_then(|k| trace.steps.get(k));
    let live: Vec<&crate::racket::protocol::VizFrame> = step.live_frames().collect();
    // Primitive operations since the previous step: `(sub1 n)` → `(sub1 4)` → `3`.
    let eval_lines = |skip: Option<usize>| -> Vec<Vec<Piece>> {
        step.evals
            .iter()
            .filter(|e| e.pos.is_some() && e.pos != skip)
            .filter_map(|e| {
                let text = src(e.pos, e.span);
                let value = e.value.as_ref().map(show)?;
                let head = head_of(&text)?;
                let with_values = format!("({head}{})", e.args.iter().map(|a| format!(" {}", show(a))).collect::<String>());
                let mut line = vec![Code(text.clone())];
                if with_values != text {
                    line.push(Text(" with values ".into()));
                    line.push(Code(with_values));
                }
                line.push(Text(" gives ".into()));
                line.push(Value(value));
                Some(line)
            })
            .collect()
    };
    let mut ex = Explanation { code: src(step.pos, step.span), ..Default::default() };
    match step.kind.as_str() {
        "call" => {
            let callee = live.last().copied();
            let caller = live.len().checked_sub(2).and_then(|k| live.get(k)).copied();
            let prev_live = prev.map(|p| p.live_frames().count()).unwrap_or(0);
            let tail = !live.is_empty() && live.len() <= prev_live && prev.is_some_and(|p| p.kind != "return");
            let recursive = prev.and_then(|p| p.live_frames().last()).is_some_and(|f| f.name == name);
            ex.headline = match (recursive, tail) {
                (true, true) => format!("{name} calls itself again. It's a tail call, so the new call replaces the current one"),
                (true, false) => format!("{name} calls itself (recursion). A new frame goes on the stack"),
                (false, true) => format!("Calling {name} as a tail call: it replaces the current frame"),
                (false, false) => match caller {
                    Some(c) => format!("{} calls {name}", c.name),
                    None => format!("Calling {name}"),
                },
            };
            ex.lines = eval_lines(None);
            if let Some(f) = callee {
                let n = f.nargs.min(f.vars.len());
                if n > 0 {
                    let mut line = vec![Text("so ".into()), Code(call_text(trace, f, step.index)), Text(" runs, with ".into())];
                    for (k, (p, v)) in f.vars[..n].iter().enumerate() {
                        if k > 0 {
                            line.push(Text(if k + 1 == n { " and ".into() } else { ", ".into() }));
                        }
                        line.push(Code(p.clone()));
                        line.push(Text(" = ".into()));
                        line.push(Value(show(v)));
                    }
                    ex.lines.push(line);
                }
            }
        }
        "return" => {
            let value = step.value.as_ref().map(show).unwrap_or_else(|| "nothing".into());
            let me = live.last().map(|f| call_text(trace, f, step.index)).unwrap_or_else(|| format!("({name} …)"));
            let back_to = live.len().checked_sub(2).and_then(|k| live.get(k)).map(|f| call_text(trace, f, step.index));
            ex.headline = format!("{name} is done");
            ex.lines = eval_lines(None);
            let mut line = vec![Code(me), Text(" returns ".into()), Value(value)];
            match back_to {
                Some(b) => {
                    line.push(Text(" to ".into()));
                    line.push(Code(b));
                }
                None => line.push(Text(" to the top level".into())),
            }
            ex.lines.push(line);
        }
        "question" => {
            let answer = step.value.as_ref().map(show).unwrap_or_default();
            let question = ex.code.clone();
            // What asked the question, from the code just before it.
            let before = step.pos.map(|p| code(p.saturating_sub(12), p.min(12))).unwrap_or_default();
            let before = before.trim_end();
            let form = if before.ends_with("(if") {
                "if"
            } else if before.ends_with("(and") || before.ends_with("(or") {
                "and/or"
            } else if before.ends_with('[') || before.ends_with('(') {
                "cond"
            } else {
                ""
            };
            let (yes, no) = match form {
                "cond" => ("so the answer of this cond clause is what we evaluate next", "so cond skips this clause and tries the next one"),
                "if" => ("so if evaluates its then-branch", "so if evaluates its else-branch"),
                _ => ("", ""),
            };
            ex.lines = eval_lines(step.pos);
            let mut line = vec![Code(question.clone()), Text(" is ".into()), Value(answer.clone())];
            let then = match answer.as_str() {
                "#true" => yes,
                "#false" => no,
                _ => "",
            };
            if !then.is_empty() {
                line.push(Text(format!(", {then}")));
            }
            ex.lines.push(line);
            ex.headline = match answer.as_str() {
                "#true" => format!("Is {question} true? Yes"),
                "#false" => format!("Is {question} true? No"),
                _ => format!("Checking {question}"),
            };
        }
        "define" => {
            ex.headline = format!("Defining {name}");
            ex.lines = eval_lines(None);
            for (n, v) in step.globals.iter().filter(|(n, _)| step.name.as_deref().is_some_and(|names| names.split(", ").any(|x| x == n))) {
                ex.lines.push(match v {
                    VizRef::Prim { k, .. } if k == "fn" => vec![Code(n.clone()), Text(" is a function: it runs when it is called".into())],
                    _ => vec![Code(n.clone()), Text(" is ".into()), Value(show(v))],
                });
            }
        }
        "set" => {
            ex.headline = format!("{name} changes");
            ex.lines = eval_lines(None);
            ex.lines.push(vec![Code(name.clone()), Text(" is now ".into()), Value(step.value.as_ref().map(show).unwrap_or_default())]);
        }
        "end" => {
            ex.lines = eval_lines(None);
            if step.failed == Some(true) {
                ex.headline = "The program stopped with an error".into();
                ex.lines.push(vec![Text("see the Interactions panel for the message".into())]);
            } else {
                ex.headline = "The program finished".into();
                ex.lines.push(vec![Text(format!("{} steps", trace.steps.len()))]);
            }
        }
        k => ex.headline = k.to_string(),
    }
    ex
}

/// Everything the program printed up to and including `step`.
pub fn output_until(view: &VisualizerView, step: usize) -> String {
    view.trace.steps.iter().take(step + 1).map(|s| s.output.as_str()).collect()
}

