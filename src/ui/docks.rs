//! Docks: the left, right and bottom islands. Each shows a tab header and the active panel.
//! Any panel (including terminals) can be moved between docks from its tab's context menu.

use crate::app::{App, Dock, PanelItem};
use crate::commands::Command;
use crate::ui::widgets::{self, TabIcon, empty_state, icon_button_sized, truncated_label};
use crate::ui::{self, TAB_BAR_H, ic};
use egui::{Rect, Stroke, Ui, UiBuilder, pos2, vec2};

impl App {
    fn item_title(&self, item: PanelItem) -> String {
        match item {
            PanelItem::Terminal(id) => self.terminals.iter().find(|t| t.id == id).map(|t| t.view.title()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Terminal".into()),
            PanelItem::Files => self.workspace.as_ref().map(|w| w.name.clone()).unwrap_or_else(|| "Files".into()),
            other => other.label().to_string(),
        }
    }

    pub(crate) fn dock_ui(&mut self, ui: &mut Ui, rect: Rect, dock: Dock) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), TAB_BAR_H));
        let ds = self.layout.dock(dock).clone();
        let Some(active) = ds.active_item() else {
            empty_state(ui, &theme, ic::DASHBOARD, "Empty panel", "");
            return;
        };
        let compact = dock == Dock::Left;
        let mut select = None;
        let mut move_to: Option<(PanelItem, Dock)> = None;
        let mut close_terminal = None;

        // Trailing actions first (right-to-left) so tabs know how much room they have.
        let mut actions = ui.new_child(UiBuilder::new().max_rect(header.shrink2(vec2(8.0, 5.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        actions.spacing_mut().item_spacing.x = 2.0;
        if !compact && icon_button_sized(&mut actions, &theme, ic::ADD, "New Terminal Here", 26.0, 18.0, None, false).clicked() {
            self.new_terminal(dock);
        }
        self.panel_actions(&mut actions, active);
        let actions_left = actions.min_rect().left();

        let tabs_rect = Rect::from_min_max(header.min + vec2(6.0, 4.0), pos2(actions_left - 6.0, header.bottom() - 4.0));
        let mut tabs = ui.new_child(UiBuilder::new().max_rect(tabs_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        tabs.set_clip_rect(tabs_rect);
        tabs.spacing_mut().item_spacing.x = 2.0;
        // Labeled tabs when they fit; otherwise only the active tab keeps its label.
        let label_w = |app: &App, it: &PanelItem| ui.painter().layout_no_wrap(app.item_title(*it), ui::medium(ui.style().text_styles[&egui::TextStyle::Body].size), theme.text).size().x + 50.0;
        let full_w: f32 = ds.items.iter().map(|it| label_w(self, it)).sum();
        let squeeze = !compact && full_w > tabs_rect.width();
        for (i, item) in ds.items.iter().enumerate() {
            let is_active = i == ds.active;
            let resp = if compact || (squeeze && !is_active && !matches!(item, PanelItem::Terminal(_))) {
                let color = if is_active { theme.text } else { theme.icon };
                icon_button_sized(&mut tabs, &theme, item.icon(), &self.item_title(*item), 28.0, 18.0, Some(color), is_active)
            } else {
                let title = self.item_title(*item);
                let closable = matches!(item, PanelItem::Terminal(_));
                let color = if is_active { theme.text_accent } else { theme.icon };
                let r = widgets::tab(&mut tabs, &theme, TabIcon::Glyph(item.icon(), color), &title, is_active, false, closable, false);
                if r.close
                    && let PanelItem::Terminal(id) = item
                {
                    close_terminal = Some(*id);
                }
                r.response
            };
            if resp.clicked() {
                select = Some(i);
            }
            resp.context_menu(|ui| {
                for (d, label) in [(Dock::Left, "Move to Left Sidebar"), (Dock::Right, "Move to Right Sidebar"), (Dock::Bottom, "Move to Bottom Panel")] {
                    if d != dock && ui.button(label).clicked() {
                        move_to = Some((*item, d));
                        ui.close();
                    }
                }
                if let PanelItem::Terminal(id) = item {
                    ui.separator();
                    if ui.button("Close Terminal").clicked() {
                        close_terminal = Some(*id);
                        ui.close();
                    }
                }
            });
        }
        ui.painter().hline(rect.left() + 1.0..=rect.right() - 1.0, header.bottom() - 0.5, Stroke::new(1.0, tk.island_border));

        if let Some(i) = select {
            self.layout.dock_mut(dock).active = i;
            if let Some(PanelItem::Terminal(id)) = ds.items.get(i)
                && let Some(t) = self.terminals.iter_mut().find(|t| t.id == *id) {
                    t.focus = true;
                }
        }
        if let Some((item, d)) = move_to {
            self.layout.move_to(item, d);
        }
        if let Some(id) = close_terminal {
            self.close_terminal(id);
        }

        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
        let pad = match active {
            PanelItem::Terminal(_) => vec2(8.0, 6.0),
            PanelItem::Interactions => vec2(0.0, 0.0),
            _ => vec2(6.0, 6.0),
        };
        let inner = body.shrink2(pad);
        let mut b = ui.new_child(UiBuilder::new().max_rect(inner));
        b.set_clip_rect(body.shrink(1.0).intersect(ui.clip_rect()));
        match active {
            PanelItem::Files => self.files_view(&mut b),
            PanelItem::Outline => self.outline_view(&mut b),
            PanelItem::Search => self.search_view(&mut b),
            PanelItem::Problems => self.problems_view(&mut b),
            PanelItem::Terminal(id) => self.terminal_panel(&mut b, id),
            PanelItem::Interactions | PanelItem::Stepper | PanelItem::Visualizer | PanelItem::Tests | PanelItem::Macros | PanelItem::Docs | PanelItem::Debugger | PanelItem::Profiler | PanelItem::Modules | PanelItem::Log => {
                crate::racket_ui::panel(self, &mut b, inner, active)
            }
        }
    }

    /// Buttons specific to the active panel, in the dock header.
    fn panel_actions(&mut self, ui: &mut Ui, item: PanelItem) {
        let theme = self.theme.clone();
        match item {
            PanelItem::Files if self.workspace.is_some() => {
                if icon_button_sized(ui, &theme, ic::UNFOLD_LESS, "Collapse All", 26.0, 17.0, None, false).clicked()
                    && let Some(ws) = &mut self.workspace
                {
                    ws.collapse_all();
                }
                if icon_button_sized(ui, &theme, ic::REFRESH, "Refresh", 26.0, 17.0, None, false).clicked()
                    && let Some(ws) = &mut self.workspace
                {
                    ws.invalidate();
                }
                if icon_button_sized(ui, &theme, ic::CREATE_NEW_FOLDER, "New Folder", 26.0, 17.0, None, false).clicked() {
                    self.new_folder_in_project();
                }
                if icon_button_sized(ui, &theme, ic::NOTE_ADD, "New File", 26.0, 17.0, None, false).clicked() {
                    self.new_file_in_project();
                }
            }
            PanelItem::Interactions
                if icon_button_sized(ui, &theme, ic::CLEANING_SERVICES, "Clear Interactions", 26.0, 17.0, None, false).clicked() => {
                    self.run_command(Command::ClearInteractions);
                }
            _ => {}
        }
    }

    fn terminal_panel(&mut self, ui: &mut Ui, id: u64) {
        let theme = self.theme.clone();
        let font = ui::mono(self.settings.terminal_font_size);
        let Some(t) = self.terminals.iter_mut().find(|t| t.id == id) else {
            empty_state(ui, &theme, ic::TERMINAL, "Terminal closed", "");
            return;
        };
        if t.focus {
            t.view.focus(ui);
            t.focus = false;
        }
        t.view.ui(ui, &theme, &font);
    }

    fn problems_view(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        // (tab index, diagnostic index, file title, severity, line, message)
        let mut items: Vec<(usize, usize, String, crate::editor::view::Severity, usize, String)> = Vec::new();
        for (ti, t) in self.tabs.iter().enumerate() {
            for (di, d) in t.editor.deco.diagnostics.iter().enumerate().filter(|(_, d)| t.editor.deco.shown(d)) {
                let line = t.editor.doc.buffer.line_of(d.range.start);
                items.push((ti, di, t.title(), d.severity, line, d.message.clone()));
            }
        }
        if items.is_empty() {
            empty_state(ui, &theme, ic::CHECK_CIRCLE, "No problems", "Errors from Run and Check Syntax appear here");
            return;
        }
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("problems").auto_shrink([false, false]).show(ui, |ui| {
            for (ti, di, title, sev, line, msg) in items.iter() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), egui::Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, egui::CornerRadius::same(6), tk.hover);
                }
                let (glyph, c) = match sev {
                    crate::editor::view::Severity::Error => (ic::ERROR, theme.error),
                    crate::editor::view::Severity::Warning => (ic::WARNING, theme.warning),
                };
                ui.painter().text(pos2(r.left() + 14.0, r.center().y), egui::Align2::CENTER_CENTER, glyph, ui::icon_font_filled(16.0), c);
                let loc = format!("{title}:{}", line + 1);
                let lw = ui.painter().layout_no_wrap(loc.clone(), ui::ui_font(12.5), theme.text_muted).size().x;
                truncated_label(ui, pos2(r.left() + 30.0, r.center().y), msg.lines().next().unwrap_or(""), ui::ui_font(13.5), theme.text, r.width() - lw - 50.0);
                ui.painter().text(pos2(r.right() - 10.0, r.center().y), egui::Align2::RIGHT_CENTER, loc, ui::ui_font(12.5), theme.text_muted);
                if resp.clicked() {
                    jump = Some((*ti, *di));
                }
            }
        });
        if let Some((ti, k)) = jump {
            self.active = ti;
            let t = &mut self.tabs[ti];
            if let Some(d) = t.editor.deco.diagnostics.get(k).cloned() {
                t.editor.reveal(d.range);
                t.editor.request_focus();
            }
        }
    }
}
