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
    /// The closed folder a drag is resting on, and since when: it opens after a moment.
    drag_hover: Option<(PathBuf, f64)>,
}

/// A file or folder being dragged in the Files tree.
struct TreeDrag {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

/// Whether `item` can be moved into the folder `dir`: not into itself or a folder inside it,
/// and not where it already is.
fn can_move(item: &std::path::Path, dir: &std::path::Path) -> bool {
    !dir.starts_with(item) && item.parent() != Some(dir)
}

/// How long a drag rests on a closed folder before it opens.
const DRAG_OPEN_DELAY: f64 = 0.6;

/// The menu for the project itself (its header, and the empty space below the files).
fn root_menu(ui: &mut Ui, root: &std::path::Path, action: &mut Option<TreeAction>) {
    ui.set_min_width(220.0);
    if ui.button("New File…").clicked() {
        *action = Some(TreeAction::NewFile(root.to_path_buf()));
        ui.close();
    }
    if ui.button("New Folder…").clicked() {
        *action = Some(TreeAction::NewFolder(root.to_path_buf()));
        ui.close();
    }
    ui.separator();
    if ui.button(crate::platform::REVEAL_LABEL).clicked() {
        crate::platform::reveal(root);
        ui.close();
    }
    if ui.button("Open Terminal Here").clicked() {
        *action = Some(TreeAction::Terminal(root.to_path_buf()));
        ui.close();
    }
}

enum TreeAction {
    NewFile(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Trash(PathBuf),
    CopyRelative(PathBuf),
    Terminal(PathBuf),
    /// Move the first path into the folder that is the second.
    Move(PathBuf, PathBuf),
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
    /// New file in the project, named in the app's own prompt: next to the file being edited
    /// (when it's in the project), otherwise at the project root.
    pub(crate) fn new_file_in_project(&mut self) {
        let Some(ws) = &self.workspace else { return };
        let dir = self
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.parent())
            .filter(|d| d.starts_with(&ws.root))
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| ws.root.clone());
        self.modal = crate::app::Modal::Prompt(crate::ui::language_dialog::Prompt::new_file(dir));
    }

    /// New folder in the project, next to the file being edited (or at the top).
    pub(crate) fn new_folder_in_project(&mut self) {
        let Some(ws) = &self.workspace else {
            self.toast("Open a folder first, then make folders in it", crate::app::ToastKind::Info);
            return;
        };
        let dir = self
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .and_then(|p| p.parent())
            .filter(|d| d.starts_with(&ws.root))
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| ws.root.clone());
        self.modal = crate::app::Modal::Prompt(crate::ui::language_dialog::Prompt::new_folder(dir));
    }

    /// Move a file or folder into another folder of the project (dragged there in the tree).
    fn move_into(&mut self, from: &std::path::Path, dir: &std::path::Path) {
        let Some(name) = from.file_name() else { return };
        if !can_move(from, dir) {
            return;
        }
        let to = dir.join(name);
        let shown = |p: &std::path::Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
        if to.symlink_metadata().is_ok() {
            self.toast(format!("There's already a “{}” in “{}”", shown(from), shown(dir)), crate::app::ToastKind::Error);
            return;
        }
        match std::fs::rename(from, &to) {
            Ok(()) => {
                self.path_moved(from, &to);
                if let Some(ws) = &mut self.workspace {
                    ws.expand_to(&to);
                }
            }
            Err(e) => self.toast(format!("Could not move “{}”: {e}", shown(from)), crate::app::ToastKind::Error),
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
        let (hr, header) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        let root = ws.root.clone();
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
        header.context_menu(|ui| root_menu(ui, &root, &mut action));
        let ctx = ui.ctx().clone();
        let drag = egui::DragAndDrop::payload::<TreeDrag>(&ctx);
        let pointer = ctx.pointer_hover_pos();
        // The row under the pointer while dragging, and where each visible row is.
        let mut drag_over: Option<usize> = None;
        let mut row_rects: Vec<(usize, Rect)> = Vec::new();
        let scrolled = egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, false]).show_rows(ui, FILE_ROW_H, rows.len(), |ui, range| {
            let first = range.start;
            for (k, row) in rows[range].iter().enumerate() {
                // Rows are known by their path, not their place: opening a folder mid-drag
                // shifts the rows below it, and the drag must stay with its item.
                let (r, _) = ui.allocate_exact_size(vec2(width, FILE_ROW_H), Sense::hover());
                let resp = ui.interact(r, ui.scope_id().with(&row.entry.path), Sense::click_and_drag());
                row_rects.push((first + k, r));
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(&ctx, TreeDrag { path: row.entry.path.clone(), name: row.entry.name.clone(), is_dir: row.entry.is_dir });
                }
                if drag.is_some() && resp.contains_pointer() {
                    drag_over = Some(first + k);
                }
                // The item being dragged stays in place, faded.
                let lifted = drag.as_ref().is_some_and(|d| d.path == row.entry.path);
                let fade = |c: egui::Color32| if lifted { c.gamma_multiply(0.45) } else { c };
                let selected = active_path.as_ref() == Some(&row.entry.path);
                if selected {
                    ui.painter().rect_filled(r, CornerRadius::same(6), tk.selected);
                } else if resp.hovered() && drag.is_none() {
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
                    ui.painter().text(pos2(x + 7.0, r.center().y), Align2::CENTER_CENTER, chev, ui::icon_font(16.0), fade(theme.icon_muted));
                }
                x += 16.0;
                let icon = if row.entry.is_dir { file_icons::folder_icon(&row.entry.name, row.expanded) } else { file_icons::file_icon(&row.entry.name) };
                file_icons::image(icon, 17.0).tint(fade(egui::Color32::WHITE)).paint_at(ui, Rect::from_center_size(pos2(x + 8.5, r.center().y), vec2(17.0, 17.0)));
                x += 22.0;
                let is_dirty = dirty.contains(&row.entry.path);
                let color = if selected { theme.text } else if is_dirty { theme.modified } else { crate::theme::mix(theme.text, theme.text_muted, 0.35) };
                truncated_label(ui, pos2(x, r.center().y), &row.entry.name, ui::ui_font(13.5), fade(color), r.right() - x - 14.0);
                if is_dirty {
                    ui.painter().circle_filled(pos2(r.right() - 10.0, r.center().y), 3.0, fade(theme.modified));
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
                    if ui.button(crate::platform::REVEAL_LABEL).clicked() {
                        crate::platform::reveal(&entry.path);
                        ui.close();
                    }
                    if ui.button("Open Terminal Here").clicked() {
                        action = Some(TreeAction::Terminal(dir.clone()));
                        ui.close();
                    }
                });
            }
            // Dragging near the top or bottom edge scrolls the tree, faster closer to the edge.
            if drag.is_some()
                && let Some(p) = pointer
            {
                let clip = ui.clip_rect();
                const EDGE: f32 = 32.0;
                if clip.x_range().contains(p.x) && clip.y_range().contains(p.y) {
                    let pull = if p.y < clip.top() + EDGE {
                        (clip.top() + EDGE - p.y) / EDGE
                    } else if p.y > clip.bottom() - EDGE {
                        -(p.y - (clip.bottom() - EDGE)) / EDGE
                    } else {
                        0.0
                    };
                    if pull != 0.0 {
                        let dt = ui.input(|i| i.stable_dt).min(0.05);
                        ui.scroll_with_delta(vec2(0.0, pull * 700.0 * dt));
                        ui.ctx().request_repaint();
                    }
                }
            }
        });
        // Where a drag would drop: the folder under the pointer, the folder of the file under
        // it, or the project itself (its header, and the space below the files).
        let tree_rect = hr.union(scrolled.inner_rect);
        let drop_dir = drag.as_ref().and_then(|_| {
            let p = pointer?;
            match drag_over {
                Some(i) => {
                    let e = &rows[i].entry;
                    Some(if e.is_dir { e.path.clone() } else { e.path.parent()?.to_path_buf() })
                }
                None => tree_rect.contains(p).then(|| root.clone()),
            }
        });
        let target = match (&drag, &drop_dir) {
            (Some(d), Some(dir)) if can_move(&d.path, dir) => Some(dir.clone()),
            _ => None,
        };
        if let Some(d) = &drag {
            // Into itself: say no. Where it already is: nothing to show.
            if drop_dir.as_ref().is_some_and(|dir| dir.starts_with(&d.path)) {
                ctx.set_cursor_icon(egui::CursorIcon::NoDrop);
            }
            self.drag_ghost(&ctx, d);
        }
        // Outline the folder it would land in, with everything shown inside it.
        if let Some(dir) = &target {
            let tint = crate::theme::with_alpha(theme.text_accent, 22);
            let edge = Stroke::new(1.0, crate::theme::with_alpha(theme.text_accent, 150));
            let area = if *dir == root {
                Some(tree_rect)
            } else {
                let at = rows.iter().position(|r| &r.entry.path == dir);
                let span = at.map(|f| f..rows[f + 1..].iter().position(|r| r.depth <= rows[f].depth).map_or(rows.len(), |n| f + 1 + n));
                span.and_then(|span| row_rects.iter().filter(|(i, _)| span.contains(i)).map(|(_, r)| *r).reduce(|a, b| a.union(b)))
            };
            if let Some(a) = area {
                let painter = ui.painter().with_clip_rect(tree_rect);
                painter.rect(a, CornerRadius::same(6), tint, edge, egui::StrokeKind::Inside);
            }
        }
        // Resting on a closed folder opens it.
        let resting = target.as_ref().zip(drag_over).filter(|(dir, i)| rows[*i].entry.path == **dir && rows[*i].entry.is_dir && !rows[*i].expanded).map(|(dir, _)| dir.clone());
        let now = ctx.input(|i| i.time);
        match (&resting, &self.sidebar.drag_hover) {
            (Some(dir), Some((was, since))) if dir == was => {
                if now - since >= DRAG_OPEN_DELAY {
                    toggle = Some(dir.clone());
                    self.sidebar.drag_hover = None;
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(DRAG_OPEN_DELAY - (now - since)));
                }
            }
            (Some(dir), _) => {
                self.sidebar.drag_hover = Some((dir.clone(), now));
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(DRAG_OPEN_DELAY));
            }
            (None, _) => self.sidebar.drag_hover = None,
        }
        if let (Some(d), Some(dir)) = (&drag, &target)
            && ctx.input(|i| i.pointer.primary_released())
        {
            action = Some(TreeAction::Move(d.path.clone(), dir.clone()));
            toggle = None;
        }
        // The space below the last row: right-click to add things at the top of the project.
        let below = scrolled.inner_rect.top() + scrolled.content_size.y - scrolled.state.clamped_offset().y;
        if below < scrolled.inner_rect.bottom() - 4.0 {
            let empty = Rect::from_min_max(pos2(scrolled.inner_rect.left(), below), scrolled.inner_rect.max);
            ui.interact(empty, Id::unique("files-empty"), Sense::click()).context_menu(|ui| root_menu(ui, &root, &mut action));
        }
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
            Some(TreeAction::Move(from, dir)) => self.move_into(&from, &dir),
            None => {}
        }
    }

    /// What follows the pointer while dragging in the tree: the item's icon and name.
    fn drag_ghost(&self, ctx: &egui::Context, d: &TreeDrag) {
        let Some(p) = ctx.pointer_hover_pos() else { return };
        let theme = &self.theme;
        let tk = self.tokens();
        egui::Area::new(Id::unique("tree-drag-ghost")).order(egui::Order::Tooltip).fixed_pos(p + vec2(14.0, 10.0)).interactable(false).show(ctx, |ui| {
            let font = ui::ui_font(13.0);
            let text_w = ui.painter().layout_no_wrap(d.name.clone(), font.clone(), theme.text).size().x.min(220.0);
            let (r, _) = ui.allocate_exact_size(vec2(text_w + 46.0, 26.0), Sense::hover());
            ui.painter().add(egui::epaint::Shadow { offset: [0, 3], blur: 10, spread: 0, color: tk.shadow }.as_shape(r, CornerRadius::same(7)));
            ui.painter().rect_filled(r, CornerRadius::same(7), tk.popup);
            ui.painter().rect_stroke(r, CornerRadius::same(7), Stroke::new(1.0, tk.subtle_border), egui::StrokeKind::Inside);
            let icon = if d.is_dir { file_icons::folder_icon(&d.name, false) } else { file_icons::file_icon(&d.name) };
            file_icons::image(icon, 16.0).paint_at(ui, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), vec2(16.0, 16.0)));
            truncated_label(ui, pos2(r.left() + 34.0, r.center().y), &d.name, font, theme.text, text_w);
        });
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

#[cfg(test)]
mod tests {
    use super::can_move;
    use std::path::Path;

    #[test]
    fn where_a_dragged_item_can_go() {
        let p = Path::new;
        assert!(can_move(p("/proj/a.rkt"), p("/proj/sub")));
        assert!(can_move(p("/proj/sub/a.rkt"), p("/proj")));
        assert!(can_move(p("/proj/sub"), p("/proj/other/deep")));
        // Where it already is.
        assert!(!can_move(p("/proj/a.rkt"), p("/proj")));
        // A folder into itself, or into a folder inside it.
        assert!(!can_move(p("/proj/sub"), p("/proj/sub")));
        assert!(!can_move(p("/proj/sub"), p("/proj/sub/inner")));
        // A name that only starts the same is a different folder.
        assert!(can_move(p("/proj/sub"), p("/proj/subway")));
    }
}
