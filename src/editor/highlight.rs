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

/// Images by placeholder character (see `editor::images`).
pub type ImageMap = std::collections::HashMap<char, super::images::EmbeddedImage>;

/// Decode a backend's image map.
pub fn decode_images(images: &crate::racket::protocol::SourceImages) -> ImageMap {
    let mut store = super::images::ImageStore::default();
    store.load(images);
    images.keys().filter_map(|k| k.parse::<u32>().ok().and_then(char::from_u32)).filter_map(|c| store.get(c).map(|i| (c, i.clone()))).collect()
}

fn thumb_size(img: &super::images::EmbeddedImage, h: f32) -> egui::Vec2 {
    egui::vec2((h * img.width as f32 / img.height.max(1) as f32).clamp(h * 0.4, h * 8.0), h)
}

/// Show image placeholder characters in `job` as spaces wide enough for their thumbnails
/// (one character stays one glyph, so ranges and cursors still line up).
pub fn widen_image_chars(job: &mut LayoutJob, images: &ImageMap, thumb_h: f32) {
    if images.is_empty() || !job.text.chars().any(|c| images.contains_key(&c)) {
        return;
    }
    let old = std::mem::take(job);
    *job = LayoutJob { wrap: old.wrap.clone(), halign: old.halign, justify: old.justify, ..Default::default() };
    for section in &old.sections {
        let text = &old.text[section.byte_range.start.0..section.byte_range.end.0];
        let mut run = String::new();
        for c in text.chars() {
            match images.get(&c) {
                Some(img) => {
                    if !run.is_empty() {
                        job.append(&std::mem::take(&mut run), section.leading_space, section.format.clone());
                    }
                    let space_w = section.format.font_id.size * 0.6;
                    let w = thumb_size(img, thumb_h).x + 4.0;
                    // The row grows to fit the thumbnail.
                    job.append(" ", 0.0, TextFormat { extra_letter_spacing: (w - space_w).max(0.0), line_height: Some(thumb_h + 4.0), valign: egui::Align::Center, ..section.format.clone() });
                }
                None => run.push(c),
            }
        }
        if !run.is_empty() {
            job.append(&run, 0.0, section.format.clone());
        }
    }
}

/// Paint the thumbnails for a galley laid out from a job passed through [`widen_image_chars`];
/// `text` is the original text (with the placeholder characters). Hover shows full size.
pub fn paint_galley_images(ui: &egui::Ui, galley: &egui::Galley, origin: egui::Pos2, text: &str, images: &ImageMap, thumb_h: f32) {
    if images.is_empty() {
        return;
    }
    let pointer = ui.input(|i| i.pointer.hover_pos());
    for (k, c) in text.chars().enumerate() {
        let Some(img) = images.get(&c) else { continue };
        let next = galley.pos_from_cursor(egui::text::CCursor::new(k + 1));
        let size = thumb_size(img, thumb_h);
        let r = egui::Rect::from_min_size(egui::pos2(origin.x + next.min.x - size.x - 2.0, origin.y + next.center().y - size.y / 2.0), size);
        let uri = format!("bytes://embedded/{:p}.png", std::sync::Arc::as_ptr(&img.png));
        egui::Image::from_bytes(uri.clone(), egui::load::Bytes::Shared(img.png.clone())).corner_radius(2).paint_at(ui, r);
        if pointer.is_some_and(|p| r.contains(p)) {
            let scale = (320.0 / img.width.max(img.height).max(1) as f32).min(2.0);
            let full = egui::vec2(img.width as f32, img.height as f32) * scale;
            egui::Area::new(egui::Id::unique(("image-preview", std::sync::Arc::as_ptr(&img.png) as *const u8 as usize, k))).order(egui::Order::Tooltip).fixed_pos(r.left_bottom() + egui::vec2(0.0, 6.0)).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    ui.add(egui::Image::from_bytes(uri, egui::load::Bytes::Shared(img.png.clone())).fit_to_exact_size(full));
                });
            });
        }
    }
}
