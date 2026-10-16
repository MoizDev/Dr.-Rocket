//! Welcome screen: quick actions and recent projects/files.

use crate::app::App;
use crate::commands::{Command, info, shortcut_text};
use crate::config::{relative_time, tildify};
use crate::theme::with_alpha;
use crate::ui::{self, file_icons, ic};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

pub use crate::ui::brand::APP_NAME;

impl App {
    pub(crate) fn welcome_ui(&mut self, ui: &mut Ui, rect: Rect) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let col_w = 560.0_f32.min(rect.width() - 48.0);
        let left = rect.center().x - col_w / 2.0;
        let mut y = rect.top() + (rect.height() * 0.12).max(36.0);

        let logo_h = 60.0;
        let logo = crate::ui::brand::logo(logo_h, theme.dark);
        let logo_w = logo.size().map(|s| s.x).unwrap_or(logo_h * 0.65);
        logo.paint_at(ui, Rect::from_min_size(pos2(left, y - 2.0), vec2(logo_w, logo_h)));
        let tx = left + logo_w + 18.0;
        ui.painter().text(pos2(tx, y + 14.0), Align2::LEFT_CENTER, APP_NAME, ui::semibold(26.0), theme.text);
        ui.painter().text(pos2(tx, y + 42.0), Align2::LEFT_CENTER, "A fast, modern IDE for Racket", ui::ui_font(14.5), theme.text_muted);
        y += 92.0;

        // Quick actions as cards in a 2x2 grid.
        let actions = [
            (Command::NewFile, ic::NOTE_ADD, "New File"),
            (Command::OpenFile, ic::FILE_OPEN, "Open File"),
            (Command::OpenFolder, ic::FOLDER_OPEN, "Open Folder"),
            (Command::Palette, ic::KEYBOARD_COMMAND_KEY, "Command Palette"),
        ];
        let card_w = (col_w - 10.0) / 2.0;
        let card_h = 52.0;
        let mut clicked = None;
        for (k, (cmd, glyph, label)) in actions.iter().enumerate() {
            let cx = left + (k % 2) as f32 * (card_w + 10.0);
            let cy = y + (k / 2) as f32 * (card_h + 10.0);
            let r = Rect::from_min_size(pos2(cx, cy), vec2(card_w, card_h));
            let resp = ui.interact(r, Id::unique(("welcome-action", k)), Sense::click());
            let bg = if resp.hovered() { tk.hover } else { with_alpha(tk.hover, 110) };
            ui.painter().rect(r, CornerRadius::same(10), bg, Stroke::new(1.0, tk.island_border), StrokeKind::Inside);
            ui.painter().text(pos2(r.left() + 24.0, r.center().y), Align2::CENTER_CENTER, *glyph, ui::icon_font(20.0), theme.text_accent);
            ui.painter().text(pos2(r.left() + 44.0, r.center().y), Align2::LEFT_CENTER, *label, ui::medium(14.0), theme.text);
            if let Some(s) = &info(*cmd).shortcut {
                ui.painter().text(pos2(r.right() - 14.0, r.center().y), Align2::RIGHT_CENTER, shortcut_text(s), ui::medium(12.5), theme.text_placeholder);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(*cmd);
            }
        }
        y += 2.0 * (card_h + 10.0) + 22.0;

        // Recent projects.
        let projects: Vec<_> = self.recents.projects.iter().take(6).cloned().collect();
        let files: Vec<_> = self.recents.files.iter().take(6).cloned().collect();
        let mut open_project = None;
        let mut open_file = None;
        let section = |ui: &Ui, y: f32, title: &str| {
            ui.painter().text(pos2(left + 2.0, y), Align2::LEFT_CENTER, title, ui::semibold(11.5), theme.text_placeholder);
        };
        if !projects.is_empty() {
            section(ui, y, "RECENT PROJECTS");
            y += 16.0;
            for (k, p) in projects.iter().enumerate() {
                let r = Rect::from_min_size(pos2(left - 8.0, y), vec2(col_w + 16.0, 34.0));
                let resp = ui.interact(r, Id::unique(("recent-project", k)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(8), tk.hover);
                }
                let name = p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                file_icons::image(file_icons::folder_icon(&name, false), 18.0).paint_at(ui, Rect::from_center_size(pos2(left + 10.0, r.center().y), vec2(18.0, 18.0)));
                ui.painter().text(pos2(left + 30.0, r.center().y), Align2::LEFT_CENTER, &name, ui::medium(14.0), theme.text);
                let nw = ui.painter().layout_no_wrap(name.clone(), ui::medium(14.0), theme.text).size().x;
                crate::ui::widgets::truncated_label(ui, pos2(left + 30.0 + nw + 10.0, r.center().y), &tildify(&p.path), ui::ui_font(13.0), theme.text_placeholder, col_w - nw - 150.0);
                ui.painter().text(pos2(left + col_w, r.center().y), Align2::RIGHT_CENTER, relative_time(p.opened_at), ui::ui_font(12.5), theme.text_placeholder);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open_project = Some(p.path.clone());
                }
                y += 34.0;
            }
            y += 16.0;
        }
        if !files.is_empty() && y + 60.0 < rect.bottom() {
            section(ui, y, "RECENT FILES");
            y += 16.0;
            for (k, p) in files.iter().enumerate() {
                if y + 34.0 > rect.bottom() - 10.0 {
                    break;
                }
                let r = Rect::from_min_size(pos2(left - 8.0, y), vec2(col_w + 16.0, 32.0));
                let resp = ui.interact(r, Id::unique(("recent-file", k)), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(8), tk.hover);
                }
                let name = p.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                file_icons::image(file_icons::file_icon(&name), 16.0).paint_at(ui, Rect::from_center_size(pos2(left + 10.0, r.center().y), vec2(16.0, 16.0)));
                ui.painter().text(pos2(left + 30.0, r.center().y), Align2::LEFT_CENTER, &name, ui::ui_font(14.0), theme.text);
                let parent = p.path.parent().map(tildify).unwrap_or_default();
                let nw = ui.painter().layout_no_wrap(name.clone(), ui::ui_font(14.0), theme.text).size().x;
                crate::ui::widgets::truncated_label(ui, pos2(left + 30.0 + nw + 10.0, r.center().y), &parent, ui::ui_font(13.0), theme.text_placeholder, col_w - nw - 60.0);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open_file = Some(p.path.clone());
                }
                y += 32.0;
            }
        }
        if projects.is_empty() && files.is_empty() {
            ui.painter().text(pos2(left + 2.0, y + 6.0), Align2::LEFT_CENTER, "Recent projects will show up here.", ui::ui_font(13.5), theme.text_placeholder);
        }
        if let Some(c) = clicked {
            self.run_command(c);
        }
        if let Some(p) = open_project {
            self.open_folder(&p);
        }
        if let Some(p) = open_file {
            self.open_file(&p);
        }
    }
}
