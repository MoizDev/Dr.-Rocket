//! Window chrome: title bar, editor tab strip, status bar.

use crate::app::{App, Dock, PanelItem};
use crate::commands::Command;
use crate::theme::with_alpha;
use crate::ui::widgets::{self, TabIcon, icon_button, icon_button_sized, pill_button};
use crate::ui::{self, TAB_BAR_H, file_icons, ic};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

impl App {
    pub(crate) fn title_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        // Drag the window by its title bar; double-click zooms (macOS behavior).
        let drag = ui.interact(rect, Id::unique("titlebar-drag"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }

        let fullscreen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));
        crate::platform::set_fullscreen(fullscreen);
        // No traffic lights in full screen.
        let left = rect.left() + if fullscreen { 4.0 } else { crate::platform::TRAFFIC_LIGHTS_W + 10.0 };
        let row = Rect::from_min_max(pos2(left, rect.top()), pos2(rect.right() - 10.0, rect.bottom()));
        let mut bar = ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
        bar.spacing_mut().item_spacing.x = 4.0;

        // App menu.
        let mr = icon_button(&mut bar, &theme, ic::MENU, "Menu");
        egui::Popup::menu(&mr).width(260.0).show(|ui| self.app_menu(ui));
        let tabs_left = bar.cursor().min.x + 6.0;

        // Right side, laid out right-to-left.
        let mut rb = bar.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
        rb.spacing_mut().item_spacing.x = 2.0;
        let (lo, ro, bo) = (self.layout.left.open, self.layout.right.open, self.layout.bottom.open);
        let toggles = [
            (Command::ToggleRightPanel, if ro { ic::RIGHT_PANEL_CLOSE } else { ic::RIGHT_PANEL_OPEN }, crate::platform::keys("Toggle Right Sidebar (⌘E)"), ro),
            (Command::ToggleBottomPanel, if bo { ic::BOTTOM_PANEL_CLOSE } else { ic::BOTTOM_PANEL_OPEN }, crate::platform::keys("Toggle Bottom Panel (⌘J)"), bo),
            (Command::ToggleLeftPanel, if lo { ic::LEFT_PANEL_CLOSE } else { ic::LEFT_PANEL_OPEN }, crate::platform::keys("Toggle Left Sidebar (⌘B)"), lo),
        ];
        for (cmd, glyph, tip, _on) in toggles {
            if icon_button_sized(&mut rb, &theme, glyph, &tip, 30.0, 19.0, None, false).clicked() {
                self.run_command(cmd);
            }
        }
        rb.add_space(6.0);
        let sep = rb.allocate_exact_size(vec2(1.0, 20.0), Sense::hover()).0;
        rb.painter().vline(sep.center().x, sep.y_range(), Stroke::new(1.0, tk.subtle_border));
        rb.add_space(6.0);
        // Run / Stop.
        let running = crate::racket_ui::is_running(self);
        let (glyph, color, tip) = if running { (ic::STOP, theme.error, crate::platform::keys("Stop (⌘K)")) } else { (ic::PLAY_ARROW, tk.run, crate::platform::keys("Run (⌘R)")) };
        let (r, resp) = rb.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
        if resp.hovered() {
            rb.painter().rect_filled(r, CornerRadius::same(8), with_alpha(color, 36));
        }
        rb.painter().text(r.center(), Align2::CENTER_CENTER, glyph, ui::icon_font_filled(24.0), color);
        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.run_command(if running { Command::Kill } else { Command::Run });
        }
        rb.add_space(6.0);
        // Language selector, styled like the run-configuration pill in the mockup.
        let label = self.active_tab().map(crate::racket_ui::language_label).unwrap_or_else(|| "Racket".into());
        let icon = self.active_tab().map(crate::racket_ui::language_icon).unwrap_or(ic::CODE);
        let lr = pill_button(&mut rb, &theme, Some((icon, theme.syntax.keyword.color)), &label, true, true);
        if lr.on_hover_text(crate::platform::keys("Choose Language (⌘L)")).clicked() {
            self.run_command(Command::ChooseLanguage);
        }
        // Project tabs fill the space between the menu and the controls.
        let tabs_right = rb.min_rect().left() - 16.0;
        if tabs_right > tabs_left + 60.0 {
            self.project_tab_strip(ui, Rect::from_min_max(pos2(tabs_left, row.top()), pos2(tabs_right, row.bottom())));
        }
    }

    /// Mac-style project tabs: flat rectangles, the selected one a lighter tint.
    fn project_tab_strip(&mut self, ui: &mut Ui, area: Rect) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let infos = self.project_tabs();
        let n = infos.len().max(1) as f32;
        let plus_w = 28.0;
        let avail = (area.width() - plus_w - 6.0).max(0.0);
        let tab_w = (avail / n).clamp(60.0, 200.0).min(avail / n);
        let h = 28.0;
        let top = area.center().y - h / 2.0;
        let selected_bg = crate::theme::mix(tk.window, theme.text, if theme.dark { 0.085 } else { 0.07 });
        let hover_bg = crate::theme::mix(tk.window, theme.text, if theme.dark { 0.045 } else { 0.04 });
        let font = ui::medium(13.0);
        let mut switch = None;
        let mut close = None;
        let mut reveal = None;
        let mut reorder = None;
        let hovered_tab = ui.ctx().pointer_hover_pos().and_then(|p| {
            (0..infos.len()).find(|&k| Rect::from_min_size(pos2(area.left() + k as f32 * tab_w, top), vec2(tab_w, h)).contains(p))
        });
        for (k, info) in infos.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.left() + k as f32 * tab_w, top), vec2(tab_w, h)).shrink2(vec2(1.0, 0.0));
            let resp = ui.interact(r, Id::unique(("project-tab", info.id)), Sense::click_and_drag());
            // Drag to reorder: the tab trades places as the pointer crosses into a neighbor.
            if resp.dragged()
                && let Some(p) = resp.interact_pointer_pos()
            {
                let target = (((p.x - area.left()) / tab_w).floor().max(0.0) as usize).min(infos.len() - 1);
                if target != k {
                    reorder = Some((k, target));
                }
            }
            let selected = k == self.current_project;
            let hovered = hovered_tab == Some(k);
            if selected {
                ui.painter().rect_filled(r, CornerRadius::same(7), selected_bg);
            } else if hovered {
                ui.painter().rect_filled(r, CornerRadius::same(7), hover_bg);
            }
            // Hairline separators between plain tabs, like Finder and Safari.
            let next_plain = k + 1 < infos.len() && k + 1 != self.current_project && hovered_tab != Some(k + 1);
            if !selected && !hovered && next_plain {
                ui.painter().vline(r.right() + 1.0, (r.top() + 7.0)..=(r.bottom() - 7.0), Stroke::new(1.0, tk.subtle_border));
            }
            // Close button (left, on hover) or the unsaved dot.
            let close_r = Rect::from_center_size(pos2(r.left() + 15.0, r.center().y), vec2(18.0, 18.0));
            let close_resp = ui.interact(close_r, Id::unique(("project-tab-close", info.id)), Sense::click());
            // The last empty project has nothing to close.
            let closable = infos.len() > 1 || info.root.is_some() || !self.tabs.is_empty();
            if hovered && closable {
                if close_resp.hovered() {
                    ui.painter().rect_filled(close_r, CornerRadius::same(4), with_alpha(theme.text, 22));
                }
                ui.painter().text(close_r.center(), Align2::CENTER_CENTER, ic::CLOSE, ui::icon_font(13.0), theme.text_muted);
                if close_resp.clicked() {
                    close = Some(k);
                }
            } else if info.dirty {
                ui.painter().circle_filled(close_r.center(), 3.5, theme.modified);
            }
            let color = if selected { theme.text } else { theme.text_muted };
            let galley = ui.painter().layout_no_wrap(info.label.clone(), font.clone(), color);
            let max_w = r.width() - 2.0 * 30.0;
            if galley.size().x <= max_w {
                ui.painter().galley(pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, color);
            } else {
                widgets::truncated_label(ui, pos2(r.left() + 30.0, r.center().y), &info.label, font.clone(), color, max_w);
            }
            let resp = match &info.root {
                Some(root) => resp.on_hover_text(crate::config::tildify(root)),
                None => resp,
            };
            // Picking a tab up selects it, as in Safari.
            if (resp.clicked() || resp.drag_started()) && close.is_none() {
                switch = Some(k);
            }
            if resp.middle_clicked() {
                close = Some(k);
            }
            resp.context_menu(|ui| {
                if ui.button("Close Project").clicked() {
                    close = Some(k);
                    ui.close();
                }
                if let Some(root) = &info.root
                    && ui.button(crate::platform::REVEAL_LABEL).clicked()
                {
                    reveal = Some(root.clone());
                    ui.close();
                }
            });
        }
        // New project: recent folders and Open Folder.
        let plus = Rect::from_min_size(pos2(area.left() + infos.len() as f32 * tab_w + 4.0, area.center().y - 14.0), vec2(plus_w, 28.0));
        let plus_resp = ui.interact(plus, Id::unique("project-tab-new"), Sense::click()).on_hover_text("Open a project in a new tab");
        if plus_resp.hovered() {
            ui.painter().rect_filled(plus, CornerRadius::same(7), hover_bg);
        }
        ui.painter().text(plus.center(), Align2::CENTER_CENTER, ic::ADD, ui::icon_font(18.0), theme.text_muted);
        egui::Popup::menu(&plus_resp).width(300.0).show(|ui| self.project_menu(ui));
        if let Some(k) = switch {
            self.switch_project(k);
        }
        if let Some((from, to)) = reorder {
            self.move_project(from, to);
        }
        if let Some(k) = close {
            self.close_project(k, false);
        }
        if let Some(p) = reveal {
            crate::platform::reveal(&p);
        }
    }

    fn project_menu(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        ui.label(egui::RichText::new("RECENT PROJECTS").size(11.0).color(theme.text_placeholder));
        let open: Vec<_> = self.project_tabs().into_iter().filter_map(|t| t.root).collect();
        let recents: Vec<_> = self.recents.projects.iter().filter(|p| !open.contains(&p.path)).take(10).cloned().collect();
        if recents.is_empty() {
            ui.label(egui::RichText::new("No recent projects").color(theme.text_muted));
        }
        for p in recents {
            let name = p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let resp = ui.add(egui::Button::new(egui::RichText::new(&name)).right_text(egui::RichText::new(crate::config::relative_time(p.opened_at)).color(theme.text_placeholder).size(12.0)));
            if resp.on_hover_text(crate::config::tildify(&p.path)).clicked() {
                self.open_folder(&p.path);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Open Folder…").clicked() {
            ui.close();
            self.run_command(Command::OpenFolder);
        }

    }

    fn app_menu(&mut self, ui: &mut Ui) {
        let groups = ["File", "Racket", "View", "Go", "App"];
        for g in groups {
            ui.menu_button(g, |ui| {
                ui.set_min_width(260.0);
                for c in crate::commands::COMMANDS.iter().filter(|c| c.group == g) {
                    let mut b = egui::Button::new(c.label);
                    if let Some(s) = &c.shortcut {
                        b = b.shortcut_text(crate::commands::shortcut_text(s));
                    }
                    if ui.add(b).clicked() {
                        ui.close();
                        self.run_command(c.cmd);
                    }
                }
            });
        }
    }

    pub(crate) fn tab_strip(&mut self, ui: &mut Ui, rect: Rect) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        ui.painter().hline(rect.left() + 1.0..=rect.right() - 1.0, rect.bottom() - 0.5, Stroke::new(1.0, tk.island_border));
        let inner = Rect::from_min_max(rect.min + vec2(6.0, 4.0), rect.max - vec2(70.0, 4.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
        child.set_clip_rect(inner);
        let mut close = None;
        let mut activate = None;
        let mut rects: Vec<Rect> = Vec::with_capacity(self.tabs.len());
        let mut dragging: Option<(usize, f32)> = None;
        let mut dropped: Option<(usize, f32)> = None;
        egui::ScrollArea::horizontal().id_salt("tabs-scroll").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (i, t) in self.tabs.iter().enumerate() {
                let title = t.title();
                let icon = file_icons::image(file_icons::file_icon(&title), 16.0);
                let r = widgets::tab(ui, &theme, TabIcon::Image(icon), &title, i == self.active, t.dirty(), true, t.path.is_none());
                rects.push(r.response.rect);
                if r.clicked {
                    activate = Some(i);
                }
                if r.close {
                    close = Some(t.id);
                }
                if let Some(x) = r.response.interact_pointer_pos().map(|p| p.x) {
                    if r.response.dragged() {
                        dragging = Some((i, x));
                    }
                    if r.response.drag_stopped() {
                        dropped = Some((i, x));
                    }
                }
                if let Some(p) = &t.path {
                    r.response.on_hover_text(crate::config::tildify(p));
                }
            }
        });
        // Where a dragged tab would land: before the first tab whose center is right of the pointer.
        let slot = |x: f32| rects.iter().position(|r| x < r.center().x).unwrap_or(rects.len());
        if let Some((from, x)) = dragging {
            let to = slot(x);
            if to != from && to != from + 1 {
                let lx = if to < rects.len() { rects[to].left() - 1.0 } else { rects.last().map(|r| r.right() + 1.0).unwrap_or(inner.left()) };
                ui.painter().vline(lx, rect.top() + 8.0..=rect.bottom() - 8.0, Stroke::new(2.0, theme.text_accent));
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if let Some((from, x)) = dropped {
            let to = slot(x);
            if to != from && to != from + 1 {
                let active_id = self.tabs.get(self.active).map(|t| t.id);
                let tab = self.tabs.remove(from);
                let to = if to > from { to - 1 } else { to };
                self.tabs.insert(to, tab);
                if let Some(id) = active_id {
                    self.active = self.tabs.iter().position(|t| t.id == id).unwrap_or(0);
                }
            }
        }
        if let Some(i) = activate {
            self.active = i;
            self.tabs[i].editor.request_focus();
        }
        if let Some(id) = close {
            self.close_tab(id, false);
        }
        // Trailing actions.
        let actions = Rect::from_min_max(pos2(rect.right() - 68.0, rect.top()), rect.max);
        let mut a = ui.new_child(UiBuilder::new().max_rect(actions).layout(egui::Layout::right_to_left(egui::Align::Center)));
        a.add_space(6.0);
        let more = icon_button(&mut a, &theme, ic::MORE_VERT, "");
        egui::Popup::menu(&more).width(220.0).show(|ui| {
            for (cmd, label) in [(Command::NewFile, "New File"), (Command::Save, "Save"), (Command::SaveAll, "Save All"), (Command::CloseTab, "Close Tab"), (Command::RevealInFinder, crate::platform::REVEAL_LABEL)] {
                if ui.button(label).clicked() {
                    ui.close();
                    self.run_command(cmd);
                }
            }
        });
        let list = icon_button(&mut a, &theme, ic::EXPAND_MORE, "Open Tabs");
        egui::Popup::menu(&list).width(260.0).show(|ui| {
            for i in 0..self.tabs.len() {
                let title = self.tabs[i].title();
                let img = file_icons::image(file_icons::file_icon(&title), 16.0);
                if ui.add(egui::Button::image_and_text(img, title).selected(i == self.active)).clicked() {
                    self.active = i;
                    ui.close();
                }
            }
        });
        let _ = TAB_BAR_H;
    }

    pub(crate) fn status_bar(&mut self, ui: &mut Ui, rect: Rect) {
        let theme = self.theme.clone();
        let mut bar = ui.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        bar.spacing_mut().item_spacing.x = 2.0;
        // Vim's mode, at the start of the bar as in Neovim's statusline.
        if let Some((label, color, extra)) = self.active_tab().and_then(|t| t.editor.vim_status(&theme)) {
            let fg = if theme.dark { theme.editor_bg } else { egui::Color32::WHITE };
            let g = bar.painter().layout_no_wrap(label.to_string(), ui::semibold(11.0), fg);
            let (r, _) = bar.allocate_exact_size(vec2(g.size().x + 16.0, 18.0), Sense::hover());
            bar.painter().rect_filled(r, CornerRadius::same(5), color);
            bar.painter().galley(pos2(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0), g, fg);
            for (text, c) in extra {
                bar.add_space(8.0);
                bar.label(egui::RichText::new(text).font(ui::mono(12.5)).color(c));
            }
            bar.add_space(10.0);
        }
        let has_terminal = !self.terminals.is_empty();
        if icon_button_sized(&mut bar, &theme, ic::TERMINAL, &crate::platform::keys("Terminal (⌃`)"), 26.0, 17.0, None, self.layout.bottom.open && has_terminal).clicked() {
            match self.terminals.first().map(|t| t.id) {
                Some(id) if self.layout.is_visible(PanelItem::Terminal(id)) => self.run_command(Command::ToggleBottomPanel),
                Some(id) => self.layout.show(PanelItem::Terminal(id), Dock::Bottom),
                None => self.run_command(Command::NewTerminal),
            }
        }
        let (errors, warnings) = self
            .active_tab()
            .map(|t| {
                let d = &t.editor.deco.diagnostics;
                (d.iter().filter(|x| x.severity == crate::editor::view::Severity::Error).count(), d.iter().filter(|x| x.severity == crate::editor::view::Severity::Warning).count())
            })
            .unwrap_or((0, 0));
        let problems_color = if errors > 0 { Some(theme.error) } else { None };
        if icon_button_sized(&mut bar, &theme, if errors > 0 { ic::ERROR } else { ic::CHECK_CIRCLE }, &format!("{errors} errors, {warnings} warnings"), 26.0, 17.0, problems_color, false).clicked() {
            self.layout.show(PanelItem::Problems, Dock::Bottom);
        }
        if icon_button_sized(&mut bar, &theme, ic::TOC, "Outline", 26.0, 17.0, None, self.layout.is_visible(PanelItem::Outline)).clicked() {
            self.run_command(Command::ShowOutline);
        }
        bar.add_space(14.0);

        // Breadcrumbs: project › folders › file › current definition.
        let crumb_font = ui::ui_font(13.5);
        let chevron = |ui: &mut Ui| {
            ui.label(egui::RichText::new(ic::CHEVRON_RIGHT).font(ui::icon_font(16.0)).color(theme.text_placeholder));
        };
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let mut parts: Vec<String> = Vec::new();
            if let (Some(ws), Some(p)) = (&self.workspace, &tab.path) {
                parts.push(ws.name.clone());
                if let Ok(rel) = p.strip_prefix(&ws.root)
                    && let Some(parent) = rel.parent()
                {
                    parts.extend(parent.components().map(|c| c.as_os_str().to_string_lossy().into_owned()));
                }
            }
            for p in &parts {
                bar.label(egui::RichText::new(p).font(crumb_font.clone()).color(theme.text_muted));
                chevron(&mut bar);
            }
            let title = tab.title();
            let img = file_icons::image(file_icons::file_icon(&title), 15.0);
            bar.add(img);
            bar.label(egui::RichText::new(&title).font(crumb_font.clone()).color(theme.text));
            // Current top-level definition.
            let head = tab.editor.doc.buffer.selections.primary().head;
            let outline = tab.editor.doc.outline();
            if let Some(def) = outline.iter().rfind(|o| o.level == 0 && o.pos <= head) {
                chevron(&mut bar);
                bar.label(egui::RichText::new(ic::FUNCTION).font(ui::icon_font(15.0)).color(theme.syntax.function.color));
                bar.label(egui::RichText::new(&def.name).font(crumb_font.clone()).color(theme.text_muted));
            }
        }

        // Right side.
        let mut rb = bar.new_child(UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 0.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        rb.spacing_mut().item_spacing.x = 14.0;
        self.update_pill(&mut rb);
        let status = crate::racket_ui::status_text(self);
        if !status.0.is_empty() {
            rb.label(egui::RichText::new(&status.0).font(ui::ui_font(12.5)).color(status.1));
        }
        if let Some(tab) = self.active_tab() {
            let b = &tab.editor.doc.buffer;
            let p = b.selections.primary();
            // Vim's visual selection ends past its cursor.
            let head = tab.editor.vim.as_ref().and_then(|v| v.cursor_pos()).unwrap_or(p.head);
            let line = b.line_of(head);
            let col = b.col_of(head);
            let sel = if p.is_empty() { String::new() } else { format!(" ({} selected)", p.end() - p.start()) };
            let cursors = if b.selections.list.len() > 1 { format!("{} cursors  ", b.selections.list.len()) } else { String::new() };
            let pos = rb.add(egui::Label::new(egui::RichText::new(format!("{cursors}Ln {}, Col {}{sel}", line + 1, col + 1)).font(ui::ui_font(12.5)).color(theme.text_muted)).sense(Sense::click()));
            if pos.on_hover_text(crate::platform::keys("Go to Line (⌃G)")).clicked() {
                self.run_command(Command::GoToLine);
            }
        }
    }

    /// At the right end of the status bar: Restart to Update once a new version is downloaded,
    /// or Update (to the website) when this copy can't install it itself.
    fn update_pill(&mut self, ui: &mut Ui) {
        use crate::update::Status;
        let theme = self.theme.clone();
        let (glyph, label, tip) = match self.updater.status() {
            Status::Ready { version } => (ic::RESTART_ALT, "Restart to Update", format!("Dr. Rocket {version} is downloaded. Restart to start using it (it also installs when you quit).")),
            Status::Available { version, why } => (ic::DOWNLOAD, "Update", format!("Dr. Rocket {version} is available. {why}")),
            _ => return,
        };
        let font = ui::medium(12.5);
        let g = ui.painter().layout_no_wrap(label.to_string(), font, egui::Color32::WHITE);
        let (r, resp) = ui.allocate_exact_size(vec2(8.0 + 16.0 + 4.0 + g.size().x + 10.0, 20.0), Sense::click());
        let fill = if resp.is_pointer_button_down_on() {
            crate::theme::mix(theme.text_accent, egui::Color32::BLACK, 0.2)
        } else if resp.hovered() {
            crate::theme::mix(theme.text_accent, egui::Color32::WHITE, 0.12)
        } else {
            theme.text_accent
        };
        ui.painter().rect_filled(r, CornerRadius::same(6), fill);
        ui.painter().text(pos2(r.left() + 8.0 + 8.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(15.0), egui::Color32::WHITE);
        ui.painter().galley(pos2(r.left() + 28.0, r.center().y - g.size().y / 2.0), g, egui::Color32::WHITE);
        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if self.updater.ready().is_some() {
                self.run_command(Command::RestartToUpdate);
            } else {
                let _ = open::that(crate::update::download_page());
            }
        }
    }
}
