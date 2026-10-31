//! Reformat to Style Guide: Reindent All, plus choosing where lines break.
//!
//! Each top-level form is laid out again from scratch: a form that fits on one line stays on
//! one line, function bodies and `cond` clauses always start their own lines, and anything too
//! long for the wrap guide is broken the way DrRacket indents it (aligned under the first
//! argument, or the body two columns in). Indentation itself is left to `sexp::indent_for`, so
//! the result is exactly what Reindent All would keep.
//!
//! Only whitespace changes: a form whose result lexes to different tokens is left alone, and so
//! are forms with unbalanced brackets or multi-line strings and block comments.

use super::lexer::{Kind, Token, datum_end, lex_chars};
use super::sexp::{HeadType, head_type};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// What reformatting a document would change.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Replacements, in char offsets of the original text.
    pub edits: Vec<(Range<usize>, String)>,
    /// Top-level forms left alone because their brackets don't match.
    pub skipped: usize,
}

/// Lay out every top-level form of `chars` to fit in `width` columns where possible.
pub fn reformat(chars: &[char], toks: &[Token], width: usize) -> Outcome {
    let mut out = Outcome::default();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i];
        match t.kind {
            Kind::Open | Kind::Quote | Kind::DatumComment => {}
            Kind::Close => {
                out.skipped += 1;
                i += 1;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        let Some(end) = datum_end(toks, i) else {
            // An unclosed bracket swallows the rest of the file.
            out.skipped += 1;
            break;
        };
        let range = t.start..toks[end - 1].end;
        match format_form(chars, toks, i..end, range.clone(), width) {
            Ok(Some(text)) => out.edits.push((range, text)),
            Ok(None) => {}
            Err(Skip::Broken) => out.skipped += 1,
            Err(Skip::Verbatim) => {}
        }
        i = end;
    }
    out
}

/// Why a form is left as it is.
#[derive(Debug, PartialEq, Eq)]
enum Skip {
    /// Mismatched brackets or unreadable tokens.
    Broken,
    /// Something whose line breaks can't move (a multi-line string), or that shares its line
    /// with other code.
    Verbatim,
}

/// The new text of one top-level form, or None if it is already formatted.
fn format_form(chars: &[char], all: &[Token], form: Range<usize>, range: Range<usize>, width: usize) -> Result<Option<String>, Skip> {
    let toks = &all[form.clone()];
    // Only forms on lines of their own: `(a) (b)` on one line stays as it is.
    let line_start = chars[..range.start].iter().rposition(|&c| c == '\n').map_or(0, |p| p + 1);
    let line_end = chars[range.end..].iter().position(|&c| c == '\n').map_or(chars.len(), |p| range.end + p);
    let after: String = chars[range.end..line_end].iter().collect();
    let after = after.trim_start();
    if chars[line_start..range.start].iter().any(|c| !c.is_whitespace()) || !(after.is_empty() || after.starts_with(';')) {
        return Err(Skip::Verbatim);
    }
    if toks.iter().any(|t| t.kind == Kind::Error || t.mismatched) {
        return Err(Skip::Broken);
    }
    if toks.iter().any(|t| chars[t.start..t.end].contains(&'\n')) {
        return Err(Skip::Verbatim);
    }
    let mut tree = Tree { chars, toks: all, nodes: Vec::new(), memo: HashMap::new(), width };
    let (root, next) = tree.parse_datum(form.start, false)?;
    debug_assert_eq!(next, form.end);
    let text = tree.render(root, 0, 0, false).to_string();

    // Whitespace is all that may change.
    let old: String = chars[range].iter().collect();
    let new_chars: Vec<char> = text.chars().collect();
    let new_toks = lex_chars(&new_chars);
    let same = new_toks.len() == toks.len()
        && new_toks.iter().zip(toks).all(|(a, b)| a.kind == b.kind && new_chars[a.start..a.end] == chars[b.start..b.end]);
    if !same {
        debug_assert!(false, "reformatting changed the code:\n{old}\n=>\n{text}");
        return Err(Skip::Broken);
    }
    Ok((text != old).then_some(text))
}

/// How a list breaks across lines, by what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// A function call or data: arguments under the first argument, or all under the head.
    Call,
    /// `define`, `lambda`, `let`, `when`...: this many arguments stay on the head's line and
    /// the body goes two columns in.
    Body(usize),
    /// `begin`, `require`: everything aligned, after the head or on the lines below it.
    Seq,
    /// `cond`: like `Seq`, but each clause on its own line.
    Cond,
    /// `case`, `match`: this many arguments on the head's line, then one clause per line.
    Clauses(usize),
}

#[derive(Debug)]
enum Node {
    Atom {
        text: String,
        /// `#:keyword`: its value stays on the same line.
        keyword: bool,
        /// A plain symbol (a possible list head).
        symbol: bool,
    },
    List {
        /// Quote or `#;` in front, with the space after it as written.
        prefix: String,
        open: String,
        close: String,
        items: Vec<Item>,
        shape: Shape,
        /// One-line text, or None when it must span lines (comments, blank lines, `cond`...).
        flat: Option<String>,
    },
}

#[derive(Debug)]
enum Item {
    Datum { id: usize, blank_before: bool },
    /// `; ...`: the next thing starts a new line.
    LineComment { text: String, trailing: bool, blank_before: bool },
    /// `#| ... |#` on one line.
    BlockComment { text: String, trailing: bool, blank_before: bool },
}

/// Where the lines after the first one start.
#[derive(Clone, Copy)]
enum Cont {
    Column(usize),
    /// Under the first argument (or under the head when that isn't on the first line).
    FirstArg,
}

#[derive(Clone, Copy)]
struct Plan {
    /// Elements (head included) on the first line.
    first_line: usize,
    cont: Cont,
    /// Elements from this index on are `cond`/`match` clauses.
    clauses_from: Option<usize>,
}

struct Tree<'a> {
    chars: &'a [char],
    toks: &'a [Token],
    nodes: Vec<Node>,
    memo: HashMap<(usize, usize, usize, bool), Rc<str>>,
    width: usize,
}

fn width_of(s: &str) -> usize {
    s.chars().count()
}

/// Column after `s` when it starts at `col`.
fn end_col(s: &str, col: usize) -> usize {
    match s.rfind('\n') {
        Some(p) => width_of(&s[p + 1..]),
        None => col + width_of(s),
    }
}

impl Tree<'_> {
    fn text(&self, r: Range<usize>) -> String {
        self.chars[r].iter().collect()
    }

    /// Parse the datum at token `i`; returns its node and the index after it.
    fn parse_datum(&mut self, i: usize, quoted: bool) -> Result<(usize, usize), Skip> {
        let t = *self.toks.get(i).ok_or(Skip::Broken)?;
        match t.kind {
            Kind::Quote | Kind::DatumComment => {
                let next = *self.toks.get(i + 1).ok_or(Skip::Broken)?;
                let gap = self.text(t.end..next.start);
                if gap.chars().any(|c| c != ' ' && c != '\t') || next.is_trivia() {
                    return Err(Skip::Verbatim);
                }
                let mark = self.text(t.start..t.end);
                let quoted = match mark.as_str() {
                    "'" | "`" | "#&" => true,
                    "," | ",@" => false,
                    _ => quoted,
                };
                let (id, end) = self.parse_datum(i + 1, quoted)?;
                let pre = mark + &gap;
                match &mut self.nodes[id] {
                    Node::Atom { text, symbol, keyword } => {
                        *text = pre + text;
                        *symbol = false;
                        *keyword = false;
                    }
                    Node::List { prefix, flat, .. } => {
                        *prefix = pre.clone() + prefix;
                        if let Some(f) = flat {
                            *f = pre + f;
                        }
                    }
                }
                Ok((id, end))
            }
            Kind::Open => self.parse_list(i, quoted),
            Kind::String | Kind::ByteString | Kind::Regex => {
                let end = datum_end(self.toks, i).ok_or(Skip::Broken)?;
                let text = self.text(t.start..self.toks[end - 1].end);
                Ok((self.push(Node::Atom { text, keyword: false, symbol: false }), end))
            }
            Kind::Close | Kind::Error => Err(Skip::Broken),
            Kind::LineComment | Kind::BlockComment | Kind::HereString | Kind::Lang | Kind::LangName | Kind::Directive => {
                Err(Skip::Verbatim)
            }
            _ => {
                let text = self.text(t.start..t.end);
                let node = Node::Atom { text, keyword: t.kind == Kind::HashKeyword, symbol: t.kind == Kind::Symbol };
                Ok((self.push(node), i + 1))
            }
        }
    }

    fn push(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    fn parse_list(&mut self, o: usize, quoted: bool) -> Result<(usize, usize), Skip> {
        let t = self.toks[o];
        let close = t.partner.ok_or(Skip::Broken)? as usize;
        let mut items = Vec::new();
        let mut prev_end = t.end;
        let mut k = o + 1;
        while k < close {
            let tk = self.toks[k];
            let newlines = self.chars[prev_end..tk.start].iter().filter(|&&c| c == '\n').count();
            let (trailing, blank_before) = (newlines == 0, newlines >= 2);
            match tk.kind {
                Kind::LineComment => {
                    items.push(Item::LineComment { text: self.text(tk.start..tk.end), trailing, blank_before });
                    k += 1;
                }
                Kind::BlockComment => {
                    items.push(Item::BlockComment { text: self.text(tk.start..tk.end), trailing, blank_before });
                    k += 1;
                }
                _ => {
                    let (id, next) = self.parse_datum(k, quoted)?;
                    items.push(Item::Datum { id, blank_before });
                    k = next;
                }
            }
            prev_end = self.toks[k - 1].end;
        }
        if k != close {
            return Err(Skip::Broken);
        }

        let datums: Vec<usize> = items.iter().filter_map(|it| if let Item::Datum { id, .. } = it { Some(*id) } else { None }).collect();
        let head = match datums.first().map(|&d| &self.nodes[d]) {
            Some(Node::Atom { text, symbol: true, .. }) if !quoted => Some(text.clone()),
            _ => None,
        };
        let second_is_symbol = datums.get(1).is_some_and(|&d| matches!(self.nodes[d], Node::Atom { symbol: true, .. }));
        let second_is_list = datums.get(1).is_some_and(|&d| matches!(self.nodes[d], Node::List { .. }));
        let shape = match head.as_deref() {
            None => Shape::Call,
            Some("cond") => Shape::Cond,
            Some("case" | "match" | "match*" | "syntax-parse") => Shape::Clauses(1),
            Some("syntax-case" | "syntax-case*") => Shape::Clauses(2),
            Some("let") if second_is_symbol => Shape::Body(2),
            Some("module" | "module*") => Shape::Body(2),
            Some(h) => match head_type(h) {
                Some(HeadType::Define | HeadType::Lambda) => Shape::Body(1),
                Some(HeadType::ForFold) => Shape::Body(2),
                Some(HeadType::Begin) => Shape::Seq,
                None => Shape::Call,
            },
        };

        // Style rules that break a form even when it would fit on one line.
        let head = head.unwrap_or_default();
        let clauses = |h: usize| datums.len().saturating_sub(1 + h);
        let defines_function = head.starts_with("define")
            && !matches!(head.as_str(), "define-values" | "define-struct" | "define-syntaxes")
            && second_is_list
            && datums.len() >= 3;
        let binds = matches!(head.as_str(), "let" | "let*" | "letrec" | "let-values" | "let*-values" | "letrec-values" | "local" | "shared");
        let lambda_like = head_type(&head) == Some(HeadType::Lambda);
        let always_break = match shape {
            Shape::Cond => clauses(0) >= 2,
            Shape::Clauses(h) => clauses(h) >= 2,
            // Each body expression on its own line, and a `let`'s body below its bindings.
            Shape::Body(h) => defines_function || binds || (lambda_like && clauses(h) >= 2),
            _ => false,
        };
        // Two or more definitions or bindings go one per line.
        let bindings = match shape {
            Shape::Body(h) if binds => datums.get(h).copied(),
            _ => None,
        };
        if let Some(b) = bindings
            && let Node::List { flat, items, .. } = &mut self.nodes[b]
            && items.iter().filter(|it| matches!(it, Item::Datum { .. })).count() >= 2
        {
            *flat = None;
        }

        let multiline = items.iter().enumerate().any(|(n, it)| match it {
            Item::LineComment { .. } => true,
            Item::Datum { blank_before, .. } | Item::BlockComment { blank_before, .. } => n > 0 && *blank_before,
        });
        let open = self.text(t.start..t.end);
        let close_text = self.text(self.toks[close].start..self.toks[close].end);
        let flat = if always_break || multiline {
            None
        } else {
            let mut parts = Vec::new();
            for it in &items {
                match it {
                    Item::Datum { id, .. } => match &self.nodes[*id] {
                        Node::Atom { text, .. } => parts.push(text.clone()),
                        Node::List { flat: Some(f), .. } => parts.push(f.clone()),
                        Node::List { flat: None, .. } => {
                            parts.clear();
                            break;
                        }
                    },
                    Item::BlockComment { text, .. } => parts.push(text.clone()),
                    Item::LineComment { .. } => unreachable!(),
                }
            }
            (parts.len() == items.len()).then(|| format!("{open}{}{close_text}", parts.join(" ")))
        };
        let id = self.push(Node::List { prefix: String::new(), open, close: close_text, items, shape, flat });
        Ok((id, close + 1))
    }

    /// Text of node `id` starting at column `col`, with `trail` more characters (closing
    /// brackets) to follow on its last line. `clause`: it is a `cond`/`match` clause.
    fn render(&mut self, id: usize, col: usize, trail: usize, clause: bool) -> Rc<str> {
        if let Some(s) = self.memo.get(&(id, col, trail, clause)) {
            return s.clone();
        }
        let (flat, shape, datums) = match &self.nodes[id] {
            Node::Atom { text, .. } => return text.as_str().into(),
            Node::List { flat, shape, items, .. } => (flat.clone(), *shape, items.iter().filter(|it| matches!(it, Item::Datum { .. })).count()),
        };
        if let Some(f) = &flat
            && col + width_of(f) + trail <= self.width
        {
            let s: Rc<str> = f.as_str().into();
            self.memo.insert((id, col, trail, clause), s.clone());
            return s;
        }
        let p = col + match &self.nodes[id] {
            Node::List { prefix, .. } => width_of(prefix),
            Node::Atom { .. } => 0,
        };
        let plan = |first_line, cont, clauses_from| Plan { first_line, cont, clauses_from };
        let mut plans = Vec::new();
        match shape {
            _ if clause => plans.push(plan(1, Cont::Column(p + 1), None)),
            Shape::Call => {
                plans.push(plan(2, Cont::FirstArg, None));
                plans.push(plan(1, Cont::Column(p + 1), None));
            }
            Shape::Body(h) => {
                plans.push(plan(h + 1, Cont::Column(p + 2), None));
                if datums == h + 2 && flat.is_some() {
                    // A single short body may stay on the head's line, breaking inside it.
                    plans.push(plan(datums, Cont::Column(p + 2), None));
                }
            }
            Shape::Seq | Shape::Cond => {
                let clauses_from = (shape == Shape::Cond).then_some(1);
                plans.push(plan(2, Cont::FirstArg, clauses_from));
                plans.push(plan(1, Cont::Column(p + 2), clauses_from));
            }
            Shape::Clauses(h) => plans.push(plan(h + 1, Cont::Column(p + 2), Some(h + 1))),
        }
        let must_break = flat.is_none();
        let mut best: Option<((usize, usize), Rc<str>)> = flat.map(|f| (self.cost(&f, col, trail), f.as_str().into()));
        for pl in plans {
            let s: Rc<str> = self.emit(id, col, trail, pl).into();
            if must_break && !s.contains('\n') {
                // It must break (style rule): one line isn't an option.
                continue;
            }
            let c = self.cost(&s, col, trail);
            if best.as_ref().is_none_or(|(b, _)| c < *b) {
                best = Some((c, s));
            }
        }
        let s = best.map(|(_, s)| s).unwrap_or_else(|| "".into());
        self.memo.insert((id, col, trail, clause), s.clone());
        s
    }

    /// (columns past the wrap guide, lines): lower is better.
    fn cost(&self, s: &str, col: usize, trail: usize) -> (usize, usize) {
        let lines: Vec<&str> = s.split('\n').collect();
        let n = lines.len();
        let over = lines
            .iter()
            .enumerate()
            .map(|(k, l)| {
                let w = if k == 0 { col } else { 0 } + width_of(l) + if k == n - 1 { trail } else { 0 };
                w.saturating_sub(self.width)
            })
            .sum();
        (over, n)
    }

    /// Lay out list `id` across lines by `plan`.
    fn emit(&mut self, id: usize, col: usize, trail: usize, plan: Plan) -> String {
        let Node::List { prefix, open, close, items, .. } = &self.nodes[id] else { unreachable!() };
        let (open, close) = (open.clone(), close.clone());
        let items: Vec<(Option<usize>, String, bool, bool, bool)> = items
            .iter()
            .map(|it| match it {
                Item::Datum { id, blank_before } => (Some(*id), String::new(), false, *blank_before, false),
                Item::LineComment { text, trailing, blank_before } => (None, text.clone(), *trailing, *blank_before, true),
                Item::BlockComment { text, trailing, blank_before } => (None, text.clone(), *trailing, *blank_before, false),
            })
            .collect();
        let p = col + width_of(prefix);
        let mut out = format!("{prefix}{open}");
        let mut cur = p + width_of(&open);
        let mut datum = 0;
        let mut first_line = true;
        let mut after_line_comment = false;
        let mut glue = false; // the previous element was a `#:keyword`
        let mut first_arg: Option<usize> = None;
        let newline = |out: &mut String, cur: &mut usize, to: usize, blank: bool| {
            out.push('\n');
            if blank {
                out.push('\n');
            }
            out.push_str(&" ".repeat(to));
            *cur = to;
        };
        let cont = |first_arg: Option<usize>| match plan.cont {
            Cont::Column(c) => c,
            Cont::FirstArg => first_arg.unwrap_or(p + 1),
        };
        let n = items.len();
        for (k, (node, text, trailing, blank, line_comment)) in items.into_iter().enumerate() {
            let at_open = k == 0;
            match node {
                None => {
                    if trailing && !after_line_comment {
                        if !at_open {
                            out.push(' ');
                            cur += 1;
                        }
                    } else {
                        first_line = false;
                        newline(&mut out, &mut cur, if datum == 0 { p + 1 } else { cont(first_arg) }, blank);
                    }
                    out.push_str(&text);
                    cur += width_of(&text);
                    after_line_comment |= line_comment;
                }
                Some(child) => {
                    if datum == 0 && at_open {
                        // The head, right after the bracket.
                    } else if datum == 0 {
                        newline(&mut out, &mut cur, p + 1, blank);
                        first_line = false;
                    } else if !after_line_comment && !blank && (glue || (first_line && datum < plan.first_line)) {
                        out.push(' ');
                        cur += 1;
                        if datum == 1 && first_line {
                            first_arg = Some(cur);
                        }
                    } else {
                        first_line = false;
                        newline(&mut out, &mut cur, cont(first_arg), blank);
                    }
                    let t = if k == n - 1 { trail + width_of(&close) } else { 0 };
                    let clause = plan.clauses_from.is_some_and(|f| datum >= f);
                    let s = self.render(child, cur, t, clause);
                    cur = end_col(&s, cur);
                    out.push_str(&s);
                    glue = matches!(self.nodes[child], Node::Atom { keyword: true, .. });
                    after_line_comment = false;
                    datum += 1;
                }
            }
        }
        if after_line_comment {
            newline(&mut out, &mut cur, cont(first_arg), false);
        }
        out.push_str(&close);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::lexer::lex_chars;
    use crate::editor::sexp::{Lines, indent_for};

    struct L(Vec<usize>);
    impl Lines for L {
        fn line_of(&self, pos: usize) -> usize {
            self.0.partition_point(|&s| s <= pos) - 1
        }
    }

    /// Reindent every line, as Reindent All does.
    fn reindent(src: &str) -> String {
        let mut lines: Vec<String> = src.split('\n').map(str::to_string).collect();
        for n in 0..lines.len() {
            let text: String = lines.join("\n");
            let chars: Vec<char> = text.chars().collect();
            let toks = lex_chars(&chars);
            let mut starts = vec![0];
            starts.extend(chars.iter().enumerate().filter(|(_, c)| **c == '\n').map(|(i, _)| i + 1));
            let Some(want) = indent_for(&toks, &chars, &L(starts.clone()), starts[n]) else { continue };
            let body = lines[n].trim_start().to_string();
            lines[n] = if body.is_empty() { String::new() } else { format!("{}{body}", " ".repeat(want)) };
        }
        lines.join("\n")
    }

    /// Reformat, then reindent, as the command does.
    fn fmt_width(src: &str, width: usize) -> String {
        let chars: Vec<char> = src.chars().collect();
        let toks = lex_chars(&chars);
        let out = reformat(&chars, &toks, width);
        for (_, new) in &out.edits {
            // The layout already matches the indenter.
            assert_eq!(&reindent(new), new);
        }
        let mut text = src.to_string();
        for (r, new) in out.edits.iter().rev() {
            let a = chars[..r.start].iter().collect::<String>().len();
            let b = chars[..r.end].iter().collect::<String>().len();
            text.replace_range(a..b, new);
        }
        reindent(&text)
    }

    fn fmt(src: &str) -> String {
        fmt_width(src, 80)
    }

    /// The layout is what the indenter would produce, and formatting twice changes nothing.
    fn check(src: &str, want: &str) {
        let got = fmt(src);
        assert_eq!(got, want, "\n{got}");
        assert_eq!(fmt(&got), got, "not stable");
    }

    #[test]
    fn aqeels_set_equality() {
        check(
            ";; set=?: Set Set -> Bool\n(define (set=? set1 set2)\n  (cond [(empty? set1) (cond [(empty? set2) true] [else false])]\n        [(empty? set2) (cond [(empty? set1) true] [else false])]))\n",
            ";; set=?: Set Set -> Bool\n(define (set=? set1 set2)\n  (cond [(empty? set1)\n         (cond [(empty? set2) true]\n               [else false])]\n        [(empty? set2)\n         (cond [(empty? set1) true]\n               [else false])]))\n",
        );
    }

    #[test]
    fn function_bodies_start_their_own_line() {
        check("(define (double x) (* 2 x))", "(define (double x)\n  (* 2 x))");
        check("(define x 5)", "(define x 5)");
        check("(define-struct posn (x y))", "(define-struct posn (x y))");
        check("(define f (lambda (x) (* x x)))", "(define f (lambda (x) (* x x)))");
    }

    #[test]
    fn short_forms_are_joined() {
        check("(check-expect (double\n   2)\n    4)", "(check-expect (double 2) 4)");
        check("(if (empty? lst)\n0\n1)", "(if (empty? lst) 0 1)");
    }

    #[test]
    fn long_calls_break_under_the_first_argument() {
        check(
            "(define (f lst) (+ (first lst) (second lst) (third lst) (fourth lst) (fifth lst) (sixth lst) (seventh lst)))",
            "(define (f lst)\n  (+ (first lst)\n     (second lst)\n     (third lst)\n     (fourth lst)\n     (fifth lst)\n     (sixth lst)\n     (seventh lst)))",
        );
    }

    #[test]
    fn local_definitions_go_one_per_line() {
        check(
            "(define (f x) (local [(define a (* x 2)) (define b (+ a 1))] (+ a b)))",
            "(define (f x)\n  (local [(define a (* x 2))\n          (define b (+ a 1))]\n    (+ a b)))",
        );
    }

    #[test]
    fn comments_and_blank_lines_stay() {
        check(
            "(define (f x)\n  ;; double it\n  (* 2 x)) ; done\n\n\n(define y 1)",
            "(define (f x)\n  ;; double it\n  (* 2 x)) ; done\n\n\n(define y 1)",
        );
        check("(list 1 ; one\n 2)", "(list 1 ; one\n      2)");
        check("(begin\n  (a)\n\n  (b))", "(begin (a)\n\n       (b))");
    }

    #[test]
    fn keywords_keep_their_values() {
        check(
            "(make-thing #:name \"something long enough\" #:size 1234567 #:color \"a long color name\" #:shape 'circle)",
            "(make-thing #:name \"something long enough\"\n            #:size 1234567\n            #:color \"a long color name\"\n            #:shape 'circle)",
        );
    }

    #[test]
    fn quoted_data_is_not_code() {
        check("'(cond [a b] [c d])", "'(cond [a b] [c d])");
    }

    #[test]
    fn match_clauses_go_one_per_line() {
        check("(match x [1 'one] [_ 'other])", "(match x\n  [1 'one]\n  [_ 'other])");
    }

    #[test]
    fn let_bodies_and_bindings_break() {
        check("(let ([a 1]) a)", "(let ([a 1])\n  a)");
        check("(let* ([a 1] [b 2]) (+ a b))", "(let* ([a 1]\n       [b 2])\n  (+ a b))");
        check("(let loop ([i 0] [acc 0]) (loop i acc))", "(let loop ([i 0]\n           [acc 0])\n  (loop i acc))");
        check("(when (positive? 1) (display \"positive\") (newline))", "(when (positive? 1)\n  (display \"positive\")\n  (newline))");
        check("(when ok (go))", "(when ok (go))");
        check("(lambda (x) (+ x n))", "(lambda (x) (+ x n))");
        check("(struct posn (x y) #:transparent)", "(struct posn (x y) #:transparent)");
        check("(big-bang 0 [on-tick add1] [to-draw draw])", "(big-bang 0\n  [on-tick add1]\n  [to-draw draw])");
    }

    #[test]
    fn broken_and_verbatim_forms_are_left_alone() {
        let src = "(define (f x) (* 2 x))\n(define s \"two\nlines\") (g)\n(define (g x] x)\n";
        let chars: Vec<char> = src.chars().collect();
        let out = reformat(&chars, &lex_chars(&chars), 80);
        assert_eq!(out.edits.len(), 1);
        assert_eq!(out.skipped, 1);
        let src = "(define (f x) (* 2 x))\n(g (h x)";
        let chars: Vec<char> = src.chars().collect();
        let out = reformat(&chars, &lex_chars(&chars), 80);
        assert_eq!((out.edits.len(), out.skipped), (1, 1));
    }

    /// Every indentation fixture stays the same code, is a fixed point of the indenter, and is
    /// stable, at several widths.
    #[test]
    fn fixtures_only_change_whitespace() {
        if let Ok(out) = std::env::var("FORMAT_DUMP") {
            let src = std::fs::read_to_string("tests/indent/student.rkt").unwrap();
            std::fs::write(out, fmt_width(&src, 80)).unwrap();
        }
        for f in ["tests/indent/forms.rkt", "tests/indent/student.rkt"] {
            let src = std::fs::read_to_string(f).unwrap();
            for width in [40, 80, 102] {
                let got = fmt_width(&src, width);
                let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                assert_eq!(strip(&got), strip(&src), "{f} at {width}");
                assert_eq!(fmt_width(&got, width), got, "{f} at {width} is not stable");
            }
        }
    }
}
