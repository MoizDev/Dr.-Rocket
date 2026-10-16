//! Print: the file as syntax-coloured HTML (in a light theme, for paper), opened in the
//! browser with its print dialog up.

use crate::editor::highlight::class_highlight;
use crate::editor::lexer::Class;
use crate::theme::Theme;
use egui::Color32;

fn css(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A program (without DrRacket's hidden header) as a standalone HTML page.
pub fn html(title: &str, doc: &mut crate::editor::document::Document, theme: &Theme) -> String {
    let text = doc.text();
    let chars: Vec<char> = text.chars().collect();
    let toks = doc.tokens().to_vec();
    let mut body = String::new();
    let mut at = 0;
    let span = |out: &mut String, from: usize, to: usize, color: Option<Color32>, italic: bool| {
        let s: String = chars[from..to]
            .iter()
            .map(|&c| match doc.images.get(c).filter(|_| crate::editor::images::is_image_char(c)) {
                Some(img) => format!("<img src=\"data:image/png;base64,{}\" style=\"vertical-align:middle;max-height:8em\">", img.b64),
                None => escape(&c.to_string()),
            })
            .collect();
        match color {
            Some(c) => out.push_str(&format!("<span style=\"color:{}{}\">{}</span>", css(c), if italic { ";font-style:italic" } else { "" }, s)),
            None => out.push_str(&s),
        }
    };
    for t in &toks {
        if t.start > at {
            span(&mut body, at, t.start, None, false);
        }
        let h = if t.class == Class::Plain { None } else { Some(class_highlight(t.class, t.depth, theme, true)) };
        span(&mut body, t.start, t.end, h.map(|h| h.color), h.is_some_and(|h| h.italic));
        at = t.end;
    }
    if at < chars.len() {
        span(&mut body, at, chars.len(), None, false);
    }
    let lines = body.split('\n').enumerate().map(|(k, l)| format!("<tr><td class=n>{}</td><td>{}</td></tr>", k + 1, if l.is_empty() { "&nbsp;" } else { l })).collect::<String>();
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>\
         body{{font:11pt 'Fira Code',Menlo,monospace;color:{fg};background:#fff;margin:2em}}\
         h1{{font:600 12pt -apple-system,sans-serif;margin:0 0 1em}}\
         table{{border-collapse:collapse}} td{{white-space:pre;vertical-align:top;padding:0}}\
         td.n{{color:#aaa;text-align:right;padding-right:1.2em;user-select:none}}\
         </style></head><body><h1>{title}</h1><table>{lines}</table>\
         <script>window.onload=()=>window.print()</script></body></html>",
        title = escape(title),
        fg = css(theme.editor_fg),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::document::Document;
    use crate::editor::images::EmbeddedImage;

    #[test]
    fn prints_coloured_numbered_lines_with_images() {
        let themes = crate::theme::ThemeRegistry::load();
        let theme = themes.get_or_default("One Light");
        let mut doc = Document::new("");
        let pic = doc.images.add(EmbeddedImage::from_rgba(image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]))).unwrap());
        doc.buffer.set_text(&format!("(define (f x) (< x 1))\n(define p {pic})"));
        let page = html("a<b>.rkt", &mut doc, &theme);
        assert!(page.contains("<title>a&lt;b&gt;.rkt</title>"));
        assert!(page.contains("<td class=n>2</td>"));
        assert!(page.contains("&lt;"), "code is escaped");
        assert!(page.contains(&format!("color:{}", css(theme.syntax.keyword.color))), "keywords are coloured");
        assert!(page.contains("data:image/png;base64,"), "images are embedded");
        assert!(page.contains("window.print()"));
    }
}
