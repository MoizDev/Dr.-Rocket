//! Syntax-highlighted layout jobs for read-only Racket text (REPL transcript, stepper,
//! macro stepper), with optional highlighted ranges drawn as backgrounds.

use super::lexer::{self, Class};
use crate::theme::{Highlight, Theme};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

pub fn class_highlight(class: Class, depth: u16, theme: &Theme, rainbow: bool) -> Highlight {
    let s = &theme.syntax;
    let plain = Highlight { color: theme.editor_fg, italic: false, bold: false };
    match class {
        Class::Plain => plain,
        Class::Comment => s.comment,
        Class::String => s.string,
        Class::Escape => s.string_escape,
        Class::Regex => s.regex,
        Class::Number => s.number,
        Class::Constant => s.constant,
        Class::HashKeyword => s.hash_keyword,
        Class::Symbol => s.symbol,
        Class::Keyword => s.keyword,
        Class::Function => s.function,
        Class::Operator => s.operator,
        Class::Variable => s.variable,
        Class::Bracket => {
            if rainbow {
                Highlight { color: theme.rainbow(depth as usize), italic: false, bold: false }
            } else {
                s.punctuation
            }
        }
        Class::Punctuation => s.punctuation,
        Class::Lang => s.keyword,
        Class::LangName => s.lang,
        Class::Error => Highlight { color: theme.error, italic: false, bold: false },
    }
}

/// Highlight `text` as Racket code. `marks` are char ranges painted with `mark_bg`.
pub fn racket_job(text: &str, theme: &Theme, font: &FontId, marks: &[(usize, usize)], mark_bg: Color32) -> LayoutJob {
    let chars: Vec<char> = text.chars().collect();
    let toks = lexer::lex_chars(&chars);
    let mut job = LayoutJob::default();
    let in_mark = |i: usize| marks.iter().any(|&(s, e)| s <= i && i < e);
    // Split at token and mark boundaries; emit runs of uniform style.
    let mut bounds: Vec<usize> = vec![0, chars.len()];
    for t in &toks {
        bounds.push(t.start);
        bounds.push(t.end);
    }
    for &(s, e) in marks {
        bounds.push(s.min(chars.len()));
        bounds.push(e.min(chars.len()));
    }
    bounds.sort_unstable();
    bounds.dedup();
    let plain = Highlight { color: theme.editor_fg, italic: false, bold: false };
    for w in bounds.windows(2) {
        let (s, e) = (w[0], w[1]);
        if s >= e {
            continue;
        }
        let h = lexer::token_at(&toks, s).filter(|(_, t)| t.start <= s && e <= t.end).map(|(_, t)| class_highlight(t.class, t.depth, theme, true)).unwrap_or(plain);
        let piece: String = chars[s..e].iter().collect();
        let background = if in_mark(s) { mark_bg } else { Color32::TRANSPARENT };
        job.append(&piece, 0.0, TextFormat { font_id: font.clone(), color: h.color, italics: h.italic, background, ..Default::default() });
    }
    if job.sections.is_empty() {
        job.append("", 0.0, TextFormat { font_id: font.clone(), color: theme.editor_fg, ..Default::default() });
    }
    job
}
