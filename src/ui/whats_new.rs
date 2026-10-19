//! What's New: the release notes, in a tab of their own beside the project tabs. It opens by
//! itself on the first launch after an update, and from the What's New command.
//!
//! The notes are `release-notes/<version>.md` (the same text as the GitHub release), embedded
//! at build time. They use a little Markdown: `#`/`##` headings, paragraphs, `-` and `1.`
//! lists, `**bold**`, `` `code` `` and `[links](https://...)`.

use crate::app::App;
use crate::theme::with_alpha;
use crate::ui::{self, ic};
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, CornerRadius, FontId, Galley, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::ops::Range;
use std::sync::Arc;

include!(concat!(env!("OUT_DIR"), "/release_notes.rs"));

/// A laid-out block's links: where its galley is, the galley, and each link's char range and URL.
type Links = (egui::Pos2, Arc<Galley>, Vec<(Range<usize>, String)>);

/// The What's New tab: whether it's in the strip, and whether it's in front.
#[derive(Default)]
pub struct WhatsNew {
    pub open: bool,
    pub front: bool,
    /// Times it was opened: each opening starts at the top.
    opened: u32,
}

/// Every version's notes, newest first.
pub fn notes() -> Vec<(&'static str, &'static str)> {
    let mut v = RELEASE_NOTES.to_vec();
    v.sort_by(|a, b| {
        if crate::update::newer(a.0, b.0) {
            std::cmp::Ordering::Less
        } else if crate::update::newer(b.0, a.0) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    v
}

/// Whether this is the first launch of a newer version than the last one run, and notes that
/// this version has run. Call before anything writes the settings file.
///
/// A development build leaves the record alone (unless it runs with a test config): it would
/// otherwise use up the installed app's first launch of the version it's being built as.
pub fn first_run_after_update() -> bool {
    if crate::update::running_dev_build() && std::env::var_os("DRROCKET_CONFIG_DIR").is_none() {
        return false;
    }
    let dir = crate::config::config_dir();
    let marker = dir.join("last-version");
    let current = env!("CARGO_PKG_VERSION");
    let last = std::fs::read_to_string(&marker).ok().map(|s| s.trim().to_string());
    let had_settings = crate::config::Settings::path().exists();
    if last.as_deref() != Some(current) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&marker, current);
    }
    updated(last.as_deref(), had_settings, current)
}

/// `last`: the version recorded last time. Versions before 1.0.0-beta.10 didn't record one, so
/// someone with settings but no record is updating from one of those; without either it's a
/// fresh install, which gets the welcome screen instead.
fn updated(last: Option<&str>, had_settings: bool, current: &str) -> bool {
    match last {
        Some(last) => crate::update::newer(current, last),
        None => had_settings,
    }
}

impl App {
    /// Show the What's New tab in front.
    pub fn show_whats_new(&mut self) {
        let opened = self.whats_new.opened + 1;
        self.whats_new = WhatsNew { open: true, front: true, opened };
    }

    /// The What's New page, in place of the project's panels and editor.
    pub(crate) fn whats_new_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let current = env!("CARGO_PKG_VERSION");
        let pad = 32.0;
        let col_w = 680.0_f32.min(rect.width() - 2.0 * pad).max(200.0);
        let inner = rect.shrink2(vec2(0.0, 1.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        child.set_clip_rect(inner);
        egui::ScrollArea::vertical().id_salt(("whats-new", self.whats_new.opened)).auto_shrink(false).show(&mut child, |ui| {
            let left = inner.center().x - col_w / 2.0;
            let mut y = ui.max_rect().top() + (inner.height() * 0.08).clamp(28.0, 64.0);

            // Header: the logo, then the title and the version running now.
            let logo_h = 52.0;
            let logo = crate::ui::brand::logo(logo_h, theme.dark);
            let logo_w = logo.size().map(|s| s.x).unwrap_or(logo_h * 0.65);
            logo.paint_at(ui, Rect::from_min_size(pos2(left, y), vec2(logo_w, logo_h)));
            let tx = left + logo_w + 16.0;
            ui.painter().text(pos2(tx, y + 13.0), Align2::LEFT_CENTER, "What's New", ui::semibold(26.0), theme.text);
            ui.painter().text(pos2(tx, y + 39.0), Align2::LEFT_CENTER, format!("{} {current}", crate::ui::brand::APP_NAME), ui::ui_font(14.5), theme.text_muted);
            y += logo_h + 30.0;

            let mut links = Vec::new();
            for (k, (version, md)) in notes().into_iter().enumerate() {
                if k > 0 {
                    y += 14.0;
                    ui.painter().hline(left..=left + col_w, y, Stroke::new(1.0, tk.subtle_border));
                    y += 26.0;
                }
                // The version, with a pill on the one running now.
                let g = ui.painter().layout_no_wrap(version_title(version), ui::semibold(if k == 0 { 19.0 } else { 16.0 }), theme.text);
                let h = g.size().y;
                ui.painter().galley(pos2(left, y), g.clone(), theme.text);
                if version == current {
                    let t = ui.painter().layout_no_wrap("Installed".into(), ui::semibold(11.5), theme.text_accent);
                    let pill = Rect::from_min_size(pos2(left + g.size().x + 10.0, y + h / 2.0 - 10.0), vec2(t.size().x + 16.0, 20.0));
                    ui.painter().rect_filled(pill, CornerRadius::same(10), with_alpha(theme.text_accent, 30));
                    ui.painter().galley(pill.center() - t.size() / 2.0, t, theme.text_accent);
                }
                y += h + 14.0;
                y = self.markdown(ui, md, left, col_w, y, &mut links);
            }
            y += 48.0;
            ui.allocate_rect(Rect::from_min_max(pos2(left, ui.max_rect().top()), pos2(left + col_w, y)), Sense::hover());

            // Links: a pointing hand over them, and a click opens them.
            let pointer = ui.ctx().pointer_hover_pos().filter(|p| inner.contains(*p));
            if let Some(p) = pointer
                && let Some(url) = links.iter().find_map(|(at, galley, ranges): &Links| {
                    let r = Rect::from_min_size(*at, galley.size());
                    if !r.contains(p) {
                        return None;
                    }
                    let i = galley.cursor_from_pos(p - *at).index.0;
                    ranges.iter().find(|(range, _)| range.contains(&i)).map(|(_, u)| u.clone())
                })
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if ui.input(|i| i.pointer.primary_clicked()) {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            }
        });
    }

    /// Lay out and paint one version's notes from `y`; returns where they end. Links are added
    /// to `links` (galley position, galley, char ranges and URLs).
    fn markdown(&self, ui: &Ui, md: &str, left: f32, width: f32, mut y: f32, links: &mut Vec<Links>) -> f32 {
        let theme = &self.theme;
        let tk = self.tokens();
        let body = Style { font: ui::ui_font(14.5), bold: ui::semibold(14.5), code: ui::mono(13.0), color: theme.text, accent: theme.text_accent, code_bg: tk.hover, line: 23.0 };
        let mut first = true;
        for block in blocks(md) {
            let gap = match block {
                Block::Heading(..) if !first => 20.0,
                Block::Item(..) => 6.0,
                _ if first => 0.0,
                _ => 12.0,
            };
            y += gap;
            first = false;
            let (x, w, text, style) = match &block {
                Block::Heading(level, t) => {
                    let size = if *level == 1 { 16.5 } else { 15.0 };
                    (left, width, t, Style { font: ui::semibold(size), bold: ui::semibold(size), line: size * 1.5, ..body.clone() })
                }
                Block::Para(t) => (left, width, t, body.clone()),
                Block::Item(_, t) => (left + 22.0, width - 22.0, t, body.clone()),
            };
            let (job, ranges) = inline(text, &style, w);
            let galley = ui.painter().layout_job(job);
            let at = pos2(x, y);
            match &block {
                Block::Item(None, _) => {
                    ui.painter().circle_filled(pos2(left + 8.0, y + style.line / 2.0), 2.6, theme.text_muted);
                }
                Block::Item(Some(n), _) => {
                    ui.painter().text(pos2(left + 14.0, y + style.line / 2.0), Align2::RIGHT_CENTER, format!("{n}."), body.font.clone(), theme.text_muted);
                }
                _ => {}
            }
            ui.painter().galley(at, galley.clone(), Color32::PLACEHOLDER);
            y += galley.size().y;
            if !ranges.is_empty() {
                links.push((at, galley, ranges));
            }
        }
        y
    }

    /// The What's New tab in the title bar's tab strip (after the project tabs). Returns
    /// whether it was clicked, and whether its close button was.
    pub(crate) fn whats_new_tab(&self, ui: &Ui, r: Rect, hovered: bool, selected_bg: Color32, hover_bg: Color32) -> (bool, bool) {
        let theme = &self.theme;
        let resp = ui.interact(r, Id::unique("whats-new-tab"), Sense::click());
        let selected = self.whats_new.front;
        if selected {
            ui.painter().rect_filled(r, CornerRadius::same(7), selected_bg);
        } else if hovered {
            ui.painter().rect_filled(r, CornerRadius::same(7), hover_bg);
        }
        let close_r = Rect::from_center_size(pos2(r.left() + 15.0, r.center().y), vec2(18.0, 18.0));
        let close_resp = ui.interact(close_r, Id::unique("whats-new-tab-close"), Sense::click());
        let mut close = false;
        if hovered {
            if close_resp.hovered() {
                ui.painter().rect_filled(close_r, CornerRadius::same(4), with_alpha(theme.text, 22));
            }
            ui.painter().text(close_r.center(), Align2::CENTER_CENTER, ic::CLOSE, ui::icon_font(13.0), theme.text_muted);
            close = close_resp.clicked();
        }
        let color = if selected { theme.text } else { theme.text_muted };
        let label = ui.painter().layout_no_wrap("What's New".into(), ui::medium(13.0), color);
        let icon_w = 20.0;
        let max_w = r.width() - 2.0 * 30.0;
        if label.size().x + icon_w <= max_w {
            let x = r.center().x - (label.size().x + icon_w) / 2.0;
            ui.painter().text(pos2(x + 7.0, r.center().y), Align2::CENTER_CENTER, ic::AUTO_AWESOME, ui::icon_font(15.0), theme.text_accent);
            ui.painter().galley(pos2(x + icon_w, r.center().y - label.size().y / 2.0), label, color);
        } else {
            ui.painter().text(r.center(), Align2::CENTER_CENTER, ic::AUTO_AWESOME, ui::icon_font(15.0), theme.text_accent);
        }
        (resp.clicked() && !close, close || resp.middle_clicked())
    }
}

/// "1.0.0-beta.10" reads as "1.0.0 beta 10".
fn version_title(v: &str) -> String {
    v.replace(['-', '.'], " ").splitn(4, ' ').enumerate().fold(String::new(), |mut s, (k, part)| {
        s.push_str(match k {
            0 => "",
            1 | 2 => ".",
            _ => " ",
        });
        s.push_str(part);
        s
    })
}

#[derive(Debug, PartialEq)]
enum Block {
    /// `#` (1) or `##` (2).
    Heading(usize, String),
    Para(String),
    /// A list item: `-` (`None`) or numbered.
    Item(Option<u32>, String),
}

fn blocks(md: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    // Whether the last block can take a continuation line.
    let mut open = false;
    for line in md.lines() {
        let t = line.trim();
        if t.is_empty() {
            open = false;
            continue;
        }
        let numbered = t.split_once(". ").and_then(|(n, rest)| n.parse::<u32>().ok().map(|n| (n, rest)));
        if let Some(h) = t.strip_prefix("## ").or_else(|| t.strip_prefix("### ")) {
            out.push(Block::Heading(2, h.trim().into()));
            open = false;
        } else if let Some(h) = t.strip_prefix("# ") {
            out.push(Block::Heading(1, h.trim().into()));
            open = false;
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            out.push(Block::Item(None, item.into()));
            open = true;
        } else if let Some((n, item)) = numbered {
            out.push(Block::Item(Some(n), item.into()));
            open = true;
        } else if open && let Some(Block::Para(s) | Block::Item(_, s)) = out.last_mut() {
            s.push(' ');
            s.push_str(t);
        } else {
            out.push(Block::Para(t.into()));
            open = true;
        }
    }
    out
}

#[derive(Clone)]
struct Style {
    font: FontId,
    bold: FontId,
    code: FontId,
    color: Color32,
    accent: Color32,
    code_bg: Color32,
    line: f32,
}

/// One block's text with `**bold**`, `` `code` `` and `[links](url)`, wrapped at `width`.
/// Also returns the links' char ranges in the laid-out text.
fn inline(text: &str, s: &Style, width: f32) -> (LayoutJob, Vec<(Range<usize>, String)>) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let mut links = Vec::new();
    let mut bold = false;
    let mut chars = 0;
    let push = |job: &mut LayoutJob, t: &str, fmt: TextFormat| {
        if !t.is_empty() {
            job.append(t, 0.0, fmt);
        }
    };
    let fmt = |bold: bool, color: Color32| TextFormat { font_id: if bold { s.bold.clone() } else { s.font.clone() }, color, line_height: Some(s.line), valign: egui::Align::Center, ..Default::default() };
    let mut rest = text;
    while !rest.is_empty() {
        // The next marker: `**`, a backtick, or `[`.
        let next = ["**", "`", "["].iter().filter_map(|m| rest.find(m).map(|i| (i, *m))).min_by_key(|(i, _)| *i);
        let Some((i, marker)) = next else {
            push(&mut job, rest, fmt(bold, s.color));
            break;
        };
        push(&mut job, &rest[..i], fmt(bold, s.color));
        chars += rest[..i].chars().count();
        rest = &rest[i..];
        match marker {
            "**" => {
                bold = !bold;
                rest = &rest[2..];
            }
            "`" => match rest[1..].find('`') {
                Some(end) => {
                    let code = &rest[1..1 + end];
                    let f = TextFormat { font_id: s.code.clone(), color: s.color, background: s.code_bg, valign: egui::Align::Center, expand_bg: 1.0, ..Default::default() };
                    push(&mut job, code, f);
                    chars += code.chars().count();
                    rest = &rest[end + 2..];
                }
                None => {
                    push(&mut job, "`", fmt(bold, s.color));
                    chars += 1;
                    rest = &rest[1..];
                }
            },
            _ => {
                // `[text](url)`, or a plain bracket.
                let link = rest.find("](").and_then(|m| rest[m..].find(')').map(|e| (m, m + e)));
                match link {
                    Some((m, e)) if !rest[1..m].contains('[') => {
                        let label = &rest[1..m];
                        let url = &rest[m + 2..e];
                        let f = TextFormat { underline: Stroke::new(1.0, with_alpha(s.accent, 140)), ..fmt(bold, s.accent) };
                        push(&mut job, label, f);
                        let n = label.chars().count();
                        links.push((chars..chars + n, url.to_string()));
                        chars += n;
                        rest = &rest[e + 1..];
                    }
                    _ => {
                        push(&mut job, "[", fmt(bold, s.color));
                        chars += 1;
                        rest = &rest[1..];
                    }
                }
            }
        }
    }
    (job, links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_release_has_notes_and_they_sort_newest_first() {
        let notes = notes();
        // The version being built is the newest, and the rest follow in order.
        assert_eq!(notes[0].0, env!("CARGO_PKG_VERSION"));
        assert!(notes.windows(2).all(|w| crate::update::newer(w[0].0, w[1].0)));
        assert!(notes.iter().any(|(v, _)| *v == "1.0.0-beta.9"));
        assert!(notes.iter().all(|(_, md)| !md.trim().is_empty() && !md.contains('\u{2014}')));
    }

    #[test]
    fn shows_after_an_update_but_not_on_a_fresh_install() {
        assert!(updated(Some("1.0.0-beta.9"), true, "1.0.0-beta.10"));
        assert!(!updated(Some("1.0.0-beta.10"), true, "1.0.0-beta.10"));
        // Going back to an older version doesn't show it.
        assert!(!updated(Some("1.0.0-beta.11"), true, "1.0.0-beta.10"));
        // Updating from a version that didn't record itself.
        assert!(updated(None, true, "1.0.0-beta.10"));
        assert!(!updated(None, false, "1.0.0-beta.10"));
    }

    #[test]
    fn reads_the_markdown_used_in_the_notes() {
        let md = "Intro line\ncontinued.\n\n## What to try\n\n- One\n  more\n- Two\n1. First\n\n**Bold.** Para";
        assert_eq!(
            blocks(md),
            vec![
                Block::Para("Intro line continued.".into()),
                Block::Heading(2, "What to try".into()),
                Block::Item(None, "One more".into()),
                Block::Item(None, "Two".into()),
                Block::Item(Some(1), "First".into()),
                Block::Para("**Bold.** Para".into()),
            ]
        );
        let s = Style { font: ui::ui_font(14.0), bold: ui::semibold(14.0), code: ui::mono(13.0), color: Color32::WHITE, accent: Color32::BLUE, code_bg: Color32::BLACK, line: 20.0 };
        let (job, links) = inline("**Get** `racket` from [racket-lang.org](https://racket-lang.org) [x] now", &s, 400.0);
        assert_eq!(job.text, "Get racket from racket-lang.org [x] now");
        assert_eq!(links, vec![(16..31, "https://racket-lang.org".to_string())]);
        assert_eq!(version_title("1.0.0-beta.10"), "1.0.0 beta 10");
        assert_eq!(version_title("1.2.0"), "1.2.0");
    }
}
