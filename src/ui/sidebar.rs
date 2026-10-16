//! The left island: Files (Material icons), Outline (definitions), and project Search.

use crate::app::App;
use crate::commands::Command;
use crate::editor::document::OutlineKind;
use crate::theme::with_alpha;
use crate::ui::widgets::{empty_state, search_field, truncated_label};
use crate::ui::{self, file_icons, ic};
use egui::{Align2, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use std::path::PathBuf;

#[derive(Default)]
pub struct SidebarState {
    pub search_query: String,
    pub search_results: Vec<SearchHit>,
    search_key: String,
    pub focus_search: bool,
}

enum TreeAction {
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Trash(PathBuf),
    CopyRelative(PathBuf),
    Terminal(PathBuf),
}

#[derive(Clone)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    pub len: usize,
    pub text: String,
}

const ROW_H: f32 = 28.0;
/// File tree rows: packed edge to edge like Zed's project panel.
const FILE_ROW_H: f32 = 26.0;

impl App {
    pub(crate) fn new_file_in_project(&mut self) {
        let Some(ws) = &self.workspace else { return };
        if let Some(p) = rfd::FileDialog::new().set_directory(&ws.root).set_file_name("untitled.rkt").save_file()
            && std::fs::write(&p, "#lang racket\n\n").is_ok() {
                self.open_file(&p);
            }
    }

    pub(crate) fn files_view(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let Some(ws) = &mut self.workspace else {
            empty_state(ui, &theme, ic::FOLDER_OPEN, "No folder open", "");
            let r = ui.available_rect_before_wrap();
            let br = Rect::from_center_size(r.center() + vec2(0.0, 40.0), vec2(150.0, 32.0));
            let resp = ui.interact(br, Id::unique("open-folder-btn"), Sense::click());
            ui.painter().rect_filled(br, CornerRadius::same(8), if resp.hovered() { tk.pressed } else { tk.hover });
            ui.painter().text(br.center(), Align2::CENTER_CENTER, "Open Folder…", ui::medium(13.5), theme.text);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.run_command(Command::OpenFolder);
            }
            return;
        };
        let project = ws.name.to_uppercase();
        let (hr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
        ui.painter().text(pos2(hr.left() + 8.0, hr.center().y), Align2::LEFT_CENTER, project, ui::semibold(11.5), theme.text_placeholder);
        let rows = ws.rows().to_vec();
        let active_path = self.tabs.get(self.active).and_then(|t| t.path.clone());
        let dirty: std::collections::HashSet<PathBuf> = self.tabs.iter().filter(|t| t.dirty()).filter_map(|t| t.path.clone()).collect();
        let mut toggle = None;
        let mut open = None;
        let mut action: Option<TreeAction> = None;
        let width = ui.available_width();
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, false]).show_rows(ui, FILE_ROW_H, rows.len(), |ui, range| {
            for row in &rows[range] {
                let (r, resp) = ui.allocate_exact_size(vec2(width, FILE_ROW_H), Sense::click());
                let selected = active_path.as_ref() == Some(&row.entry.path);
                if selected {
                    ui.painter().rect_filled(r, CornerRadius::same(6), tk.selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(6), tk.hover);
                }
                let indent = 8.0 + row.depth as f32 * 14.0;
                // Indent guides.
                for d in 0..row.depth {
                    let x = r.left() + 8.0 + d as f32 * 14.0 + 7.0;
                    ui.painter().vline(x, r.y_range(), Stroke::new(1.0, with_alpha(theme.indent_guide, 160)));
                }
                let mut x = r.left() + indent;
                if row.entry.is_dir {
                    let chev = if row.expanded { ic::EXPAND_MORE } else { ic::CHEVRON_RIGHT };
                    ui.painter().text(pos2(x + 7.0, r.center().y), Align2::CENTER_CENTER, chev, ui::icon_font(16.0), theme.icon_muted);
                }
                x += 16.0;
                let icon = if row.entry.is_dir { file_icons::folder_icon(&row.entry.name, row.expanded) } else { file_icons::file_icon(&row.entry.name) };
                file_icons::image(icon, 17.0).paint_at(ui, Rect::from_center_size(pos2(x + 8.5, r.center().y), vec2(17.0, 17.0)));
                x += 22.0;
                let is_dirty = dirty.contains(&row.entry.path);
                let color = if selected { theme.text } else if is_dirty { theme.modified } else { crate::theme::mix(theme.text, theme.text_muted, 0.35) };
                truncated_label(ui, pos2(x, r.center().y), &row.entry.name, ui::ui_font(13.5), color, r.right() - x - 14.0);
                if is_dirty {
                    ui.painter().circle_filled(pos2(r.right() - 10.0, r.center().y), 3.0, theme.modified);
                }
                if resp.clicked() {
                    if row.entry.is_dir {
                        toggle = Some(row.entry.path.clone());
                    } else {
                        open = Some(row.entry.path.clone());
                    }
                }
                let entry = row.entry.clone();
                resp.context_menu(|ui| {
                    ui.set_min_width(220.0);
                    let dir = if entry.is_dir { entry.path.clone() } else { entry.path.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
                    if ui.button("New File…").clicked() {
                        action = Some(TreeAction::NewFile(dir.clone()));
                        ui.close();
                    }
                    if ui.button("New Folder…").clicked() {
                        action = Some(TreeAction::NewFolder(dir.clone()));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rename…").clicked() {
                        action = Some(TreeAction::Rename(entry.path.clone()));
                        ui.close();
                    }
                    if ui.button("Move to Trash").clicked() {
                        action = Some(TreeAction::Trash(entry.path.clone()));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Copy Path").clicked() {
                        ui.ctx().copy_text(entry.path.display().to_string());
                        ui.close();
                    }
                    if ui.button("Copy Relative Path").clicked() {
                        action = Some(TreeAction::CopyRelative(entry.path.clone()));
                        ui.close();
                    }
                    if ui.button("Reveal in Finder").clicked() {
                        let _ = std::process::Command::new("open").arg("-R").arg(&entry.path).spawn();
                        ui.close();
                    }
                    if ui.button("Open Terminal Here").clicked() {
                        action = Some(TreeAction::Terminal(dir.clone()));
                        ui.close();
                    }
                });
            }
        });
        ui.spacing_mut().item_spacing.y = spacing;
        if let Some(p) = toggle
            && let Some(ws) = &mut self.workspace
        {
            ws.toggle(&p);
        }
        if let Some(p) = open {
            self.open_file(&p);
        }
        match action {
            Some(TreeAction::NewFile(d)) => self.modal = crate::app::Modal::Prompt(crate::ui::language_dialog::Prompt::new_file(d)),
            Some(TreeAction::NewFolder(d)) => self.modal = crate::app::Modal::Prompt(crate::ui::language_dialog::Prompt::new_folder(d)),
            Some(TreeAction::Rename(p)) => self.modal = crate::app::Modal::Prompt(crate::ui::language_dialog::Prompt::rename_path(p)),
            Some(TreeAction::Trash(p)) => match trash::delete(&p) {
                Ok(()) => {
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.toast(format!("Moved “{name}” to the Trash"), crate::app::ToastKind::Info);
                    self.refresh_tree();
                }
                Err(e) => self.toast(format!("Could not move to Trash: {e}"), crate::app::ToastKind::Error),
            },
            Some(TreeAction::CopyRelative(p)) => {
                let rel = self.workspace.as_ref().map(|w| w.relative(&p)).unwrap_or(p);
                ui.ctx().copy_text(rel.display().to_string());
            }
            Some(TreeAction::Terminal(d)) => self.terminal_at(&d, crate::app::Dock::Bottom),
            None => {}
        }
    }

    pub(crate) fn outline_view(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            empty_state(ui, &theme, ic::TOC, "No file open", "");
            return;
        };
        let items = tab.editor.doc.outline();
        if items.is_empty() {
            empty_state(ui, &theme, ic::TOC, "No definitions", "Top-level defines appear here");
            return;
        }
        let head = tab.editor.doc.buffer.selections.primary().head;
        let current = items.iter().rposition(|i| i.pos <= head);
        let width = ui.available_width();
        let mut jump = None;
        egui::ScrollArea::vertical().id_salt("outline").auto_shrink([false, false]).show(ui, |ui| {
            for (k, it) in items.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                if Some(k) == current {
                    ui.painter().rect_filled(r, CornerRadius::same(6), tk.selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(6), tk.hover);
                }
                let (glyph, color) = match it.kind {
                    OutlineKind::Function => (ic::FUNCTION, theme.syntax.function.color),
                    OutlineKind::Value => (ic::DATA_OBJECT, theme.syntax.variable.color),
                    OutlineKind::Macro => (ic::AUTO_FIX_HIGH, theme.syntax.keyword.color),
                    OutlineKind::Struct => (ic::SCHEMA, theme.syntax.type_.color),
                    OutlineKind::Module => (ic::DEPLOYED_CODE, theme.syntax.lang.color),
                    OutlineKind::Test => (ic::SCIENCE, theme.success),
                };
                let x = r.left() + 12.0 + it.level as f32 * 16.0;
                ui.painter().text(pos2(x + 8.0, r.center().y), Align2::CENTER_CENTER, glyph, ui::icon_font(16.0), color);
                let font = if it.kind == OutlineKind::Test { ui::mono(12.5) } else { ui::mono(13.5) };
                truncated_label(ui, pos2(x + 24.0, r.center().y), &it.name, font, if it.kind == OutlineKind::Test { theme.text_muted } else { theme.text }, r.right() - x - 30.0);
                if resp.clicked() {
                    jump = Some(it.name_pos);
                }
            }
        });
        if let Some(p) = jump
            && let Some(tab) = self.tabs.get_mut(self.active)
        {
            tab.editor.reveal(p..p);
            tab.editor.request_focus();
        }
    }

    pub(crate) fn search_view(&mut self, ui: &mut Ui) {
        let theme = self.theme.clone();
        let tk = self.tokens();
        let id = Id::unique("project-search");
        let w = ui.available_width();
        let resp = search_field(ui, &theme, &mut self.sidebar.search_query, "Search in project", id, w);
        if self.sidebar.focus_search {
            resp.request_focus();
            self.sidebar.focus_search = false;
        }
        ui.add_space(8.0);
        if self.sidebar.search_query != self.sidebar.search_key {
            self.sidebar.search_key = self.sidebar.search_query.clone();
            self.sidebar.search_results = self.project_search(&self.sidebar.search_query.clone());
        }
        let results = self.sidebar.search_results.clone();
        if self.sidebar.search_query.len() >= 2 && results.is_empty() {
            ui.label(egui::RichText::new("No results").color(theme.text_muted));
            return;
        }
        if !results.is_empty() {
            let files = results.iter().map(|r| &r.path).collect::<std::collections::HashSet<_>>().len();
            ui.label(egui::RichText::new(format!("{} results in {} files", results.len(), files)).size(12.0).color(theme.text_placeholder));
            ui.add_space(4.0);
        }
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("search-results").auto_shrink([false, false]).show(ui, |ui| {
            let mut last: Option<&PathBuf> = None;
            for hit in &results {
                if last != Some(&hit.path) {
                    last = Some(&hit.path);
                    let name = hit.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.add(file_icons::image(file_icons::file_icon(&name), 15.0));
                        ui.label(egui::RichText::new(name).font(ui::medium(13.0)).color(theme.text));
                    });
                }
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(5), tk.hover);
                }
                let mut job = egui::text::LayoutJob::default();
                let font = ui::mono(12.5);
                let chars: Vec<char> = hit.text.chars().collect();
                let s = hit.col.min(chars.len());
                let e = (hit.col + hit.len).min(chars.len());
                let pre: String = chars[s.saturating_sub(24)..s].iter().collect();
                let mid: String = chars[s..e].iter().collect();
                let post: String = chars[e..].iter().collect();
                job.append(pre.trim_start(), 0.0, egui::TextFormat { font_id: font.clone(), color: theme.text_muted, ..Default::default() });
                job.append(&mid, 0.0, egui::TextFormat { font_id: font.clone(), color: theme.text, background: theme.search_match, ..Default::default() });
                job.append(&post, 0.0, egui::TextFormat { font_id: font, color: theme.text_muted, ..Default::default() });
                job.wrap = egui::text::TextWrapping::truncate_at_width(r.width() - 24.0);
                let g = ui.painter().layout_job(job);
                ui.painter().galley(pos2(r.left() + 20.0, r.center().y - g.size().y / 2.0), g, theme.text);
                if resp.clicked() {
                    open = Some(hit.clone());
                }
            }
        });
        if let Some(hit) = open {
            self.open_file(&hit.path);
            if let Some(t) = self.tabs.get_mut(self.active) {
                let ls = t.editor.doc.buffer.line_start(hit.line);
                t.editor.reveal(ls + hit.col..ls + hit.col + hit.len);
            }
        }
    }

    /// Literal, case-insensitive search over the project's files (and open buffers).
    fn project_search(&self, q: &str) -> Vec<SearchHit> {
        if q.chars().count() < 2 {
            return Vec::new();
        }
        let ql = q.to_lowercase();
        let mut out = Vec::new();
        let mut paths: Vec<PathBuf> = match &self.workspace {
            Some(ws) => ws.index.lock().iter().map(|p| ws.root.join(p)).collect(),
            None => Vec::new(),
        };
        for t in &self.tabs {
            if let Some(p) = &t.path
                && !paths.contains(p)
            {
                paths.push(p.clone());
            }
        }
        for p in paths {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "rkt" | "rktl" | "rktd" | "scrbl" | "ss" | "scm" | "md" | "txt" | "json" | "toml") {
                continue;
            }
            let text = match self.tabs.iter().find(|t| t.path.as_ref() == Some(&p)) {
                Some(t) => t.editor.doc.text(),
                None => match std::fs::read_to_string(&p) {
                    Ok(s) if s.len() < 2_000_000 => s,
                    _ => continue,
                },
            };
            for (line, l) in text.lines().enumerate() {
                let lower = l.to_lowercase();
                if let Some(bi) = lower.find(&ql) {
                    let col = lower[..bi].chars().count();
                    out.push(SearchHit { path: p.clone(), line, col, len: q.chars().count(), text: l.to_string() });
                    if out.len() >= 2000 {
                        return out;
                    }
                }
            }
        }
        out
    }
}
