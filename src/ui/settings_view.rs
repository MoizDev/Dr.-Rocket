//! Settings sheet and the other modal dialogs.

use crate::app::{App, Modal};
use crate::config::{CursorShape, IndentStyle};
use crate::theme::{Theme, with_alpha};
use crate::ui::{self, ic};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Appearance,
    Editor,
    Racket,
    Terminal,
}

#[derive(Default)]
pub struct SettingsView {
    pub section: Section,
}

/// iOS-style switch.
pub fn toggle(ui: &mut Ui, theme: &Theme, on: &mut bool) -> egui::Response {
    let (r, mut resp) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.12);
    let bg = crate::theme::mix(crate::ui::Tokens::of(theme).pressed, theme.text_accent, t);
    ui.painter().rect_filled(r, CornerRadius::same(10), bg);
    let x = egui::lerp(r.left() + 10.0..=r.right() - 10.0, t);
    ui.painter().circle_filled(pos2(x, r.center().y), 7.5, egui::Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Radio button styled like the rest of the UI. Returns true when clicked.
pub fn radio(ui: &mut Ui, theme: &Theme, selected: bool, label: &str) -> bool {
    let font = ui::ui_font(14.0);
    let g = ui.painter().layout_no_wrap(label.to_string(), font, theme.text);
    let (r, resp) = ui.allocate_exact_size(vec2(26.0 + g.size().x + 4.0, 26.0), Sense::click());
    let c = pos2(r.left() + 9.0, r.center().y);
    let ring = if selected { theme.text_accent } else if resp.hovered() { theme.text_muted } else { theme.text_placeholder };
    ui.painter().circle_stroke(c, 7.0, Stroke::new(1.5, ring));
    if selected {
        ui.painter().circle_filled(c, 4.0, theme.text_accent);
    }
    ui.painter().galley(pos2(r.left() + 26.0, r.center().y - g.size().y / 2.0), g, theme.text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn row(ui: &mut Ui, theme: &Theme, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - 200.0).max(160.0));
            ui.label(egui::RichText::new(title).font(ui::medium(14.0)).color(theme.text));
            if !desc.is_empty() {
                ui.label(egui::RichText::new(desc).size(12.5).color(theme.text_placeholder));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
    ui.add_space(6.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, crate::ui::Tokens::of(theme).island_border));
    ui.add_space(2.0);
}

/// Mini code preview of a theme, used on the theme cards.
fn theme_preview(ui: &Ui, r: Rect, t: &Theme) {
    let p = ui.painter();
    p.rect_filled(r, CornerRadius::same(8), t.editor_bg);
    let lines: [&[(&str, egui::Color32)]; 4] = [
        &[("(", t.rainbow(0)), ("define", t.syntax.keyword.color), (" (", t.rainbow(1)), ("fact", t.syntax.function.color), (" n", t.syntax.variable.color), (")", t.rainbow(1))],
        &[("  (", t.rainbow(1)), ("if", t.syntax.keyword.color), (" (", t.rainbow(2)), ("zero?", t.syntax.function.color), (" n", t.syntax.variable.color), (") ", t.rainbow(2)), ("1", t.syntax.number.color)],
        &[("      (", t.rainbow(2)), ("*", t.syntax.operator.color), (" n ", t.syntax.variable.color), ("…", t.text_muted), ("))", t.rainbow(1))],
        &[("; ", t.syntax.comment.color), ("\"done\"", t.syntax.string.color)],
    ];
    for (k, segs) in lines.iter().enumerate() {
        let mut job = egui::text::LayoutJob::default();
        for (s, c) in segs.iter() {
            job.append(s, 0.0, egui::TextFormat { font_id: ui::mono(10.5), color: *c, ..Default::default() });
        }
        let g = p.layout_job(job);
        p.galley(pos2(r.left() + 10.0, r.top() + 10.0 + k as f32 * 15.0), g, t.editor_fg);
    }
}

impl App {
    pub(crate) fn modal_ui(&mut self, ctx: &egui::Context) {
        let modal = std::mem::replace(&mut self.modal, Modal::None);
        match modal {
            Modal::None => {}
            Modal::Palette(p) => self.palette_ui(ctx, p),
            Modal::Settings(s) => self.settings_ui(ctx, s),
            Modal::Language(d) => self.language_dialog_ui(ctx, d),
            Modal::Prompt(p) => self.prompt_ui(ctx, p),
            Modal::ConfirmClose { tab, then_quit } => self.confirm_close_ui(ctx, tab, then_quit),
            Modal::ConfirmQuit => self.confirm_quit_ui(ctx),
            Modal::About => self.about_ui(ctx),
        }
    }

    /// Centered modal card with a dimmed backdrop.
    pub(crate) fn dialog(&mut self, ctx: &egui::Context, title: &str, width: f32, body: impl FnOnce(&mut App, &mut Ui)) {
        let screen = ctx.content_rect();
        let theme = self.theme.clone();
        let tk = self.tokens();
        egui::Area::new(Id::unique("modal-scrim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::ZERO, egui::Color32::from_black_alpha(if theme.dark { 110 } else { 50 }));
        });
        egui::Area::new(Id::unique(("modal", title))).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(screen.center()).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::same(20)).show(ui, |ui| {
                ui.set_width(width);
                ui.label(egui::RichText::new(title).font(ui::semibold(17.0)).color(theme.text));
                ui.add_space(8.0);
                body(self, ui);
            });
        });
    }

    fn confirm_close_ui(&mut self, ctx: &egui::Context, tab: u64, then_quit: bool) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == tab) else { return };
        let name = self.tabs[idx].title();
        let mut choice = None;
        self.dialog(ctx, &format!("Save changes to “{name}”?"), 400.0, |app, ui| {
            ui.label(egui::RichText::new("Your changes will be lost if you don't save them.").color(app.theme.text_muted));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button("Don't Save").clicked() {
                    choice = Some(0);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button(&app.theme, "Save")).clicked() {
                        choice = Some(2);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(1);
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(1);
        }
        match choice {
            Some(0) => {
                self.close_tab(tab, true);
                if then_quit {
                    self.modal = Modal::ConfirmQuit;
                }
            }
            Some(1) => {}
            Some(2) => {
                if self.save_tab(idx, false) {
                    self.close_tab(tab, true);
                }
            }
            _ => self.modal = Modal::ConfirmClose { tab, then_quit },
        }
    }

    fn confirm_quit_ui(&mut self, ctx: &egui::Context) {
        let dirty: Vec<String> = self.tabs.iter().filter(|t| t.dirty()).map(|t| t.title()).collect();
        if dirty.is_empty() {
            self.quit_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let mut choice = None;
        self.dialog(ctx, "Quit with unsaved changes?", 420.0, |app, ui| {
            ui.label(egui::RichText::new(format!("Unsaved: {}", dirty.join(", "))).color(app.theme.text_muted));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button("Discard & Quit").clicked() {
                    choice = Some(0);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button(&app.theme, "Save All & Quit")).clicked() {
                        choice = Some(2);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(1);
                    }
                });
            });
        });
        match choice {
            Some(0) => {
                self.quit_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(1) => {}
            Some(2) => {
                for i in 0..self.tabs.len() {
                    if self.tabs[i].dirty() && !self.save_tab(i, false) {
                        return;
                    }
                }
                self.quit_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            _ => self.modal = Modal::ConfirmQuit,
        }
    }

    fn about_ui(&mut self, ctx: &egui::Context) {
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        self.dialog(ctx, "About", 380.0, |app, ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                crate::ui::welcome::paint_logo(ui, r, &app.theme);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(crate::ui::welcome::APP_NAME).font(ui::semibold(18.0)).color(app.theme.text));
                    ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(app.theme.text_muted));
                });
            });
            ui.add_space(10.0);
            ui.label(egui::RichText::new("A native Racket IDE written in Rust. Uses Racket's own stepper, check-syntax and debugger.").color(app.theme.text_muted));
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button(&app.theme, "Close")).clicked() {
                    close = true;
                }
            });
        });
        if !close {
            self.modal = Modal::About;
        }
    }

    fn settings_ui(&mut self, ctx: &egui::Context, mut sv: SettingsView) {
        let screen = ctx.content_rect();
        let theme = self.theme.clone();
        let tk = self.tokens();
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let before = self.settings.clone();
        let size = vec2(860.0_f32.min(screen.width() - 60.0), 620.0_f32.min(screen.height() - 80.0));
        egui::Area::new(Id::unique("settings-scrim")).order(egui::Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::ZERO, egui::Color32::from_black_alpha(if theme.dark { 110 } else { 50 }));
            if resp.clicked() {
                close = true;
            }
        });
        egui::Area::new(Id::unique("settings")).order(egui::Order::Foreground).pivot(Align2::CENTER_CENTER).fixed_pos(screen.center()).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(tk.popup).corner_radius(CornerRadius::same(14)).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                // Navigation.
                let nav = Rect::from_min_size(rect.min, vec2(200.0, rect.height()));
                ui.painter().rect_filled(nav, CornerRadius { nw: 14, sw: 14, ne: 0, se: 0 }, with_alpha(tk.hover, 120));
                ui.painter().vline(nav.right(), nav.y_range(), Stroke::new(1.0, tk.island_border));
                ui.painter().text(pos2(nav.left() + 20.0, nav.top() + 30.0), Align2::LEFT_CENTER, "Settings", ui::semibold(18.0), theme.text);
                for (k, (sec, glyph, label)) in [(Section::Appearance, ic::PALETTE, "Appearance"), (Section::Editor, ic::CODE, "Editor"), (Section::Racket, ic::FUNCTION, "Racket"), (Section::Terminal, ic::TERMINAL, "Terminal")].into_iter().enumerate() {
                    let r = Rect::from_min_size(pos2(nav.left() + 10.0, nav.top() + 60.0 + k as f32 * 36.0), vec2(180.0, 32.0));
                    let resp = ui.interact(r, Id::unique(("settings-nav", k)), Sense::click());
                    if sv.section == sec {
                        ui.painter().rect_filled(r, CornerRadius::same(8), tk.selected);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(8), tk.hover);
                    }
                    ui.painter().text(pos2(r.left() + 18.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(18.0), if sv.section == sec { theme.text_accent } else { theme.icon });
                    ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, label, ui::medium(14.0), theme.text);
                    if resp.clicked() {
                        sv.section = sec;
                    }
                }
                let content = Rect::from_min_max(pos2(nav.right() + 24.0, rect.top() + 18.0), rect.max - vec2(24.0, 18.0));
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(content));
                egui::ScrollArea::vertical().id_salt("settings-scroll").auto_shrink([false, false]).show(&mut c, |ui| {
                    ui.set_width(content.width() - 8.0);
                    match sv.section {
                        Section::Appearance => self.settings_appearance(ui),
                        Section::Editor => self.settings_editor(ui),
                        Section::Racket => self.settings_racket(ui),
                        Section::Terminal => self.settings_terminal(ui),
                    }
                });
                let close_r = Rect::from_min_size(pos2(rect.right() - 40.0, rect.top() + 12.0), vec2(28.0, 28.0));
                let cr = ui.interact(close_r, Id::unique("settings-close"), Sense::click());
                if cr.hovered() {
                    ui.painter().rect_filled(close_r, CornerRadius::same(6), tk.hover);
                }
                ui.painter().text(close_r.center(), Align2::CENTER_CENTER, ic::CLOSE, ui::icon_font(18.0), theme.icon);
                if cr.clicked() {
                    close = true;
                }
            });
        });
        if self.settings != before {
            let _ = self.settings.save();
            if self.settings.cursor_color != before.cursor_color || self.settings.rainbow_colors != before.rainbow_colors {
                self.apply_theme_overrides();
            }
        }
        if !close {
            self.modal = Modal::Settings(sv);
        }
    }

    fn settings_appearance(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        ui.label(egui::RichText::new("Theme").font(ui::semibold(15.0)).color(theme.text));
        ui.label(egui::RichText::new("Bundled themes plus every theme installed in Zed.").size(12.5).color(theme.text_placeholder));
        ui.add_space(10.0);
        let names: Vec<String> = self.themes.names();
        let card_w = ((ui.available_width() - 24.0) / 3.0).floor();
        let mut pick = None;
        for chunk in names.chunks(3) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for name in chunk {
                    let Some(t) = self.themes.get(name) else { continue };
                    let (r, resp) = ui.allocate_exact_size(vec2(card_w, 112.0), Sense::click());
                    let selected = &self.settings.theme == name;
                    let preview = Rect::from_min_size(r.min, vec2(r.width(), 82.0));
                    theme_preview(ui, preview, t);
                    let stroke = if selected { Stroke::new(2.0, theme.text_accent) } else if resp.hovered() { Stroke::new(1.0, tk.subtle_border) } else { Stroke::new(1.0, tk.island_border) };
                    ui.painter().rect_stroke(preview, CornerRadius::same(8), stroke, StrokeKind::Outside);
                    ui.painter().text(pos2(r.left() + 2.0, r.bottom() - 12.0), Align2::LEFT_CENTER, name, ui::medium(13.0), if selected { theme.text } else { theme.text_muted });
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        pick = Some(name.clone());
                    }
                }
            });
            ui.add_space(10.0);
        }
        if let Some(n) = pick {
            self.set_theme(&n, true);
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Typography").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        let s = &mut self.settings;
        row(ui, &theme, "Editor font size", "Fira Code, with ligatures", |ui| {
            ui.add(egui::DragValue::new(&mut s.buffer_font_size).range(9.0..=40.0).speed(0.25).suffix(" px"));
        });
        row(ui, &theme, "Line height", "Multiple of the font size", |ui| {
            ui.add(egui::DragValue::new(&mut s.line_height).range(1.0..=2.4).speed(0.01));
        });
        row(ui, &theme, "Interface font size", "Inter", |ui| {
            ui.add(egui::DragValue::new(&mut s.ui_font_size).range(11.0..=20.0).speed(0.1).suffix(" px"));
        });
    }

    fn settings_editor(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        caret_and_brackets_preview(ui, &theme, &self.settings, self.settings.buffer_font_size.min(17.0));
        ui.add_space(10.0);

        ui.label(egui::RichText::new("Caret").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        let s = &mut self.settings;
        row(ui, &theme, "Shape", "Block is the thick, terminal-style caret", |ui| {
            for (shape, label) in [(CursorShape::Underline, "Underline"), (CursorShape::Block, "Block"), (CursorShape::Bar, "Bar")] {
                if ui.add(egui::Button::new(label).selected(s.cursor_shape == shape)).clicked() {
                    s.cursor_shape = shape;
                }
            }
        });
        row(ui, &theme, "Thickness", "Width of the bar caret, height of the underline", |ui| {
            ui.add(egui::Slider::new(&mut s.cursor_width, 1.0..=6.0).step_by(0.5).suffix(" px"));
        });
        row(ui, &theme, "Smooth caret", "The caret glides to its new position", |ui| {
            toggle(ui, &theme, &mut s.smooth_caret);
        });
        row(ui, &theme, "Blinking caret", "", |ui| {
            toggle(ui, &theme, &mut s.cursor_blink);
        });
        let mut custom = s.cursor_color.is_some();
        let mut color = s.cursor_color.as_deref().and_then(crate::theme::parse_color).unwrap_or(theme.cursor);
        row(ui, &theme, "Custom caret color", "Otherwise the theme's caret color", |ui| {
            if custom && ui.color_edit_button_srgba(&mut color).changed() {
                s.cursor_color = Some(hex(color));
            }
            if toggle(ui, &theme, &mut custom).changed() {
                s.cursor_color = if custom { Some(hex(color)) } else { None };
            }
        });

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Brackets & Indentation").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        row(ui, &theme, "Rainbow brackets", "Color parentheses by nesting depth", |ui| {
            toggle(ui, &theme, &mut s.rainbow_brackets);
        });
        row(ui, &theme, "Highlight the bracket pair at the caret", "In its own depth color, with its s-expression shaded", |ui| {
            toggle(ui, &theme, &mut s.bracket_pair_highlight);
        });
        // Palette editor.
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Bracket colors").font(ui::medium(14.0)).color(theme.text));
        ui.label(egui::RichText::new("Used for brackets and indent guides, cycling by depth.").size(12.5).color(theme.text_placeholder));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let theme_default = s.rainbow_colors.is_empty();
            if ui.add(egui::Button::new("Theme").selected(theme_default)).clicked() {
                s.rainbow_colors.clear();
            }
            for (name, colors) in crate::config::RAINBOW_PRESETS {
                let selected = s.rainbow_colors.iter().map(|c| c.to_ascii_uppercase()).eq(colors.iter().map(|c| c.to_ascii_uppercase()));
                let (r, resp) = ui.allocate_exact_size(vec2(18.0 + colors.len() as f32 * 9.0 + ui.painter().layout_no_wrap(name.to_string(), ui::ui_font(13.0), theme.text).size().x + 8.0, 28.0), Sense::click());
                let tk = crate::ui::Tokens::of(&theme);
                ui.painter().rect(r, CornerRadius::same(7), if selected { tk.selected } else if resp.hovered() { tk.hover } else { egui::Color32::TRANSPARENT }, Stroke::new(1.0, if selected { theme.text_accent } else { tk.island_border }), StrokeKind::Inside);
                for (k, c) in colors.iter().enumerate() {
                    if let Some(c) = crate::theme::parse_color(c) {
                        ui.painter().circle_filled(pos2(r.left() + 12.0 + k as f32 * 9.0, r.center().y), 5.0, c);
                    }
                }
                ui.painter().text(pos2(r.left() + 14.0 + colors.len() as f32 * 9.0, r.center().y), Align2::LEFT_CENTER, *name, ui::ui_font(13.0), theme.text);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    s.rainbow_colors = colors.iter().map(|c| c.to_string()).collect();
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            let mut colors: Vec<egui::Color32> = if s.rainbow_colors.is_empty() { theme.rainbow.clone() } else { s.rainbow_colors.iter().filter_map(|c| crate::theme::parse_color(c)).collect() };
            let mut changed = false;
            let mut remove = None;
            for (k, c) in colors.iter_mut().enumerate() {
                ui.vertical(|ui| {
                    if ui.color_edit_button_srgba(c).changed() {
                        changed = true;
                    }
                    if colors_len_gt1(&s.rainbow_colors, &theme) && ui.small_button("✕").on_hover_text("Remove").clicked() {
                        remove = Some(k);
                    }
                });
            }
            if let Some(k) = remove {
                colors.remove(k);
                changed = true;
            }
            if colors.len() < 8 && ui.button("+ Add").clicked() {
                colors.push(theme.text_accent);
                changed = true;
            }
            if changed {
                s.rainbow_colors = colors.iter().map(|c| hex(*c)).collect();
            }
        });
        ui.add_space(8.0);
        row(ui, &theme, "Indentation", "Rainbow highlights per level, guide lines, or nothing", |ui| {
            for (style, label) in [(IndentStyle::Off, "Off"), (IndentStyle::Lines, "Lines"), (IndentStyle::Highlights, "Highlights")] {
                if ui.add(egui::Button::new(label).selected(s.indent_style == style)).clicked() {
                    s.indent_style = style;
                }
            }
        });
        if s.indent_style == IndentStyle::Lines {
            row(ui, &theme, "Rainbow guide lines", "Lines use the bracket colors of their depth", |ui| {
                toggle(ui, &theme, &mut s.rainbow_indent_guides);
            });
        }

        ui.add_space(10.0);
        ui.label(egui::RichText::new("Editor").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        row(ui, &theme, "Line numbers", "", |ui| {
            toggle(ui, &theme, &mut s.show_line_numbers);
        });
        row(ui, &theme, "Highlight current line", "", |ui| {
            toggle(ui, &theme, &mut s.highlight_active_line);
        });
        row(ui, &theme, "Auto-close brackets and quotes", "Typing ) always inserts the matching closer, like DrRacket", |ui| {
            toggle(ui, &theme, &mut s.auto_close_brackets);
        });
        row(ui, &theme, "Restore session on launch", "Reopen the last project and files", |ui| {
            toggle(ui, &theme, &mut s.restore_session);
        });
    }

    fn settings_racket(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let s = &mut self.settings;
        ui.label(egui::RichText::new("Racket").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        row(ui, &theme, "Background check syntax", "Expand the program as you type: errors, binding arrows, unused bindings", |ui| {
            toggle(ui, &theme, &mut s.online_check_syntax);
        });
        row(ui, &theme, "Binding arrows", "DrRacket-style arrows from bindings to their uses on hover", |ui| {
            toggle(ui, &theme, &mut s.show_binding_arrows);
        });
        let mut path = s.racket_path.clone().unwrap_or_default();
        row(ui, &theme, "Racket executable", "Leave empty to auto-detect", |ui| {
            ui.add(egui::TextEdit::singleline(&mut path).desired_width(220.0).hint_text("auto"));
        });
        s.racket_path = if path.trim().is_empty() { None } else { Some(path) };
    }

    fn settings_terminal(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let s = &mut self.settings;
        ui.label(egui::RichText::new("Terminal").font(ui::semibold(15.0)).color(theme.text));
        ui.add_space(4.0);
        row(ui, &theme, "Font size", "", |ui| {
            ui.add(egui::DragValue::new(&mut s.terminal_font_size).range(9.0..=32.0).speed(0.25).suffix(" px"));
        });
        let mut shell = s.shell.clone().unwrap_or_default();
        row(ui, &theme, "Shell", "Leave empty to use $SHELL", |ui| {
            ui.add(egui::TextEdit::singleline(&mut shell).desired_width(220.0).hint_text("$SHELL"));
        });
        s.shell = if shell.trim().is_empty() { None } else { Some(shell) };
    }
}

/// Accent-filled button for the primary action of a dialog.
pub fn primary_button(theme: &Theme, label: &str) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.to_string()).color(egui::Color32::WHITE).font(ui::medium(13.5))).fill(theme.text_accent).corner_radius(CornerRadius::same(7))
}

fn colors_len_gt1(custom: &[String], theme: &Theme) -> bool {
    if custom.is_empty() { theme.rainbow.len() > 1 } else { custom.len() > 1 }
}

fn hex(c: egui::Color32) -> String {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// A live preview of the caret and bracket settings.
fn caret_and_brackets_preview(ui: &mut Ui, theme: &Theme, s: &crate::config::Settings, size: f32) {
    let tk = crate::ui::Tokens::of(theme);
    let font = ui::mono(size);
    let line_h = (size * 1.6).round();
    let lines: [(usize, &str); 5] = [(0, "(define (sum-squares lst)"), (2, "(for/fold ([acc 0])"), (12, "([x (in-list lst)])"), (4, "(+ acc (* x x))))"), (0, "")];
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, line_h * 4.0 + 24.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(10), theme.editor_bg, Stroke::new(1.0, tk.island_border), StrokeKind::Inside);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
    let x0 = rect.left() + 18.0;
    let rainbow = |d: usize| if s.rainbow_brackets { theme.rainbow(d) } else { theme.syntax.punctuation.color };
    let p = ui.painter().with_clip_rect(rect.shrink(1.0));
    // Indent guides at columns 0 and 2 (the define and for/fold brackets).
    for (li, &(indent, _)) in lines.iter().enumerate().take(4) {
        let y = rect.top() + 12.0 + li as f32 * line_h;
        let guides: &[(usize, usize)] = match li {
            1 => &[(0, 0)],
            2 | 3 => &[(0, 0), (2, 1)],
            _ => &[],
        };
        for (k, &(col, depth)) in guides.iter().enumerate() {
            let gx = x0 + col as f32 * cw;
            if s.indent_style == IndentStyle::Highlights {
                let next = guides.get(k + 1).map(|g| g.0).unwrap_or(indent);
                p.rect_filled(Rect::from_min_max(pos2(gx, y), pos2(x0 + next as f32 * cw, y + line_h)), CornerRadius::ZERO, with_alpha(rainbow(depth), if theme.dark { 40 } else { 46 }));
            }
            if s.indent_style == IndentStyle::Lines {
                let c = if s.rainbow_indent_guides { with_alpha(rainbow(depth), 110) } else { theme.indent_guide };
                p.vline(gx.round() + 0.5, y..=y + line_h, Stroke::new(1.0, c));
            }
        }
    }
    let mut depth: i32 = 0;
    for (li, &(indent, text)) in lines.iter().enumerate().take(4) {
        let y = rect.top() + 12.0 + li as f32 * line_h;
        let mut job = egui::text::LayoutJob::default();
        for ch in text.chars() {
            let color = match ch {
                '(' | '[' => {
                    let c = rainbow(depth.max(0) as usize);
                    depth += 1;
                    c
                }
                ')' | ']' => {
                    depth -= 1;
                    rainbow(depth.max(0) as usize)
                }
                _ => theme.syntax.variable.color,
            };
            job.append(&ch.to_string(), 0.0, egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
        }
        let g = p.layout_job(job);
        p.galley(pos2(x0 + indent as f32 * cw, y + (line_h - g.size().y) / 2.0), g, theme.editor_fg);
    }
    // Caret after "acc" on the last line.
    let cy = rect.top() + 12.0 + 3.0 * line_h;
    let cx = x0 + (4 + 6) as f32 * cw;
    let color = s.cursor_color.as_deref().and_then(crate::theme::parse_color).unwrap_or(theme.cursor);
    let width = s.cursor_width.clamp(1.0, 8.0);
    match s.cursor_shape {
        CursorShape::Bar => p.rect_filled(Rect::from_min_size(pos2(cx - width / 2.0, cy + 3.0), vec2(width, line_h - 6.0)), CornerRadius::same(1), color),
        CursorShape::Block => p.rect_filled(Rect::from_min_size(pos2(cx, cy + 2.0), vec2(cw, line_h - 4.0)), CornerRadius::same(2), with_alpha(color, 200)),
        CursorShape::Underline => p.rect_filled(Rect::from_min_size(pos2(cx, cy + line_h - 2.0 - width), vec2(cw, width)), CornerRadius::ZERO, color),
    };
}
