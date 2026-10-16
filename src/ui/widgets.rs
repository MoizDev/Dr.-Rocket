//! Shared custom-painted widgets in the islands style.

use super::{Tokens, icon_font, icon_font_filled, medium, ui_font};
use crate::theme::{Theme, with_alpha};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

/// Paint a Material Symbols glyph centered in `rect`.
pub fn paint_icon(ui: &Ui, rect: Rect, glyph: &str, size: f32, color: Color32) {
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, icon_font(size), color);
}

pub fn paint_icon_filled(ui: &Ui, rect: Rect, glyph: &str, size: f32, color: Color32) {
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, icon_font_filled(size), color);
}

/// A square ghost button with a Material icon. Hover shows a soft rounded background.
pub fn icon_button(ui: &mut Ui, theme: &Theme, glyph: &str, tooltip: &str) -> Response {
    icon_button_sized(ui, theme, glyph, tooltip, 28.0, 18.0, None, false)
}

#[allow(clippy::too_many_arguments)]
pub fn icon_button_sized(ui: &mut Ui, theme: &Theme, glyph: &str, tooltip: &str, box_size: f32, icon_size: f32, color: Option<Color32>, active: bool) -> Response {
    let tk = Tokens::of(theme);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    let hovered = resp.hovered();
    let pressed = resp.is_pointer_button_down_on();
    if ui.is_rect_visible(rect) {
        let bg = if pressed {
            tk.pressed
        } else if active {
            tk.selected
        } else if hovered {
            tk.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
        }
        let c = color.unwrap_or(if hovered || active { theme.text } else { theme.icon });
        paint_icon(ui, rect, glyph, icon_size, c);
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) }
}

/// A rounded "pill" button with optional leading icon/color dot, label, and dropdown chevron
/// (like the project and run-configuration selectors in the title bar).
pub fn pill_button(ui: &mut Ui, theme: &Theme, leading: Option<(&str, Color32)>, label: &str, chevron: bool, framed: bool) -> Response {
    let tk = Tokens::of(theme);
    let font = medium(ui.style().text_styles[&egui::TextStyle::Body].size);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, theme.text);
    let icon_w = if leading.is_some() { 22.0 } else { 0.0 };
    let chev_w = if chevron { 20.0 } else { 0.0 };
    let pad = 10.0;
    let size = vec2(pad + icon_w + galley.size().x + chev_w + pad - if chevron { 4.0 } else { 0.0 }, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        let bg = if resp.is_pointer_button_down_on() {
            tk.pressed
        } else if hovered {
            tk.hover
        } else if framed {
            with_alpha(tk.hover, 120)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect(rect, CornerRadius::same(8), bg, if framed { Stroke::new(1.0, tk.island_border) } else { Stroke::NONE }, StrokeKind::Inside);
        let mut x = rect.left() + pad;
        if let Some((glyph, color)) = leading {
            let r = Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(18.0));
            paint_icon_filled(ui, r, glyph, 17.0, color);
            x += icon_w;
        }
        ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley.clone(), theme.text);
        x += galley.size().x;
        if chevron {
            let r = Rect::from_min_size(pos2(x + 2.0, rect.top()), vec2(16.0, rect.height()));
            paint_icon(ui, r, super::ic::EXPAND_MORE, 18.0, theme.text_muted);
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub struct TabResponse {
    pub clicked: bool,
    pub close: bool,
    pub response: Response,
}

pub enum TabIcon<'a> {
    Image(egui::Image<'a>),
    Glyph(&'a str, Color32),
}

/// An editor/panel tab: icon, label, dirty dot or close button.
#[allow(clippy::too_many_arguments)]
pub fn tab(ui: &mut Ui, theme: &Theme, icon: TabIcon<'_>, label: &str, active: bool, dirty: bool, closable: bool, italic: bool) -> TabResponse {
    let tk = Tokens::of(theme);
    let size = ui.style().text_styles[&egui::TextStyle::Body].size;
    let font = if active { medium(size) } else { ui_font(size) };
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_string(), font, if active { theme.text } else { theme.text_muted });
    if italic {
        job.sections[0].format.italics = true;
    }
    let galley = ui.painter().layout_job(job);
    let icon_w = 22.0;
    let close_w = if closable || dirty { 22.0 } else { 0.0 };
    let h = super::TAB_BAR_H - 8.0;
    let w = 10.0 + icon_w + galley.size().x + 6.0 + close_w + 6.0;
    // Tabs can be dragged to reorder them.
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click_and_drag());
    let mut out = TabResponse { clicked: resp.clicked(), close: resp.middle_clicked(), response: resp.clone() };
    if !ui.is_rect_visible(rect) {
        return out;
    }
    let hovered = resp.hovered();
    if active {
        ui.painter().rect_filled(rect, CornerRadius::same(8), tk.selected);
    } else if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(8), tk.hover);
    }
    let mut x = rect.left() + 10.0;
    match icon {
        TabIcon::Image(img) => {
            let r = Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0));
            img.paint_at(ui, r);
        }
        TabIcon::Glyph(g, c) => {
            let r = Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(18.0));
            paint_icon(ui, r, g, 17.0, c);
        }
    }
    x += icon_w;
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley.clone(), theme.text);
    x += galley.size().x + 6.0;
    if closable || dirty {
        let cr = Rect::from_center_size(pos2(x + 9.0, rect.center().y), Vec2::splat(18.0));
        let close_resp = ui.interact(cr, resp.id.with("close"), Sense::click());
        let show_close = closable && (hovered || active || close_resp.hovered());
        if dirty && !close_resp.hovered() && !(hovered && closable) {
            ui.painter().circle_filled(cr.center(), 4.0, theme.text_muted);
        } else if show_close {
            if close_resp.hovered() {
                ui.painter().rect_filled(cr, CornerRadius::same(4), tk.pressed);
            }
            paint_icon(ui, cr, super::ic::CLOSE, 15.0, if close_resp.hovered() { theme.text } else { theme.text_muted });
        }
        if close_resp.clicked() {
            out.close = true;
            out.clicked = false;
        }
    }
    out
}

/// Single-line search/text field with a leading icon, in a rounded frame.
pub fn search_field(ui: &mut Ui, theme: &Theme, text: &mut String, hint: &str, id: egui::Id, width: f32) -> Response {
    let tk = Tokens::of(theme);
    let h = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
    let focused = ui.memory(|m| m.has_focus(id));
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        tk.hover,
        Stroke::new(1.0, if focused { with_alpha(theme.border_focused, 200) } else { tk.island_border }),
        StrokeKind::Inside,
    );
    paint_icon(ui, Rect::from_min_size(rect.min + vec2(4.0, 0.0), vec2(24.0, h)), super::ic::SEARCH, 17.0, theme.text_placeholder);
    let inner = Rect::from_min_max(rect.min + vec2(28.0, 0.0), rect.max - vec2(8.0, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let te = egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(egui::RichText::new(hint).color(theme.text_placeholder))
        .frame(egui::Frame::NONE)
        .desired_width(inner.width())
        .min_size(vec2(inner.width(), h))
        .align(egui::Align2::LEFT_CENTER)
        .margin(egui::Margin::ZERO)
        .font(egui::TextStyle::Body);
    child.add(te)
}

/// Draw a label clipped with an ellipsis to `max_w`.
pub fn truncated_label(ui: &Ui, pos: Pos2, text: &str, font: FontId, color: Color32, max_w: f32) -> f32 {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
    let g = ui.painter().layout_job(job);
    let w = g.size().x;
    ui.painter().galley(pos2(pos.x, pos.y - g.size().y / 2.0), g, color);
    w
}

/// Empty-state message with a big muted icon.
pub fn empty_state(ui: &mut Ui, theme: &Theme, glyph: &str, title: &str, subtitle: &str) {
    let rect = ui.available_rect_before_wrap();
    // The subtitle wraps to the panel; extra lines push the icon and title up, so whatever a
    // caller puts below (at a fixed offset) stays clear.
    let sub = (!subtitle.is_empty()).then(|| {
        let mut job = egui::text::LayoutJob::simple(subtitle.to_string(), ui_font(13.0), theme.text_placeholder, (rect.width() - 40.0).max(80.0));
        job.halign = egui::Align::Center;
        ui.painter().layout_job(job)
    });
    let extra = sub.as_ref().map(|g| (g.size().y - 17.0).max(0.0)).unwrap_or(0.0);
    let c = rect.center() - vec2(0.0, extra);
    ui.painter().text(c - vec2(0.0, 34.0), Align2::CENTER_CENTER, glyph, icon_font(40.0), with_alpha(theme.text_muted, 110));
    ui.painter().text(c + vec2(0.0, 6.0), Align2::CENTER_CENTER, title, medium(15.0), theme.text_muted);
    if let Some(g) = sub {
        // A centered job lays out around x = 0.
        ui.painter().galley(pos2(rect.center().x, c.y + 20.0), g, theme.text_placeholder);
    }
}

