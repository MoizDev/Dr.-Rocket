//! Project tabs: several folders open at once, shown as tabs in the title bar.
//!
//! The current project's folder and editor tabs live in `App::workspace`, `App::tabs` and
//! `App::active`, so the rest of the app only ever sees one project. The others are parked in
//! their [`ProjectSlot`]; switching swaps the whole set in and out.

use crate::app::{App, Modal, Tab};
use crate::workspace::Workspace;
use std::path::{Path, PathBuf};

pub struct ProjectSlot {
    /// Stable identity (UI ids, drag-reordering).
    pub id: u64,
    /// The project folder (`None` for a project of loose files).
    pub root: Option<PathBuf>,
    workspace: Option<Workspace>,
    tabs: Vec<Tab>,
    active: usize,
}

impl Default for ProjectSlot {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self { id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed), root: None, workspace: None, tabs: Vec::new(), active: 0 }
    }
}

impl ProjectSlot {
    pub fn label(&self) -> String {
        match &self.root {
            Some(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned()),
            None => "Untitled".into(),
        }
    }
}

/// What a project tab shows: its name and whether it has unsaved files.
pub struct ProjectTabInfo {
    pub id: u64,
    pub label: String,
    pub root: Option<PathBuf>,
    pub dirty: bool,
}

impl App {
    pub fn project_tabs(&self) -> Vec<ProjectTabInfo> {
        self.projects
            .iter()
            .enumerate()
            .map(|(k, p)| ProjectTabInfo {
                id: p.id,
                label: p.label(),
                root: p.root.clone(),
                dirty: if k == self.current_project { self.tabs.iter().any(|t| t.dirty()) } else { p.tabs.iter().any(|t| t.dirty()) },
            })
            .collect()
    }

    /// Bring project `k` to the front.
    pub fn switch_project(&mut self, k: usize) {
        self.whats_new.front = false;
        if k == self.current_project || k >= self.projects.len() {
            return;
        }
        let cur = &mut self.projects[self.current_project];
        if let Some(ws) = &mut self.workspace { ws.set_active(false); }
        cur.workspace = self.workspace.take();
        cur.tabs = std::mem::take(&mut self.tabs);
        cur.active = self.active;
        self.current_project = k;
        let next = &mut self.projects[k];
        self.workspace = next.workspace.take();
        if let Some(ws) = &mut self.workspace { ws.set_active(true); }
        self.tabs = std::mem::take(&mut next.tabs);
        self.active = next.active.min(self.tabs.len().saturating_sub(1));
        if let Some(root) = self.projects[k].root.clone() {
            self.recents.add_project(&root);
        }
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.editor.request_focus();
        }
        crate::racket_ui::resume_tabs(self);
    }

    /// Move project tab `from` to position `to` (drag-reordering).
    pub fn move_project(&mut self, from: usize, to: usize) {
        if from == to || from >= self.projects.len() || to >= self.projects.len() {
            return;
        }
        let current = self.projects[self.current_project].id;
        let slot = self.projects.remove(from);
        self.projects.insert(to, slot);
        self.current_project = self.projects.iter().position(|p| p.id == current).unwrap_or(0);
    }

    /// Open a fresh, empty project tab and switch to it.
    pub fn new_project(&mut self) {
        self.projects.push(ProjectSlot::default());
        self.switch_project(self.projects.len() - 1);
    }

    /// Show `path` as a project: switch to its tab if it is open, use the current tab if it is
    /// empty, otherwise open a new tab.
    pub fn open_project(&mut self, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let path = path.as_path();
        self.whats_new.front = false;
        if let Some(k) = self.projects.iter().enumerate().position(|(k, p)| {
            let root = if k == self.current_project { self.workspace.as_ref().map(|w| w.root.as_path()) } else { p.root.as_deref() };
            root.is_some_and(|r| r == path)
        }) {
            self.switch_project(k);
            return;
        }
        if self.workspace.is_some() || !self.tabs.is_empty() {
            self.new_project();
        }
        self.projects[self.current_project].root = Some(path.to_path_buf());
        self.workspace = Some(Workspace::open(path, &self.ctx));
        self.recents.add_project(path);
    }

    /// Close project tab `k`, asking first if it has unsaved files.
    pub fn close_project(&mut self, k: usize, force: bool) {
        if k >= self.projects.len() {
            return;
        }
        if !force {
            let dirty = if k == self.current_project { self.tabs.iter().any(|t| t.dirty()) } else { self.projects[k].tabs.iter().any(|t| t.dirty()) };
            if dirty {
                self.switch_project(k);
                self.modal = Modal::ConfirmCloseProject;
                return;
            }
        }
        if self.projects.len() == 1 {
            self.workspace = None;
            self.tabs.clear();
            self.active = 0;
            self.projects[0] = ProjectSlot::default();
            return;
        }
        if k == self.current_project {
            self.switch_project(if k + 1 < self.projects.len() { k + 1 } else { k - 1 });
        }
        self.projects.remove(k);
        if self.current_project > k {
            self.current_project -= 1;
        }
    }

    /// Every project's unsaved file names (for the quit prompt).
    pub fn all_dirty_titles(&self) -> Vec<String> {
        let parked = self.projects.iter().enumerate().filter(|(k, _)| *k != self.current_project).flat_map(|(_, p)| p.tabs.iter());
        self.tabs.iter().chain(parked).filter(|t| t.dirty()).map(|t| t.title()).collect()
    }

    /// Save every unsaved file in every project. False if a save failed or was cancelled.
    pub fn save_all_projects(&mut self) -> bool {
        let start = self.current_project;
        for k in 0..self.projects.len() {
            self.switch_project(k);
            for i in 0..self.tabs.len() {
                if self.tabs[i].dirty() && !self.save_tab(i, false) {
                    return false;
                }
            }
        }
        self.switch_project(start);
        true
    }

    /// The saved form of every project, the current one included.
    pub fn project_sessions(&self) -> (Vec<crate::app::ProjectSession>, usize) {
        let files = |tabs: &[Tab]| tabs.iter().filter_map(|t| t.path.clone()).collect();
        let list = self
            .projects
            .iter()
            .enumerate()
            .map(|(k, p)| {
                if k == self.current_project {
                    crate::app::ProjectSession { project: self.workspace.as_ref().map(|w| w.root.clone()), files: files(&self.tabs), active: self.active }
                } else {
                    crate::app::ProjectSession { project: p.root.clone(), files: files(&p.tabs), active: p.active }
                }
            })
            .collect();
        (list, self.current_project)
    }
}

impl App {
    pub(crate) fn all_file_paths(&self) -> Vec<(u64,PathBuf)> {
        self.tabs.iter().chain(self.projects.iter().enumerate().filter(|(k,_)| *k != self.current_project).flat_map(|(_,p)| p.tabs.iter()))
            .filter_map(|t| t.path.clone().map(|p| (t.id,p))).collect()
    }
    pub(crate) fn parked_project_for_tab(&self, id:u64) -> Option<usize> {
        self.projects.iter().enumerate().position(|(k,p)| k != self.current_project && p.tabs.iter().any(|t| t.id == id))
    }
    pub(crate) fn swap_parked_tabs(&mut self, k:usize) { std::mem::swap(&mut self.tabs,&mut self.projects[k].tabs); }
    pub(crate) fn follow_parked_paths(&mut self, from:&Path,to:&Path) {
        for (k,p) in self.projects.iter_mut().enumerate() {
            if k == self.current_project { continue; }
            for t in &mut p.tabs {
                if let Some(path) = &t.path && let Ok(rest) = path.strip_prefix(from) { t.path = Some(to.join(rest)); }
            }
            if let Some(root) = &p.root && let Ok(rest) = root.strip_prefix(from) {
                let root = to.join(rest);
                let mut workspace = Workspace::open(&root,&self.ctx);
                workspace.set_active(false);
                p.root = Some(root);
                p.workspace = Some(workspace);
            }
        }
    }
}
