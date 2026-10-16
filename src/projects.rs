//! Project tabs: several folders open at once, shown as tabs in the title bar.
//!
//! The current project's folder and editor tabs live in `App::workspace`, `App::tabs` and
//! `App::active`, so the rest of the app only ever sees one project. The others are parked in
//! their [`ProjectSlot`]; switching swaps the whole set in and out.

use crate::app::{App, Modal, Tab};
use crate::workspace::Workspace;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ProjectSlot {
    /// The project folder (`None` for a project of loose files).
    pub root: Option<PathBuf>,
    workspace: Option<Workspace>,
    tabs: Vec<Tab>,
    active: usize,
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
                label: p.label(),
                root: p.root.clone(),
                dirty: if k == self.current_project { self.tabs.iter().any(|t| t.dirty()) } else { p.tabs.iter().any(|t| t.dirty()) },
            })
            .collect()
    }

    /// Bring project `k` to the front.
    pub fn switch_project(&mut self, k: usize) {
        if k == self.current_project || k >= self.projects.len() {
            return;
        }
        let cur = &mut self.projects[self.current_project];
        cur.workspace = self.workspace.take();
        cur.tabs = std::mem::take(&mut self.tabs);
        cur.active = self.active;
        self.current_project = k;
        let next = &mut self.projects[k];
        self.workspace = next.workspace.take();
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

    /// Open a fresh, empty project tab and switch to it.
    pub fn new_project(&mut self) {
        self.projects.push(ProjectSlot::default());
        self.switch_project(self.projects.len() - 1);
    }

    /// Show `path` as a project: switch to its tab if it is open, use the current tab if it is
    /// empty, otherwise open a new tab.
    pub fn open_project(&mut self, path: &Path) {
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
