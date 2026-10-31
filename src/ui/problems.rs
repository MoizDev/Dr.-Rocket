//! The problems counter in the editor's top-right corner and its hover list, plus the rows the
//! Problems panel shares with it. Clicking the counter lists every problem in the file; clicking
//! a row goes to it. The list opens only on a click, so it never covers code the pointer is
//! just passing over.

use crate::app::{App, Dock, PanelItem};
use crate::editor::view::{EditorView, InlayKind, Severity};
use crate::theme::Theme;
use crate::ui::widgets::truncated_label;
use crate::ui::{self, ic};
use egui::{Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    Error,
    Warning,
    FailedTest,
}

impl ProblemKind {
    pub fn glyph(self) -> &'static str {
        match self {
            ProblemKind::Error => ic::ERROR,
            ProblemKind::Warning => ic::WARNING,
            ProblemKind::FailedTest => ic::CANCEL,
        }
    }

    pub fn color(self, theme: &Theme) -> Color32 {
        match self {
            ProblemKind::Warning => theme.warning,
            ProblemKind::Error | ProblemKind::FailedTest => theme.error,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Problem {
    pub kind: ProblemKind,
    pub range: Range<usize>,
    /// Zero-based.
    pub line: usize,
    pub message: String,
}

/// The errors, warnings and failed tests shown in an editor, in file order.
pub fn problems(editor: &EditorView) -> Vec<Problem> {
    let buffer = &editor.doc.buffer;
    let diagnostics = editor.deco.shown_diagnostics().map(|d| {
        let kind = match d.severity {
            Severity::Error => ProblemKind::Error,
            Severity::Warning => ProblemKind::Warning,
        };
        (kind, d.range.clone(), d.message.as_str())
    });
    let failed = editor.deco.inlays.iter().filter(|i| i.kind == InlayKind::Fail).map(|i| (ProblemKind::FailedTest, i.range.clone(), i.text.as_str()));
    let mut out: Vec<Problem> = diagnostics
        .chain(failed)
        .map(|(kind, range, message)| Problem { kind, line: buffer.line_of(range.start), range, message: message.lines().next().unwrap_or("").trim().to_string() })
        .collect();
    out.sort_by_key(|p| (p.range.start, p.kind as u8));
    out
}

/// One problem: its icon, the first line of its message, and where it is on the right.
pub fn paint_row(ui: &Ui, theme: &Theme, r: Rect, p: &Problem, location: &str) {
    let painter = ui.painter();
    painter.text(pos2(r.left() + 14.0, r.center().y), Align2::CENTER_CENTER, p.kind.glyph(), ui::icon_font_filled(16.0), p.kind.color(theme));
    let loc_w = painter.layout_no_wrap(location.to_string(), ui::ui_font(12.5), theme.text_muted).size().x;
    truncated_label(ui, pos2(r.left() + 30.0, r.center().y), &p.message, ui::ui_font(13.5), theme.text, r.width() - loc_w - 50.0);
    painter.text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, location, ui::ui_font(12.5), theme.text_muted);
}

/// "2 errors, 1 warning".
pub fn summary(problems: &[Problem]) -> String {
    let count = |k| problems.iter().filter(|p| p.kind == k).count();
    let mut parts = Vec::new();
    for (kind, one, many) in [(ProblemKind::Error, "error", "errors"), (ProblemKind::Warning, "warning", "warnings"), (ProblemKind::FailedTest, "failed test", "failed tests")] {
        match count(kind) {
            0 => {}
            1 => parts.push(format!("1 {one}")),
            n => parts.push(format!("{n} {many}")),
        }
    }
    parts.join(", ")
}

const ROW_H: f32 = 28.0;
/// Rows shown before the list scrolls.
const MAX_ROWS: usize = 8;

/// State of the list, kept in egui's memory between frames.
#[derive(Clone, Copy, Default)]
struct PopupState {
    open: bool,
    /// Where the list was drawn last frame, so a click elsewhere closes it.
    rect: Option<Rect>,
}

impl App {
    /// Error / warning / test counters in the editor's top-right corner.
    pub(crate) fn problems_counter(&mut self, ui: &mut Ui, ed_rect: Rect) {
        let state_id = Id::unique("problems-popup");
        let Some(tab) = self.active_tab() else { return };
        let list = problems(&tab.editor);
        let passed = tab.editor.deco.inlays.iter().filter(|i| i.kind == InlayKind::Pass).count();
        if list.is_empty() && passed == 0 || tab.editor.find.open {
            ui.data_mut(|d| d.remove::<PopupState>(state_id));
            return;
        }
        let theme = self.theme.clone();
        let tk = self.tokens();

        // The counter: one icon and number per kind.
        let count = |k| list.iter().filter(|p| p.kind == k).count();
        let mut items: Vec<(&str, Color32, usize)> = [ProblemKind::Error, ProblemKind::Warning, ProblemKind::FailedTest]
            .into_iter()
            .map(|k| (k.glyph(), k.color(&theme), count(k)))
            .filter(|&(_, _, n)| n > 0)
            .collect();
        if passed > 0 {
            items.push((ic::DONE_ALL, theme.success, passed));
        }
        let font = ui::medium(13.0);
        let galleys: Vec<_> = items.iter().map(|(_, c, n)| ui.painter().layout_no_wrap(n.to_string(), font.clone(), *c)).collect();
        let w = 10.0 + galleys.iter().map(|g| 18.0 + g.size().x + 10.0).sum::<f32>();
        let counter = Rect::from_min_size(pos2(ed_rect.right() - w - 22.0, ed_rect.top() + 8.0), vec2(w, 26.0));
        let resp = ui.interact(counter, Id::unique("diag-counter"), Sense::click());

        let mut st: PopupState = ui.data(|d| d.get_temp(state_id)).unwrap_or_default();
        // A press anywhere but the counter and the list closes it (the click still does its
        // own thing, like placing the caret).
        let pressed_outside = ui.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pos, pressed: true, .. } if !counter.contains(*pos) && !st.rect.is_some_and(|r| r.contains(*pos))))
        });
        if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) || list.is_empty() {
            st.open = false;
        }

        let fill = if resp.hovered() || st.open { tk.hover } else { crate::theme::with_alpha(tk.popup, 235) };
        ui.painter().rect(counter, CornerRadius::same(7), fill, Stroke::new(1.0, tk.island_border), StrokeKind::Inside);
        let mut x = counter.left() + 8.0;
        for ((glyph, c, _), g) in items.iter().zip(galleys) {
            ui.painter().text(pos2(x + 8.0, counter.center().y), Align2::CENTER_CENTER, *glyph, ui::icon_font_filled(16.0), *c);
            x += 18.0;
            ui.painter().galley(pos2(x, counter.center().y - g.size().y / 2.0), g.clone(), *c);
            x += g.size().x + 10.0;
        }
        if resp.clicked() && !list.is_empty() {
            st.open = !st.open;
        }
        if list.is_empty() {
            resp.on_hover_text(format!("{passed} tests passed"));
        } else if !st.open {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(summary(&list));
        }

        let mut go_to = None;
        let mut open_panel = false;
        st.rect = None;
        if st.open {
            let width = 440.0_f32.min(ed_rect.width() - 24.0);
            let area = egui::Area::new(Id::unique("problems-list")).order(egui::Order::Foreground).pivot(Align2::RIGHT_TOP).fixed_pos(pos2(counter.right(), counter.bottom() + 4.0));
            let shown = area.show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (head, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
                    ui.painter().text(pos2(head.left() + 8.0, head.center().y), Align2::LEFT_CENTER, summary(&list), ui::medium(12.5), theme.text_muted);
                    egui::ScrollArea::vertical().id_salt("problems-list-scroll").max_height(MAX_ROWS as f32 * ROW_H).auto_shrink([false, true]).show(ui, |ui| {
                        for (k, p) in list.iter().enumerate() {
                            let (r, row) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                            if row.hovered() {
                                ui.painter().rect_filled(r, CornerRadius::same(5), tk.hover);
                            }
                            paint_row(ui, &theme, r, p, &format!("Ln {}", p.line + 1));
                            if row.clicked() {
                                go_to = Some(k);
                            }
                        }
                    });
                    ui.add_space(3.0);
                    let sep = ui.cursor().top();
                    ui.painter().hline(head.left() + 4.0..=head.right() - 4.0, sep, Stroke::new(1.0, tk.subtle_border));
                    ui.add_space(3.0);
                    let (r, row) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                    let c = if row.hovered() { theme.text } else { theme.text_muted };
                    if row.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(5), tk.hover);
                    }
                    ui.painter().text(pos2(r.left() + 14.0, r.center().y), Align2::CENTER_CENTER, ic::BOTTOM_PANEL_OPEN, ui::icon_font(16.0), c);
                    ui.painter().text(pos2(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, "Open Problems panel", ui::ui_font(13.5), c);
                    open_panel = row.clicked();
                });
            });
            st.rect = Some(shown.response.rect);
        }
        if let Some(k) = go_to {
            st.open = false;
            if let Some(t) = self.active_tab_mut() {
                t.editor.reveal(list[k].range.clone());
                t.editor.request_focus();
            }
        }
        if open_panel {
            st.open = false;
            self.layout.show(PanelItem::Problems, Dock::Bottom);
        }
        ui.data_mut(|d| d.insert_temp(state_id, st));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::view::{Diagnostic, Inlay};

    #[test]
    fn problems_are_in_file_order_with_failed_tests() {
        let mut ed = EditorView::new("(define x 1)\n(check-expect x 2)\n(f y)\n", Id::unique("t"));
        ed.deco.diagnostics = vec![
            Diagnostic::new(33..34, Severity::Error, "y: unbound identifier\n  in: y", true),
            Diagnostic::new(1..7, Severity::Warning, "unused", true),
        ];
        ed.deco.inlays = vec![
            Inlay { range: 13..31, text: "Actual value 1 differs from 2".into(), kind: InlayKind::Fail },
            Inlay { range: 0..12, text: "ok".into(), kind: InlayKind::Pass },
        ];
        let ps = problems(&ed);
        let got: Vec<_> = ps.iter().map(|p| (p.kind, p.line, p.message.as_str())).collect();
        assert_eq!(
            got,
            [
                (ProblemKind::Warning, 0, "unused"),
                (ProblemKind::FailedTest, 1, "Actual value 1 differs from 2"),
                (ProblemKind::Error, 2, "y: unbound identifier"),
            ]
        );
        assert_eq!(summary(&ps), "1 error, 1 warning, 1 failed test");
    }

    #[test]
    fn an_error_from_both_the_run_and_the_check_counts_once() {
        let mut ed = EditorView::new("(f Z)\n", Id::unique("t"));
        ed.deco.diagnostics = vec![
            Diagnostic::new(3..4, Severity::Error, "Z: this variable is not defined", true),
            Diagnostic::new(3..4, Severity::Error, "Z: this variable is not defined\n  in: Z", false),
        ];
        assert_eq!(summary(&problems(&ed)), "1 error");
    }
}
