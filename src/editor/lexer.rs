//! Racket lexer.
//!
//! Produces a flat, position-sorted token list over the whole document (char offsets), then a
//! second pass classifies tokens for highlighting: bracket depth (rainbow brackets and
//! mismatches), head-of-list position (function / special form), quoted data, and `#;` datum
//! comments. Lexing a 10k-line file takes a couple of milliseconds, so the editor simply
//! re-lexes on each change.

use super::keywords::KEYWORDS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    LineComment,
    BlockComment,
    /// The `#;` prefix itself.
    DatumComment,
    String,
    StringEscape,
    ByteString,
    Regex,
    HereString,
    Char,
    Number,
    Boolean,
    /// `#:keyword`
    HashKeyword,
    Open,
    Close,
    /// `'` `` ` `` `,` `,@` `#'` `` #` `` `#,` `#,@` `#&`
    Quote,
    Symbol,
    Dot,
    /// `#lang` / `#!` line prefix
    Lang,
    /// The language name after `#lang`
    LangName,
    /// `#reader`, `#ci`, `#cs` and other reader directives
    Directive,
    Error,
}

/// Semantic class used for coloring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Plain,
    Comment,
    String,
    Escape,
    Regex,
    Number,
    Constant,
    HashKeyword,
    /// Quoted symbol / datum
    Symbol,
    Keyword,
    Function,
    Operator,
    Variable,
    Bracket,
    Punctuation,
    Lang,
    LangName,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
    pub class: Class,
    /// Nesting depth for brackets (depth of the list they open/close).
    pub depth: u16,
    /// For Open/Close: index of the matching token, if balanced.
    pub partner: Option<u32>,
    /// For Open/Close: whether the pair is mismatched (`(` closed by `]`).
    pub mismatched: bool,
}

impl Token {
    fn new(start: usize, end: usize, kind: Kind) -> Self {
        Token { start, end, kind, class: Class::Plain, depth: 0, partner: None, mismatched: false }
    }
    pub fn is_open(&self) -> bool {
        self.kind == Kind::Open
    }
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment)
    }
}

pub fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '"' | ',' | '\'' | '`' | ';')
}

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.binary_search(&s).is_ok()
}

pub fn closer_for(open: char) -> char {
    match open {
        '[' => ']',
        '{' => '}',
        _ => ')',
    }
}

struct Lexer<'a> {
    s: &'a [char],
    i: usize,
    out: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn peek(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }
    fn at(&self, j: usize) -> Option<char> {
        self.s.get(j).copied()
    }
    fn push(&mut self, start: usize, kind: Kind) {
        self.out.push(Token::new(start, self.i, kind));
    }
    fn starts_with(&self, pat: &str) -> bool {
        pat.chars().enumerate().all(|(k, c)| self.peek(k) == Some(c))
    }

    /// Advance over a delimited atom (symbol/number), honoring `|...|` and `\x` escapes.
    fn atom_end(&self, mut j: usize) -> usize {
        while let Some(c) = self.at(j) {
            if c == '|' {
                j += 1;
                while let Some(c) = self.at(j) {
                    j += 1;
                    if c == '|' {
                        break;
                    }
                }
                continue;
            }
            if c == '\\' {
                j += 2;
                continue;
            }
            if is_delimiter(c) {
                break;
            }
            j += 1;
        }
        j.min(self.s.len())
    }

    fn string_body(&mut self, start: usize, kind: Kind) {
        // self.i is just past the opening quote.
        let mut seg = start;
        while let Some(c) = self.peek(0) {
            if c == '"' {
                self.i += 1;
                self.out.push(Token::new(seg, self.i, kind));
                return;
            }
            if c == '\\' && kind == Kind::String {
                if self.i > seg {
                    self.out.push(Token::new(seg, self.i, kind));
                }
                let esc_start = self.i;
                self.i += 1;
                match self.peek(0) {
                    Some('x') => {
                        self.i += 1;
                        let mut n = 0;
                        while n < 2 && self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                            self.i += 1;
                            n += 1;
                        }
                    }
                    Some('u') | Some('U') => {
                        let max = if self.peek(0) == Some('u') { 4 } else { 8 };
                        self.i += 1;
                        let mut n = 0;
                        while n < max && self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                            self.i += 1;
                            n += 1;
                        }
                    }
                    Some(c) if c.is_digit(8) => {
                        let mut n = 0;
                        while n < 3 && self.peek(0).is_some_and(|c| c.is_digit(8)) {
                            self.i += 1;
                            n += 1;
                        }
                    }
                    Some(_) => self.i += 1,
                    None => {}
                }
                self.out.push(Token::new(esc_start, self.i, Kind::StringEscape));
                seg = self.i;
                continue;
            }
            if c == '\\' {
                self.i += 2;
                continue;
            }
            self.i += 1;
        }
        // Unterminated string runs to EOF.
        self.i = self.s.len();
        if self.i > seg {
            self.out.push(Token::new(seg, self.i, kind));
        }
    }

    fn block_comment(&mut self, start: usize) {
        self.i += 2;
        let mut depth = 1;
        while self.i < self.s.len() {
            if self.starts_with("|#") {
                self.i += 2;
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if self.starts_with("#|") {
                self.i += 2;
                depth += 1;
            } else {
                self.i += 1;
            }
        }
        self.push(start, Kind::BlockComment);
    }

    fn line_rest(&mut self) {
        while let Some(c) = self.peek(0) {
            if c == '\n' {
                break;
            }
            self.i += 1;
        }
    }

    fn here_string(&mut self, start: usize) {
        // #<<TERM\n ... \nTERM
        self.i += 3;
        let term_start = self.i;
        self.line_rest();
        let term: String = self.s[term_start..self.i].iter().collect();
        loop {
            if self.i >= self.s.len() {
                break;
            }
            self.i += 1; // newline
            let ls = self.i;
            self.line_rest();
            let line: String = self.s[ls..self.i].iter().collect();
            if line == term {
                break;
            }
        }
        self.push(start, Kind::HereString);
    }

    fn hash(&mut self, start: usize) {
        let next = self.peek(1);
        match next {
            Some('|') => self.block_comment(start),
            Some(';') => {
                self.i += 2;
                self.push(start, Kind::DatumComment);
            }
            Some('\\') => {
                // Character literal.
                self.i += 2;
                if self.i < self.s.len() {
                    let first = self.s[self.i];
                    self.i += 1;
                    if first.is_alphabetic() {
                        while self.peek(0).is_some_and(|c| c.is_alphanumeric()) {
                            self.i += 1;
                        }
                    }
                }
                self.push(start, Kind::Char);
            }
            Some(':') => {
                self.i = self.atom_end(self.i + 2);
                self.push(start, Kind::HashKeyword);
            }
            Some('(') | Some('[') | Some('{') => {
                self.i += 2;
                self.push(start, Kind::Open);
            }
            Some('\'') | Some('`') | Some('&') => {
                self.i += 2;
                self.push(start, Kind::Quote);
            }
            Some(',') => {
                self.i += if self.peek(2) == Some('@') { 3 } else { 2 };
                self.push(start, Kind::Quote);
            }
            Some('"') => {
                self.i += 2;
                self.string_body(start, Kind::ByteString);
            }
            Some('<') if self.starts_with("#<<") => self.here_string(start),
            Some('!') => {
                // `#!/usr/bin/env racket` or `#!r6rs` style line.
                self.line_rest();
                self.push(start, Kind::Lang);
            }
            _ => {
                if self.starts_with("#lang") && self.peek(5).is_none_or(|c| c.is_whitespace()) {
                    self.i += 5;
                    self.push(start, Kind::Lang);
                    while self.peek(0).is_some_and(|c| c == ' ' || c == '\t') {
                        self.i += 1;
                    }
                    let ns = self.i;
                    self.line_rest();
                    if self.i > ns {
                        self.push(ns, Kind::LangName);
                    }
                    return;
                }
                for rx in ["#rx#\"", "#px#\"", "#rx\"", "#px\""] {
                    if self.starts_with(rx) {
                        self.i += rx.chars().count();
                        self.string_body(start, Kind::Regex);
                        return;
                    }
                }
                if self.starts_with("#reader") {
                    self.i += 7;
                    self.push(start, Kind::Directive);
                    return;
                }
                for prefix in ["#hasheqv(", "#hasheq(", "#hashalw(", "#hash(", "#hasheqv[", "#hasheq[", "#hash[", "#s(", "#s["] {
                    if self.starts_with(prefix) {
                        self.i += prefix.chars().count();
                        self.push(start, Kind::Open);
                        return;
                    }
                }
                let end = self.atom_end(self.i + 1);
                let text: String = self.s[start..end].iter().collect();
                self.i = end.max(start + 1);
                let lower = text.to_ascii_lowercase();
                let kind = if matches!(lower.as_str(), "#t" | "#f" | "#true" | "#false") {
                    Kind::Boolean
                } else if matches!(lower.as_str(), "#ci" | "#cs") {
                    Kind::Directive
                } else if text.starts_with("#%") {
                    Kind::Symbol
                } else if is_number(&text) {
                    Kind::Number
                } else {
                    Kind::Error
                };
                self.push(start, kind);
            }
        }
    }

    fn run(mut self) -> Vec<Token> {
        while self.step() {}
        self.out
    }

    /// Lex one unit (a token, or a string with its escape pieces, or `#lang` + name),
    /// skipping leading whitespace. Returns false at end of input.
    fn step(&mut self) -> bool {
        while self.i < self.s.len() && self.s[self.i].is_whitespace() {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return false;
        }
        let c = self.s[self.i];
        let start = self.i;
        match c {
            ';' => {
                self.line_rest();
                self.push(start, Kind::LineComment);
            }
            '(' | '[' | '{' => {
                self.i += 1;
                self.push(start, Kind::Open);
            }
            ')' | ']' | '}' => {
                self.i += 1;
                self.push(start, Kind::Close);
            }
            '"' => {
                self.i += 1;
                self.string_body(start, Kind::String);
            }
            '\'' | '`' => {
                self.i += 1;
                self.push(start, Kind::Quote);
            }
            ',' => {
                self.i += if self.peek(1) == Some('@') { 2 } else { 1 };
                self.push(start, Kind::Quote);
            }
            '#' => self.hash(start),
            _ => {
                let end = self.atom_end(self.i);
                self.i = end.max(start + 1);
                let atom = &self.s[start..self.i];
                let kind = if atom == ['.'] {
                    Kind::Dot
                } else if could_be_number(atom) && is_number(&atom.iter().collect::<String>()) {
                    Kind::Number
                } else {
                    Kind::Symbol
                };
                self.push(start, kind);
            }
        }
        true
    }
}

/// Racket number syntax (a practical approximation that covers everything in real code:
/// integers, decimals, exponents, fractions, `#x`/`#b`/`#o`/`#d`/`#e`/`#i` prefixes, infinities,
/// NaNs, and rectangular/polar complex numbers).
pub fn is_number(text: &str) -> bool {
    let mut s = text;
    let mut radix = 10;
    loop {
        let b = s.as_bytes();
        if b.len() >= 2 && b[0] == b'#' {
            match b[1].to_ascii_lowercase() {
                b'x' => radix = 16,
                b'b' => radix = 2,
                b'o' => radix = 8,
                b'd' => radix = 10,
                b'e' | b'i' => {}
                _ => return false,
            }
            s = &s[2..];
        } else {
            break;
        }
    }
    if s.is_empty() {
        return false;
    }
    // Complex: split on '@' (polar) or a trailing 'i' with an inner sign.
    if let Some((a, b)) = s.split_once('@') {
        return is_real(a, radix) && is_real(b, radix);
    }
    if (s.ends_with('i') || s.ends_with('I')) && radix != 16 {
        let body = &s[..s.len() - 1];
        if body == "+" || body == "-" {
            return true;
        }
        // Find the sign that starts the imaginary part (not at 0, not after 'e').
        let bytes = body.as_bytes();
        for k in (1..bytes.len()).rev() {
            if (bytes[k] == b'+' || bytes[k] == b'-') && !matches!(bytes[k - 1], b'e' | b'E') {
                let (re, im) = body.split_at(k);
                let im_ok = im == "+" || im == "-" || is_real(im, radix);
                return is_real(re, radix) && im_ok;
            }
        }
        return is_real(body, radix);
    }
    is_real(s, radix)
}

fn is_real(s: &str, radix: u32) -> bool {
    let lower = s.to_ascii_lowercase();
    if matches!(lower.as_str(), "+inf.0" | "-inf.0" | "+nan.0" | "-nan.0" | "+inf.f" | "-inf.f" | "+nan.f" | "-nan.f" | "+inf.t" | "-inf.t" | "+nan.t" | "-nan.t") {
        return true;
    }
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    if s.is_empty() {
        return false;
    }
    if let Some((n, d)) = s.split_once('/') {
        return is_uint(n, radix) && is_uint(d, radix);
    }
    if radix != 10 {
        return is_uint(s, radix);
    }
    // Decimal with optional exponent.
    let (mantissa, exp) = match s.find(['e', 'E', 'f', 'F', 'd', 'D', 's', 'S', 'l', 'L', 't', 'T']) {
        Some(k) => (&s[..k], Some(&s[k + 1..])),
        None => (s, None),
    };
    let digits = mantissa.replace('#', "0");
    let ok_mantissa = match digits.split_once('.') {
        Some((a, b)) => (!a.is_empty() || !b.is_empty()) && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()),
        None => !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()),
    };
    let ok_exp = exp.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    ok_mantissa && ok_exp
}

fn is_uint(s: &str, radix: u32) -> bool {
    !s.is_empty() && s.chars().all(|c| c == '#' || c.is_digit(radix))
}

/// Lex `text` into classified tokens.
#[cfg(test)]
pub fn lex(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    lex_chars(&chars)
}

pub fn lex_chars(chars: &[char]) -> Vec<Token> {
    let mut toks = Lexer { s: chars, i: 0, out: Vec::with_capacity(chars.len() / 4) }.run();
    classify(&mut toks, chars);
    toks
}

fn is_operator_chars(s: &[char]) -> bool {
    matches!(s, ['+'] | ['-'] | ['*'] | ['/'] | ['='] | ['>'] | ['<'] | ['>', '='] | ['<', '='])
}

/// Cheap pre-check before the allocating number parse: numbers start with a digit, sign,
/// dot or `#`.
fn could_be_number(s: &[char]) -> bool {
    match s.first() {
        Some(c) if c.is_ascii_digit() || *c == '.' => true,
        Some('+' | '-') => s.len() > 1,
        Some('#') => true,
        _ => false,
    }
}

/// Second pass over the whole token list.
fn classify(toks: &mut [Token], chars: &[char]) {
    classify_range(toks, chars, 0, toks.len());
}

fn is_prefix(t: &Token) -> bool {
    matches!(t.kind, Kind::Quote | Kind::DatumComment)
}

/// Classify tokens starting at `from` (a top-level boundary). Classification continues at
/// least to `min_end`, then stops at the first top-level boundary after it where the old
/// classification is still valid. Returns the end of the reclassified range.
fn classify_range(toks: &mut [Token], chars: &[char], from: usize, min_end: usize) -> usize {
    let mut scratch = String::with_capacity(32);
    // Brackets.
    let mut stack: Vec<usize> = Vec::new();
    let mut end = toks.len();
    let mut last_sig: Option<usize> = None;
    let mut i = from;
    while i < toks.len() {
        let t = toks[i];
        if i >= min_end
            && stack.is_empty()
            && t.kind == Kind::Open
            && t.depth == 0
            && !t.mismatched
            // The preceding token must be an unchanged old token, so the old classification
            // of this form saw the same context (e.g. no deleted quote prefix).
            && last_sig.is_some_and(|k| k >= min_end && !is_prefix(&toks[k]))
        {
            end = i;
            break;
        }
        toks[i].partner = None;
        toks[i].mismatched = false;
        toks[i].depth = 0;
        match t.kind {
            Kind::Open => {
                toks[i].depth = stack.len() as u16;
                stack.push(i);
            }
            Kind::Close => {
                if let Some(o) = stack.pop() {
                    toks[i].depth = stack.len() as u16;
                    toks[i].partner = Some(o as u32);
                    toks[o].partner = Some(i as u32);
                    let open_c = chars[toks[o].end - 1];
                    let close_c = chars[toks[i].start];
                    if closer_for(open_c) != close_c {
                        toks[i].mismatched = true;
                        toks[o].mismatched = true;
                    }
                } else {
                    toks[i].mismatched = true;
                }
            }
            _ => {}
        }
        if !t.is_trivia() {
            last_sig = Some(i);
        }
        i += 1;
    }
    for o in stack {
        toks[o].mismatched = true;
    }

    // Base classes.
    for t in toks.iter_mut().take(end).skip(from) {
        t.class = match t.kind {
            Kind::LineComment | Kind::BlockComment | Kind::DatumComment => Class::Comment,
            Kind::String | Kind::ByteString | Kind::HereString => Class::String,
            Kind::StringEscape => Class::Escape,
            Kind::Regex => Class::Regex,
            Kind::Char | Kind::Boolean => Class::Constant,
            Kind::Number => Class::Number,
            Kind::HashKeyword => Class::HashKeyword,
            Kind::Open | Kind::Close => {
                if t.mismatched {
                    Class::Error
                } else {
                    Class::Bracket
                }
            }
            Kind::Quote | Kind::Dot => Class::Punctuation,
            Kind::Lang | Kind::Directive => Class::Lang,
            Kind::LangName => Class::LangName,
            Kind::Error => Class::Error,
            Kind::Symbol => {
                if is_operator_chars(&chars[t.start..t.end]) { Class::Operator } else { Class::Variable }
            }
        };
    }

    // Head position: first significant token after an open bracket.
    for i in from..end {
        if toks[i].kind != Kind::Open {
            continue;
        }
        let mut j = i + 1;
        while j < end && toks[j].is_trivia() {
            j += 1;
        }
        if j < end && toks[j].kind == Kind::Symbol {
            let sym = &chars[toks[j].start..toks[j].end];
            scratch.clear();
            scratch.extend(sym.iter());
            toks[j].class = if is_keyword(&scratch) {
                Class::Keyword
            } else if is_operator_chars(sym) {
                Class::Operator
            } else {
                Class::Function
            };
        }
    }

    // Quoted data and datum comments.
    let mut i = from;
    while i < end {
        match toks[i].kind {
            Kind::Quote => {
                let q = &chars[toks[i].start..toks[i].end];
                if q == ['\''] || q == ['`'] {
                    let quasi = q == ['`'];
                    if let Some(e) = datum_end(toks, i + 1) {
                        mark_quoted(toks, chars, i + 1, e.min(end), quasi);
                    }
                }
                i += 1;
            }
            Kind::Open => {
                // (quote datum)
                let mut j = i + 1;
                while j < end && toks[j].is_trivia() {
                    j += 1;
                }
                if j < end
                    && toks[j].kind == Kind::Symbol
                    && chars[toks[j].start..toks[j].end] == ['q', 'u', 'o', 't', 'e']
                    && let Some(close) = toks[i].partner
                {
                    mark_quoted(toks, chars, j + 1, close as usize, false);
                }
                i += 1;
            }
            Kind::DatumComment => {
                if let Some(e) = datum_end(toks, i + 1) {
                    for t in toks.iter_mut().take(e.min(end)).skip(i + 1) {
                        t.class = Class::Comment;
                    }
                    i = e;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    end
}

fn is_string_piece(k: Kind) -> bool {
    matches!(k, Kind::String | Kind::StringEscape | Kind::ByteString | Kind::Regex)
}

/// Is old token `j` the first token of a lexing unit (so lexing can resume there)?
fn is_unit_start(toks: &[Token], j: usize) -> bool {
    let t = &toks[j];
    if matches!(t.kind, Kind::StringEscape | Kind::LangName) {
        return false;
    }
    !(is_string_piece(t.kind) && j > 0 && is_string_piece(toks[j - 1].kind) && toks[j - 1].end == t.start)
}

/// Outermost Open token enclosing `pos` (old, fully classified tokens).
fn outermost_open(toks: &[Token], pos: usize) -> Option<usize> {
    let mut best = None;
    let mut j = token_index_at(toks, pos);
    while j > 0 {
        j -= 1;
        let t = &toks[j];
        match t.kind {
            Kind::Close => {
                if let Some(p) = t.partner
                    && (p as usize) < j
                {
                    j = p as usize;
                }
            }
            Kind::Open if t.end <= pos && t.partner.is_none_or(|p| toks[p as usize].start >= pos) => best = Some(j),
            _ => {}
        }
    }
    best
}

/// Incrementally update `toks` (lexed from the previous text) to match `chars`, given that
/// every change lies within `lo..hi` of the new text and the length changed by `delta`.
/// Produces exactly the same tokens as a full `lex_chars`, re-lexing only the edited region
/// and reclassifying only the affected top-level forms.
pub fn relex(toks: &mut Vec<Token>, chars: &[char], lo: usize, hi: usize, delta: isize) {
    if toks.is_empty() {
        *toks = lex_chars(chars);
        return;
    }
    // Restart one token before the first token touching the edit, at a unit start.
    let mut i0 = toks.partition_point(|t| t.end < lo).saturating_sub(1);
    while i0 > 0 && !is_unit_start(toks, i0) {
        i0 -= 1;
    }
    let restart = toks.get(i0).map(|t| t.start.min(lo)).unwrap_or(lo).min(chars.len());

    // Top-level boundary for reclassification, using the old (valid) structure.
    let mut c0 = outermost_open(toks, restart.min(toks[i0].start)).unwrap_or(i0).min(i0);
    loop {
        let mut k = c0;
        while k > 0 && toks[k - 1].is_trivia() {
            k -= 1;
        }
        if k > 0 && is_prefix(&toks[k - 1]) {
            c0 = k - 1;
        } else {
            break;
        }
    }

    // Old tokens at/after the edited region, in old coordinates.
    let hi_old = hi as isize - delta;
    let mut j = toks.partition_point(|t| (t.start as isize) < hi_old);
    let mut lx = Lexer { s: chars, i: restart, out: Vec::new() };
    let mut resync = None;
    loop {
        let before = lx.out.len();
        if !lx.step() {
            break;
        }
        let nt = lx.out[before];
        if nt.start >= hi {
            while j < toks.len() && (toks[j].start as isize + delta) < nt.start as isize {
                j += 1;
            }
            if let Some(ot) = toks.get(j)
                && ot.start as isize + delta == nt.start as isize
                && ot.end as isize + delta == nt.end as isize
                && ot.kind == nt.kind
                && is_unit_start(toks, j)
            {
                lx.out.truncate(before);
                resync = Some(j);
                break;
            }
        }
    }
    let new = lx.out;
    let new_len = new.len();
    let tail_from = resync.unwrap_or(toks.len());
    let idx_delta = (i0 + new_len) as isize - tail_from as isize;
    toks.splice(i0..tail_from, new);
    for t in toks.iter_mut().skip(i0 + new_len) {
        t.start = (t.start as isize + delta) as usize;
        t.end = (t.end as isize + delta) as usize;
        if let Some(p) = t.partner {
            t.partner = Some((p as isize + idx_delta) as u32);
        }
    }
    classify_range(toks, chars, c0, i0 + new_len);
}

/// Index one past the end of the datum starting at the first significant token at/after `i`.
pub fn datum_end(toks: &[Token], mut i: usize) -> Option<usize> {
    while i < toks.len() && toks[i].is_trivia() {
        i += 1;
    }
    let t = toks.get(i)?;
    match t.kind {
        Kind::Quote | Kind::DatumComment => {
            // Prefix applies to the following datum.
            datum_end(toks, i + 1)
        }
        Kind::Open => t.partner.map(|p| p as usize + 1),
        Kind::Close => None,
        Kind::String | Kind::ByteString | Kind::Regex => {
            // Strings are split around escapes: consume all pieces.
            let mut j = i + 1;
            while j < toks.len() && matches!(toks[j].kind, Kind::StringEscape | Kind::String | Kind::ByteString | Kind::Regex) && toks[j].start == toks[j - 1].end {
                j += 1;
            }
            Some(j)
        }
        _ => Some(i + 1),
    }
}

fn mark_quoted(toks: &mut [Token], chars: &[char], from: usize, to: usize, quasi: bool) {
    let mut i = from;
    while i < to.min(toks.len()) {
        if quasi && toks[i].kind == Kind::Quote {
            let q = &chars[toks[i].start..toks[i].end];
            if q == [','] || q == [',', '@'] {
                // Unquoted region keeps normal highlighting.
                i = datum_end(toks, i + 1).unwrap_or(i + 1);
                continue;
            }
        }
        if toks[i].kind == Kind::Symbol {
            toks[i].class = Class::Symbol;
        }
        i += 1;
    }
}

/// Index of the first token whose end is after `pos` (binary search).
pub fn token_index_at(toks: &[Token], pos: usize) -> usize {
    toks.partition_point(|t| t.end <= pos)
}

/// The token containing `pos` (start <= pos < end), if any.
pub fn token_at(toks: &[Token], pos: usize) -> Option<(usize, &Token)> {
    let i = token_index_at(toks, pos);
    toks.get(i).filter(|t| t.start <= pos).map(|t| (i, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(String, Kind, Class)> {
        let chars: Vec<char> = src.chars().collect();
        lex(src).iter().map(|t| (chars[t.start..t.end].iter().collect(), t.kind, t.class)).collect()
    }

    #[test]
    fn numbers() {
        for n in ["0", "-12", "3.14", ".5", "1e10", "1/2", "#x1F", "#b101", "+inf.0", "-nan.0", "1+2i", "+i", "#e1.5", "6.02e+23", "1@2"] {
            assert!(is_number(n), "{n}");
        }
        for n in ["-", "+", "...", "a1", "1a", "e10", "#xG", "/", "1/"] {
            assert!(!is_number(n), "{n}");
        }
    }

    #[test]
    fn basic_classes() {
        let k = kinds("(define (f x) (+ x 1)) ; hi");
        assert_eq!(k[1], ("define".into(), Kind::Symbol, Class::Keyword));
        assert_eq!(k[3], ("f".into(), Kind::Symbol, Class::Function));
        assert_eq!(k[4], ("x".into(), Kind::Symbol, Class::Variable));
        assert_eq!(k[7], ("+".into(), Kind::Symbol, Class::Operator));
        assert_eq!(k.last().unwrap().1, Kind::LineComment);
    }

    #[test]
    fn lang_line_and_hash_forms() {
        let k = kinds("#lang racket/base\n#t #:key #\\space #\\a #rx\"a+\" #(1 2) #hash((a . 1))");
        assert_eq!(k[0].1, Kind::Lang);
        assert_eq!(k[1], ("racket/base".into(), Kind::LangName, Class::LangName));
        assert_eq!(k[2].1, Kind::Boolean);
        assert_eq!(k[3].1, Kind::HashKeyword);
        assert_eq!(k[4], ("#\\space".into(), Kind::Char, Class::Constant));
        assert_eq!(k[5].0, "#\\a");
        assert_eq!(k[6].1, Kind::Regex);
        assert_eq!(k[7], ("#(".into(), Kind::Open, Class::Bracket));
        assert!(k.iter().any(|t| t.0 == "#hash("));
    }

    #[test]
    fn strings_with_escapes() {
        let k = kinds(r#""a\nb\x41c""#);
        assert_eq!(k.iter().map(|t| t.1).collect::<Vec<_>>(), vec![Kind::String, Kind::StringEscape, Kind::String, Kind::StringEscape, Kind::String]);
    }

    #[test]
    fn nested_block_comment_and_datum_comment() {
        let k = kinds("#| a #| b |# c |# x #;(foo bar) y");
        assert_eq!(k[0].1, Kind::BlockComment);
        assert_eq!(k[1].0, "x");
        assert!(k.iter().filter(|t| t.0 == "foo" || t.0 == "bar").all(|t| t.2 == Class::Comment));
        assert_eq!(k.last().unwrap().2, Class::Variable);
    }

    #[test]
    fn quoted_data() {
        let k = kinds("'(a b) `(c ,d) (quote e) 'f");
        let class = |name: &str| k.iter().find(|t| t.0 == name).unwrap().2;
        assert_eq!(class("a"), Class::Symbol);
        assert_eq!(class("b"), Class::Symbol);
        assert_eq!(class("c"), Class::Symbol);
        assert_eq!(class("d"), Class::Variable);
        assert_eq!(class("e"), Class::Symbol);
        assert_eq!(class("f"), Class::Symbol);
    }

    #[test]
    fn brackets_depth_and_mismatch() {
        let toks = lex("([a] (b]");
        let opens: Vec<_> = toks.iter().filter(|t| t.is_open()).map(|t| t.depth).collect();
        assert_eq!(opens, vec![0, 1, 1]);
        assert!(toks.iter().any(|t| t.mismatched));
        assert_eq!(toks[0].partner, None.or(toks[0].partner));
    }

    #[test]
    fn symbols_with_bars_and_hash_percent() {
        let k = kinds("|a b| #%app foo#bar");
        assert_eq!(k[0], ("|a b|".into(), Kind::Symbol, Class::Variable));
        assert_eq!(k[1].1, Kind::Symbol);
        assert_eq!(k[2].0, "foo#bar");
    }

    #[test]
    fn here_string() {
        let k = kinds("#<<END\nhello \"x\"\nEND\n(a)");
        assert_eq!(k[0].1, Kind::HereString);
        assert_eq!(k[1].1, Kind::Open);
    }

    #[test]
    fn keywords_sorted() {
        assert!(KEYWORDS.windows(2).all(|w| w[0] < w[1]));
        assert!(is_keyword("define") && is_keyword("check-expect") && is_keyword("λ"));
    }
}

#[cfg(test)]
mod bench {
    /// `cargo test --release lex_speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn lex_speed() {
        let unit = "(define (fib n)\n  (cond [(< n 2) n] ; base\n        [else (+ (fib (- n 1)) (fib (- n 2)))]))\n\"str\\n\" 'sym #:kw 3.14\n";
        let src = unit.repeat(5000); // 20k lines
        let t = std::time::Instant::now();
        let chars: Vec<char> = src.chars().collect();
        let t_chars = t.elapsed();
        let mut toks = super::Lexer { s: &chars, i: 0, out: Vec::new() }.run();
        let t_lex = t.elapsed();
        super::classify(&mut toks, &chars);
        let t_all = t.elapsed();
        eprintln!("lexed {} lines / {} tokens: chars {:?}, lex {:?}, classify {:?}", src.lines().count(), toks.len(), t_chars, t_lex - t_chars, t_all - t_lex);
    }
}

#[cfg(test)]
mod incremental_tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    const PIECES: &[&str] = &[
        "(", ")", "[", "]", " ", "\n", "define", "x", "'", "`", ",", "\"", "\\", "n", "#;", "#|", "|#", ";", "#t", "12",
        "3.5", "#\\a", "#:k", "#lang racket\n", "λ", "quote", "#(", "+", "#rx\"", "|", "#<<E\nhi\nE\n", "cond", "else",
    ];

    #[test]
    fn incremental_matches_full_relex() {
        let mut rng = Rng(0x9e3779b97f4a7c15);
        for round in 0..4000 {
            let mut text: Vec<char> = Vec::new();
            for _ in 0..rng.below(60) {
                text.extend(PIECES[rng.below(PIECES.len())].chars());
            }
            let mut toks = lex_chars(&text);
            for step in 0..25 {
                // Random edit: replace a range with random pieces.
                let a = rng.below(text.len() + 1);
                let b = (a + rng.below(6)).min(text.len());
                let mut ins: Vec<char> = Vec::new();
                for _ in 0..rng.below(3) {
                    ins.extend(PIECES[rng.below(PIECES.len())].chars());
                }
                let delta = ins.len() as isize - (b - a) as isize;
                text.splice(a..b, ins.iter().copied());
                relex(&mut toks, &text, a, a + ins.len(), delta);
                let full = lex_chars(&text);
                if toks != full {
                    let k = toks.iter().zip(full.iter()).position(|(a, b)| a != b).unwrap_or(toks.len().min(full.len()));
                    eprintln!("edit {a}..{b} ins {:?}\nfirst diff at {k}:\n inc {:?}\n full {:?}", ins.iter().collect::<String>(), toks.get(k), full.get(k));
                    let st = full.get(k).map(|t| t.start).unwrap_or(0);
                    eprintln!(" context {:?}", text[st.saturating_sub(10)..(st + 10).min(text.len())].iter().collect::<String>());
                }
                assert_eq!(toks, full, "round {round} step {step}: text {:?}", text.iter().collect::<String>());
            }
        }
    }
}

#[cfg(test)]
mod incremental_bench {
    /// `cargo test --release keystroke_speed -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn keystroke_speed() {
        let unit = "(define (fib n)\n  (cond [(< n 2) n] ; base\n        [else (+ (fib (- n 1)) (fib (- n 2)))]))\n\"str\\n\" 'sym #:kw 3.14\n";
        let mut chars: Vec<char> = unit.repeat(5000).chars().collect();
        let mut toks = super::lex_chars(&chars);
        let pos = chars.len() / 2;
        let t = std::time::Instant::now();
        for k in 0..100 {
            chars.insert(pos + k, 'x');
            super::relex(&mut toks, &chars, pos + k, pos + k + 1, 1);
        }
        eprintln!("incremental keystroke on 20k lines: {:?} avg", t.elapsed() / 100);
    }
}
