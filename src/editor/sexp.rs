//! S-expression structure over the token stream: bracket matching, sexp navigation,
//! selection expansion, and Racket indentation (a port of DrRacket's).

use super::lexer::{Kind, Token, datum_end, token_index_at};

/// Index of the innermost Open token whose list contains `pos` (strictly after the open
/// bracket and at or before its close).
pub fn enclosing_open(toks: &[Token], pos: usize) -> Option<usize> {
    let mut j = token_index_at(toks, pos);
    // Tokens [0, j) end at or before pos. Walk backwards skipping closed lists.
    while j > 0 {
        j -= 1;
        let t = &toks[j];
        match t.kind {
            Kind::Close => {
                if let Some(p) = t.partner {
                    j = p as usize;
                }
            }
            Kind::Open
                if t.end <= pos && t.partner.is_none_or(|p| toks[p as usize].start >= pos) => {
                    return Some(j);
                }
            _ => {}
        }
    }
    None
}

/// The outermost list containing `pos` (strictly after its open bracket, at or before its
/// close), from its open bracket to after its close (or `usize::MAX` while it is unclosed).
pub fn top_level_form(toks: &[Token], pos: usize) -> Option<std::ops::Range<usize>> {
    let mut open = enclosing_open(toks, pos)?;
    while let Some(o) = enclosing_open(toks, toks[open].start) {
        open = o;
    }
    let end = toks[open].partner.map_or(usize::MAX, |p| toks[p as usize].end);
    Some(toks[open].start..end)
}

/// The matching bracket pair adjacent to the cursor: the cursor is right after a close
/// bracket or right before an open bracket. Returns (open token idx, close token idx).
pub fn bracket_pair_at(toks: &[Token], pos: usize) -> Option<(usize, usize)> {
    let i = token_index_at(toks, pos);
    // Right before an open (or inside a multi-char opener like `#hash(`).
    if let Some(t) = toks.get(i)
        && t.kind == Kind::Open
        && t.start <= pos
        && pos < t.end
        && let Some(p) = t.partner
    {
        return Some((i, p as usize));
    }
    if let Some(t) = toks.get(i)
        && t.kind == Kind::Open
        && t.start == pos
        && let Some(p) = t.partner
    {
        return Some((i, p as usize));
    }
    // Right after a close.
    if i > 0 {
        let t = &toks[i - 1];
        if t.kind == Kind::Close && t.end == pos
            && let Some(p) = t.partner
        {
            return Some((p as usize, i - 1));
        }
    }
    None
}

fn skip_trivia(toks: &[Token], mut i: usize) -> usize {
    while i < toks.len() && toks[i].is_trivia() {
        i += 1;
    }
    i
}

/// Start position of the datum ending at token `last` (inclusive), including quote prefixes
/// and string pieces.
fn datum_start(toks: &[Token], last: usize) -> usize {
    let mut i = match toks[last].kind {
        Kind::Close => toks[last].partner.map(|p| p as usize).unwrap_or(last),
        Kind::String | Kind::StringEscape | Kind::ByteString | Kind::Regex => {
            let mut k = last;
            while k > 0 && matches!(toks[k - 1].kind, Kind::String | Kind::StringEscape | Kind::ByteString | Kind::Regex) && toks[k - 1].end == toks[k].start {
                k -= 1;
            }
            k
        }
        _ => last,
    };
    while i > 0 && matches!(toks[i - 1].kind, Kind::Quote | Kind::DatumComment) && toks[i - 1].end == toks[i].start {
        i -= 1;
    }
    toks[i].start
}

/// Position after the next sexp starting at or after `pos`, or None at the end of a list.
pub fn forward_sexp(toks: &[Token], pos: usize) -> Option<usize> {
    let mut i = token_index_at(toks, pos);
    // If inside a token (e.g. middle of a symbol), finish that token.
    if let Some(t) = toks.get(i)
        && t.start < pos
        && t.kind != Kind::Open
    {
        if matches!(t.kind, Kind::String | Kind::StringEscape) {
            // Inside a string: go to the end of the whole string.
            let mut k = i;
            while k + 1 < toks.len() && matches!(toks[k + 1].kind, Kind::String | Kind::StringEscape) && toks[k + 1].start == toks[k].end {
                k += 1;
            }
            return Some(toks[k].end);
        }
        return Some(t.end);
    }
    i = skip_trivia(toks, i);
    let t = toks.get(i)?;
    if t.kind == Kind::Close {
        return None;
    }
    let end = datum_end(toks, i)?;
    Some(toks[end - 1].end)
}

/// Position of the start of the previous sexp ending at or before `pos`.
pub fn backward_sexp(toks: &[Token], pos: usize) -> Option<usize> {
    let mut i = token_index_at(toks, pos);
    if let Some(t) = toks.get(i)
        && t.start < pos
        && t.kind != Kind::Close
    {
        return Some(datum_start(toks, i));
    }
    while i > 0 && toks[i - 1].is_trivia() {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let t = &toks[i - 1];
    if t.kind == Kind::Open {
        return None;
    }
    Some(datum_start(toks, i - 1))
}

/// Start of the enclosing list (`backward-up-sexp`).
pub fn up_sexp(toks: &[Token], pos: usize) -> Option<usize> {
    enclosing_open(toks, pos).map(|o| datum_start(toks, o))
}

/// End of the enclosing list (`forward-up-sexp`).
pub fn forward_up_sexp(toks: &[Token], pos: usize) -> Option<usize> {
    let o = enclosing_open(toks, pos)?;
    toks[o].partner.map(|p| toks[p as usize].end)
}

/// Just inside the next list (`down-sexp`).
pub fn down_sexp(toks: &[Token], pos: usize) -> Option<usize> {
    let i = token_index_at(toks, pos);
    toks[i..].iter().take_while(|t| t.kind != Kind::Close).find(|t| t.kind == Kind::Open).map(|t| t.end)
}

/// The smallest syntactic unit strictly larger than `start..end`: token, list interior,
/// whole list, and so on outward. Used for "expand selection".
pub fn expand_selection(toks: &[Token], start: usize, end: usize) -> Option<(usize, usize)> {
    // A caret touching the end of a token selects that token.
    let i = token_index_at(toks, start);
    if start == end
        && i > 0
        && toks[i - 1].end == start
        && !matches!(toks[i - 1].kind, Kind::Open | Kind::Close)
        && toks.get(i).is_none_or(|t| t.start > start)
    {
        return Some((toks[i - 1].start, toks[i - 1].end));
    }
    // A token containing the range.
    if let Some(t) = toks.get(i)
        && t.start <= start
        && t.end >= end
        && (t.start, t.end) != (start, end)
        && !matches!(t.kind, Kind::Open | Kind::Close)
    {
        if matches!(t.kind, Kind::String | Kind::StringEscape) {
            let s = datum_start(toks, i);
            let e = datum_end(toks, token_index_at(toks, s)).map(|e| toks[e - 1].end).unwrap_or(t.end);
            return Some((s, e));
        }
        return Some((t.start, t.end));
    }
    // Enclosing list: first its interior, then the whole list.
    let mut probe = start;
    loop {
        let o = enclosing_open(toks, probe)?;
        let c = toks[o].partner? as usize;
        let interior = (toks[o].end, toks[c].start);
        let whole = (datum_start(toks, o), toks[c].end);
        if (interior.0 < start || interior.1 > end) && interior.0 <= start && interior.1 >= end && interior != (start, end) {
            // Only use the interior if it is strictly bigger than the selection.
            return Some(interior);
        }
        if whole.0 <= start && whole.1 >= end && whole != (start, end) {
            return Some(whole);
        }
        if toks[o].start == 0 {
            return None;
        }
        probe = toks[o].start;
    }
}

// ---------------------------------------------------------------------------------------
// Indentation: a port of DrRacket's `racket-amount-to-indent`
// (syntax-color/racket-indentation.rkt) and its default keyword table
// (syntax-color/private/racket-tabify.rkt), so Tab and Reindent All agree with DrRacket.

/// How DrRacket indents the body of a form, by the form's head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadType {
    /// `begin`, `cond`, `require`...: a first argument on its own line indents 2, later
    /// lines align with the line above.
    Begin,
    /// `define`, `local`, `struct`...: every line after the head indents 2.
    Define,
    /// `lambda`, `let`, `when`...: a first argument on its own line indents 4, the rest 2.
    Lambda,
    /// `for/fold`, `for/lists`: the second argument aligns with the first, the rest indent 2.
    ForFold,
}

/// DrRacket's default table, then its regexps: `^begin`, `^def`, `^(for\*?(/|$)|with-)`.
pub(crate) fn head_type(head: &str) -> Option<HeadType> {
    use HeadType::*;
    let t = match head {
        "for/fold" | "for/fold:" | "for*/fold" | "for*/fold:" | "for/lists" | "for/lists:" | "for*/lists"
        | "for*/lists:" => ForFold,
        "struct" | "local" | "struct:" | "pdefine:" | "match-define" | "match-define-values" | "pattern" => Define,
        "case-lambda" | "case-lambda:" | "pcase-lambda:" | "match-lambda" | "match-lambda*" | "syntax-parser" | "cond"
        | "delay" | "unit" | "compound-unit" | "public" | "private" | "override" | "require" | "inherit"
        | "with-output-to-string" | "with-output-to-bytes" | "with-module-reading-parameterization" => Begin,
        "cases" | "instantiate" | "super-instantiate" | "syntax/loc" | "quasisyntax/loc" | "datum-case" | "match"
        | "match*" | "match-let" | "match-let*" | "match-letrec" | "λ" | "lambda" | "let" | "let*" | "letrec" | "recur"
        | "lambda/kw" | "letrec-values" | "with-syntax" | "with-syntax*" | "with-continuation-mark" | "module"
        | "module*" | "module+" | "let/cc" | "let/ec" | "let-syntax" | "letrec-syntax" | "letrec-syntaxes+values"
        | "let:" | "letrec:" | "let*:" | "let-values:" | "letrec-values:" | "let*-values:" | "let/cc:" | "let/ec:"
        | "lambda:" | "λ:" | "plambda:" | "opt-lambda:" | "popt-lambda:" | "splicing-let" | "splicing-letrec"
        | "splicing-let-values" | "splicing-letrec-values" | "splicing-let-syntax" | "splicing-letrec-syntax"
        | "splicing-let-syntaxes" | "splicing-letrec-syntaxes" | "splicing-letrec-syntaxes+values" | "splicing-local"
        | "splicing-parameterize" | "splicing-syntax-parameterize" | "do:" | "kernel-syntax-case" | "syntax-case"
        | "syntax-case*" | "syntax-rules" | "syntax-id-rules" | "syntax-parse" | "fluid-let" | "let-struct"
        | "let-values" | "let*-values" | "case" | "when" | "unless" | "class" | "class*" | "rec" | "make-object"
        | "mixin" | "do" | "opt-lambda" | "send*" | "with-method" | "define-record" | "shared" | "with-handlers"
        | "interface" | "parameterize" | "parameterize*" | "syntax-parameterize" | "call-with-input-file"
        | "call-with-input-file*" | "with-input-from-file" | "with-input-from-string" | "call-with-output-file"
        | "with-output-to-file" | "for-all" | "big-bang" | "type-case" => Lambda,
        _ if head.starts_with("begin") => Begin,
        _ if head.starts_with("def") => Define,
        _ if ["for", "for*"].contains(&head) || head.starts_with("for/") || head.starts_with("for*/") || head.starts_with("with-") => {
            Lambda
        }
        _ => return None,
    };
    Some(t)
}

/// Line lookup supplied by the caller (keeps this module independent of the rope).
pub trait Lines {
    fn line_of(&self, pos: usize) -> usize;
}

/// Column of `pos` on its line, with tabs to the next multiple of 8 (DrRacket's
/// `visual-offset`).
fn visual_col(text: &[char], pos: usize) -> usize {
    let start = text[..pos].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
    text[start..pos].iter().fold(0, |col, &c| if c == '\t' { col + 8 - col % 8 } else { col + 1 })
}

/// Desired indentation (in columns) for the line starting at `line_start`, or None if the
/// line is inside a multi-line string or block comment and must be left alone.
pub fn indent_for(toks: &[Token], text: &[char], lines: &impl Lines, line_start: usize) -> Option<usize> {
    use HeadType::*;
    let i = token_index_at(toks, line_start);
    if let Some(t) = toks.get(i)
        && t.start < line_start
        && matches!(t.kind, Kind::String | Kind::StringEscape | Kind::BlockComment | Kind::HereString | Kind::ByteString | Kind::Regex)
    {
        return None;
    }
    let Some(o) = enclosing_open(toks, line_start) else { return Some(0) };

    // The list's elements that start before this line.
    let mut kids: Vec<usize> = Vec::new();
    let mut k = o + 1;
    loop {
        k = skip_trivia(toks, k);
        match toks.get(k) {
            Some(t) if t.kind != Kind::Close && t.start < line_start => {
                kids.push(k);
                k = datum_end(toks, k).unwrap_or(k + 1);
            }
            _ => break,
        }
    }
    // Nothing before this line: just inside the bracket.
    let (Some(&first), Some(&last)) = (kids.first(), kids.last()) else {
        return Some(visual_col(text, toks[o].start) + 1);
    };
    let head = &toks[first];
    let contains = head.start;
    let col = visual_col(text, contains);
    let token_text = |t: &Token| text[t.start..t.end].iter().collect::<String>();
    let head_type = if head.kind == Kind::Symbol { head_type(&token_text(head)) } else { None };

    // This line holds the first argument.
    if kids.len() == 1 {
        return Some(col + match head_type {
            Some(Begin | Define) => 1,
            Some(Lambda) => 3,
            _ => 0,
        });
    }
    if head_type == Some(ForFold) && kids.len() == 2 {
        return Some(visual_col(text, toks[last].start));
    }
    if matches!(head_type, Some(Define | Lambda | ForFold)) {
        return Some(col + 1);
    }
    let all_hyphens = |t: Option<&Token>| {
        t.is_some_and(|t| t.kind == Kind::Symbol && t.end - t.start >= 3 && text[t.start..t.end].iter().all(|&c| c == '-'))
    };
    if lines.line_of(contains) == lines.line_of(toks[last].start) {
        // Everything so far is on the head's line: align with the first argument, except
        // after a keyword or `---` head, a lone `...`, or before a `---` line.
        if head.kind == Kind::HashKeyword || all_hyphens(Some(head)) {
            return Some(col);
        }
        let second = &toks[kids[1]];
        if datum_end(toks, kids[1]) == Some(kids[1] + 1) && token_text(second) == "..." {
            let third_end = datum_end(toks, kids[1] + 1).map(|e| toks[e - 1].end);
            if third_end.is_none_or(|e| lines.line_of(e) != lines.line_of(second.start)) {
                return Some(col);
            }
        }
        if all_hyphens(toks.get(skip_trivia(toks, i))) {
            return Some(col);
        }
        let head_end = toks[datum_end(toks, first).unwrap_or(first + 1) - 1].end;
        let arg = (head_end..text.len()).find(|&p| text[p] == '\n' || !text[p].is_whitespace()).unwrap_or(text.len());
        return Some(visual_col(text, arg));
    }
    // Align with the first element on the line of the last one.
    let last_line = lines.line_of(toks[last].start);
    let row_start = kids.iter().rev().take_while(|&&k| lines.line_of(toks[k].start) == last_line).last().unwrap_or(&last);
    Some(visual_col(text, toks[*row_start].start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::lexer::lex_chars;

    struct L(Vec<usize>);
    impl Lines for L {
        fn line_of(&self, pos: usize) -> usize {
            self.0.partition_point(|&s| s <= pos) - 1
        }
    }

    #[test]
    fn top_level_form_is_the_outermost_list() {
        let src: Vec<char> = "(define (f x)\n  (cond [x 1]))\n(g".chars().collect();
        let toks = lex_chars(&src);
        assert_eq!(top_level_form(&toks, 23), Some(0..29)); // inside [x 1]
        assert_eq!(top_level_form(&toks, 29), None); // after the close
        assert_eq!(top_level_form(&toks, 0), None); // before the open
        assert_eq!(top_level_form(&toks, 32), Some(30..usize::MAX)); // unclosed
    }

    /// Reindent every line of `src` and return the result.
    fn reindent(src: &str) -> String {
        let mut lines: Vec<String> = src.lines().map(|l| l.trim_start().to_string()).collect();
        for n in 0..lines.len() {
            let text = lines.join("\n");
            let chars: Vec<char> = text.chars().collect();
            let toks = lex_chars(&chars);
            let mut starts = vec![0];
            for (i, c) in chars.iter().enumerate() {
                if *c == '\n' {
                    starts.push(i + 1);
                }
            }
            let ind = indent_for(&toks, &chars, &L(starts.clone()), starts[n]).unwrap_or(0);
            lines[n] = format!("{}{}", " ".repeat(ind), lines[n].trim_start());
        }
        lines.join("\n")
    }

    #[test]
    fn define_and_function_calls() {
        let expected = "(define (f x)\n  (+ x\n     1))";
        assert_eq!(reindent(expected), expected);
        let expected = "(foo\n bar\n baz)";
        assert_eq!(reindent(expected), expected);
    }

    #[test]
    fn cond_and_let() {
        let expected = "(cond\n  [(empty? l) 0]\n  [else\n   (+ 1 (len (rest l)))])";
        assert_eq!(reindent(expected), expected);
        let expected = "(cond [a b]\n      [c d])";
        assert_eq!(reindent(expected), expected);
        let expected = "(let ([x 1]\n      [y 2])\n  (+ x y))";
        assert_eq!(reindent(expected), expected);
        let expected = "(let loop ([i 0])\n  (loop i))";
        assert_eq!(reindent(expected), expected);
    }

    #[test]
    fn lambda_distinguished_args() {
        let expected = "(lambda\n    (x)\n  x)";
        assert_eq!(reindent(expected), expected);
        let expected = "(for/fold ([acc 0])\n          ([x xs])\n  (+ acc x))";
        assert_eq!(reindent(expected), expected);
    }

    #[test]
    fn data_lists_and_strings() {
        let expected = "'(a b\n    c)";
        assert_eq!(reindent(expected), expected);
        let expected = "(list 1\n      2)";
        assert_eq!(reindent(expected), expected);
        let expected = "(require racket/list\n         racket/string)";
        assert_eq!(reindent(expected), expected);
    }

    #[test]
    fn navigation() {
        let src = "(a (b c) \"s\" 'd)";
        let chars: Vec<char> = src.chars().collect();
        let toks = lex_chars(&chars);
        assert_eq!(forward_sexp(&toks, 0), Some(src.len()));
        assert_eq!(forward_sexp(&toks, 2), Some(8));
        assert_eq!(backward_sexp(&toks, 8), Some(3));
        assert_eq!(forward_sexp(&toks, 8), Some(12));
        assert_eq!(backward_sexp(&toks, 15), Some(13));
        assert_eq!(up_sexp(&toks, 5), Some(3));
        assert_eq!(forward_up_sexp(&toks, 5), Some(8));
        assert_eq!(down_sexp(&toks, 0), Some(1));
        assert_eq!(bracket_pair_at(&toks, 8).map(|(o, c)| (toks[o].start, toks[c].start)), Some((3, 7)));
        assert_eq!(expand_selection(&toks, 5, 5), Some((4, 5)));
        assert_eq!(expand_selection(&toks, 4, 5), Some((4, 7)));
        assert_eq!(expand_selection(&toks, 4, 7), Some((3, 8)));
        assert_eq!(expand_selection(&toks, 3, 8), Some((1, 15)));
        assert_eq!(expand_selection(&toks, 1, 15), Some((0, 16)));
    }
}
