//! Save snapshots, asynchronous WXME serialization, and save-before-close continuations.
//! Backend writes are retained until completion and target private sibling files, never
//! live destinations. Only this module publishes a successful, conflict-checked snapshot.
use super::{App, Modal, Tab, ToastKind, file_safety};
use crate::racket::lang::{Language, TeachingSettings};
use crate::racket::protocol::SourceImages;
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    Saved,
    Pending,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SavePurpose {
    Manual,
    SaveAs,
    Autosave,
    CloseTab,
    CloseProject,
    Quit,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum AfterSave {
    Tab { id: u64, then_quit: bool, discard: bool },
    Project { id: u64, discard: bool },
    Quit { discard: bool },
    SaveAll { project: Option<u64> },
}
#[derive(Debug, PartialEq, Eq)]
enum CloseDecision {
    Empty,
    Waiting,
    Failed,
    NewEdits,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReviewStamp {
    pub tab: u64,
    pub version: u64,
    pub language: Language,
    pub settings: TeachingSettings,
}
impl AfterSave {
    fn discards(self) -> bool {
        matches!(
            self,
            Self::Tab { discard: true, .. } | Self::Project { discard: true, .. } | Self::Quit { discard: true }
        )
    }
}
struct CloseIntent {
    action: AfterSave,
    tabs: Vec<u64>,
    failed: bool,
    reviewed: Vec<ReviewStamp>,
    remaining: VecDeque<u64>,
}

pub(super) struct Snapshot {
    tab: u64,
    version: u64,
    original_path: Option<PathBuf>,
    target: PathBuf,
    expected: Option<[u8; 32]>,
    language: Language,
    settings: TeachingSettings,
    text: String,
    images: SourceImages,
    format: file_safety::TextFormat,
    wxme: bool,
    recovery: bool,
    purpose: SavePurpose,
}
impl Snapshot {
    fn current(&self, tab: &Tab) -> bool {
        tab.id == self.tab
            && tab.editor.doc.version() == self.version
            && tab.language == self.language
            && tab.lang_settings == self.settings
    }
    fn apply_to(&self, tab: &mut Tab, hash: [u8; 32]) -> bool {
        if tab.id != self.tab || tab.path != self.original_path {
            return false;
        }
        if self.current(tab) {
            tab.editor.doc.buffer.mark_saved();
        }
        if tab.language == self.language && tab.lang_settings == self.settings {
            tab.lang_changed = false;
        }
        tab.path = Some(self.target.clone());
        tab.disk_hash = Some(hash);
        tab.disk_version = self.version;
        tab.save_failed = false;
        tab.disk_mtime = std::fs::metadata(&self.target).ok().and_then(|m| m.modified().ok());
        tab.original_header = crate::racket::lang::load(&self.text).header;
        tab.text_format = self.format.clone();
        tab.wxme = self.wxme;
        true
    }
    fn same_request(&self, other: &Self) -> bool {
        self.tab == other.tab
            && self.version == other.version
            && self.target == other.target
            && self.language == other.language
            && self.settings == other.settings
            && self.original_path == other.original_path
    }
}
struct PendingSave {
    snapshot: Snapshot,
    temporary: PathBuf,
    task: crate::racket_ui::WxmeWriteTask,
}
#[derive(Default)]
pub(super) struct SaveState {
    pending: Vec<PendingSave>,
    queued: VecDeque<Snapshot>,
    intent: Option<CloseIntent>,
}
impl SaveState {
    pub(super) fn busy(&self) -> bool {
        !self.pending.is_empty()
            || !self.queued.is_empty()
            || self.intent.as_ref().is_some_and(|i| !i.remaining.is_empty())
    }
    pub(super) fn has_tab(&self, id: u64) -> bool {
        self.pending.iter().any(|p| p.snapshot.tab == id)
            || self.queued.iter().any(|s| s.tab == id)
            || self.intent.as_ref().is_some_and(|i| i.remaining.contains(&id))
    }
    fn planned_for(&self, tab: u64, path: &Option<PathBuf>) -> Option<&Snapshot> {
        self.queued
            .iter()
            .rev()
            .chain(self.pending.iter().rev().map(|p| &p.snapshot))
            .find(|s| s.tab == tab && &s.original_path == path)
    }
    fn blocks(&self, s: &Snapshot) -> bool {
        // Serializing a directory also covers different spellings of a destination on
        // case-insensitive volumes, even while the destination does not exist yet.
        self.pending
            .iter()
            .any(|p| p.snapshot.tab == s.tab || p.snapshot.target.parent() == s.target.parent())
    }
    fn take_finished(&mut self) -> Option<(Snapshot, PathBuf, Result<(), String>)> {
        let (index, result) = self
            .pending
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.task.try_result().map(|result| (i, result)))?;
        let pending = self.pending.remove(index);
        Some((pending.snapshot, pending.temporary, result))
    }
    fn take_ready(&mut self) -> Option<Snapshot> {
        let index = self.queued.iter().position(|s| !self.blocks(s))?;
        self.queued.remove(index)
    }
    fn advance_queued(&mut self, snapshot: &Snapshot, hash: [u8; 32], bound: bool) {
        for queued in &mut self.queued {
            if queued.tab == snapshot.tab {
                if queued.target == snapshot.target && queued.expected == snapshot.expected {
                    queued.expected = Some(hash);
                }
                if bound && queued.original_path == snapshot.original_path {
                    queued.original_path = Some(snapshot.target.clone());
                }
            }
        }
    }
    fn cancel_intent(&mut self) -> bool {
        self.intent
            .take()
            .is_some_and(|i| matches!(i.action, AfterSave::Quit { .. }))
    }
    fn close_decision(&self, dirty: bool, review_changed: bool) -> CloseDecision {
        let Some(intent) = &self.intent else {
            return CloseDecision::Empty;
        };
        if self.busy()
            && (matches!(intent.action, AfterSave::Quit { .. }) || intent.tabs.iter().any(|id| self.has_tab(*id)))
        {
            return CloseDecision::Waiting;
        }
        if intent.failed {
            return CloseDecision::Failed;
        }
        if review_changed || (dirty && !intent.action.discards()) {
            return CloseDecision::NewEdits;
        }
        CloseDecision::Ready
    }
    pub(super) fn has_intent(&self) -> bool {
        self.intent.is_some()
    }
    pub(super) fn closing(&self) -> bool {
        self.intent
            .as_ref()
            .is_some_and(|i| !matches!(i.action, AfterSave::SaveAll { .. }))
    }
}

fn disk_hash(path: &Path) -> io::Result<Option<[u8; 32]>> {
    match file_safety::fingerprint_path(path) {
        Ok(hash) => Ok(Some(hash)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn destination(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|p| p.canonicalize().ok())
            .zip(path.file_name())
            .map(|(p, n)| p.join(n))
            .unwrap_or(path)
    })
}
fn temporary(target: &Path) -> io::Result<(PathBuf, File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..10_000 {
        let path = target.with_file_name(format!(
            ".{}.drrocket-save-{}-{}.tmp",
            target.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("Could not reserve a private save file"))
}
fn preserve(temporary: &Path, target: &Path) -> io::Result<PathBuf> {
    for n in 0..10_000 {
        let copy = target.with_file_name(format!(
            "{}.drrocket-conflict-{n}.rkt",
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
        match file_safety::rename_noreplace(temporary, &copy) {
            Ok(()) => return Ok(copy),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("Too many conflict copies"))
}
/// Ok(hash) means the snapshot was published. A conflict always retains the complete output.
fn publish(snapshot: &Snapshot, temporary: &Path) -> Result<[u8; 32], String> {
    let preserve_conflict = || match preserve(temporary, &snapshot.target) {
        Ok(copy) => format!(
            "Save conflict at {}. Disk data was kept; your saved snapshot is preserved at {}. Use Save As or Revert.",
            snapshot.target.display(),
            copy.display()
        ),
        Err(e) => format!(
            "Save stopped: disk data was kept. Your snapshot remains at {} (could not move recovery copy: {e}).",
            temporary.display()
        ),
    };
    let file = File::options()
        .read(true)
        .write(true)
        .open(temporary)
        .map_err(|e| e.to_string())?;
    if let Ok(meta) = std::fs::metadata(&snapshot.target) {
        file.set_permissions(meta.permissions()).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    let hash = file_safety::fingerprint_path(temporary).map_err(|e| e.to_string())?;
    drop(file);
    if snapshot.recovery || disk_hash(&snapshot.target).ok() != Some(snapshot.expected) {
        return Err(preserve_conflict());
    }
    let result = if snapshot.expected.is_none() {
        file_safety::rename_noreplace(temporary, &snapshot.target)
    } else {
        std::fs::rename(temporary, &snapshot.target)
    };
    match result {
        Ok(()) => Ok(hash),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(preserve_conflict()),
        Err(e) => Err(format!(
            "Save failed: {e}. Snapshot retained at {}.",
            temporary.display()
        )),
    }
}

impl App {
    pub(crate) fn saving_tab_for_path(&self, path: &Path) -> Option<u64> {
        self.saves
            .queued
            .iter()
            .rev()
            .chain(self.saves.pending.iter().rev().map(|p| &p.snapshot))
            .find(|s| s.target == path && self.project_for_tab(s.tab).is_some())
            .map(|s| s.tab)
    }
    pub(crate) fn tab_has_pending_save(&self, id: u64) -> bool {
        self.saves.has_tab(id)
    }
    /// Compatibility entry point: true only for an already completed save, never for a task.
    pub fn save_tab(&mut self, idx: usize, save_as: bool) -> bool {
        self.save_tab_for(
            idx,
            save_as,
            if save_as {
                SavePurpose::SaveAs
            } else {
                SavePurpose::Manual
            },
        ) == SaveOutcome::Saved
    }
    pub(crate) fn save_tab_for(&mut self, idx: usize, save_as: bool, purpose: SavePurpose) -> SaveOutcome {
        let Some(tab) = self.tabs.get(idx) else {
            return SaveOutcome::Failed;
        };
        if let Err(message) = tab.check_writable() {
            self.tabs[idx].save_failed = true;
            self.toast(message, ToastKind::Error);
            return SaveOutcome::Failed;
        }
        // An ordinary save following an unfinished Save As uses that logical destination
        // and its conflict baseline, rather than rewriting the original file or prompting again.
        let planned = (!save_as)
            .then(|| self.saves.planned_for(tab.id, &tab.path))
            .flatten()
            .map(|s| (s.target.clone(), s.expected));
        let path = match (&tab.path, save_as, &planned) {
            (_, false, Some((target, _))) => target.clone(),
            (Some(path), false, _) => path.clone(),
            _ => {
                let mut dialog = rfd::FileDialog::new()
                    .add_filter("Racket", &["rkt", "rktl", "rktd", "scrbl", "ss", "scm"])
                    .set_file_name(format!("{}.rkt", tab.title().trim_end_matches(".rkt")));
                if let Some(ws) = &self.workspace {
                    dialog = dialog.set_directory(&ws.root);
                }
                let Some(path) = dialog.save_file() else {
                    return SaveOutcome::Failed;
                };
                path
            }
        };
        let target = destination(path);
        let tab = &self.tabs[idx];
        if self
            .all_file_paths()
            .iter()
            .any(|(id, p)| *id != tab.id && p == &target)
        {
            self.tabs[idx].save_failed = true;
            self.toast(
                "Save cancelled: this path is already open in another tab.",
                ToastKind::Error,
            );
            return SaveOutcome::Failed;
        }
        let actual = match disk_hash(&target) {
            Ok(hash) => hash,
            Err(e) => {
                self.tabs[idx].save_failed = true;
                self.toast(format!("Cannot inspect save destination: {e}"), ToastKind::Error);
                return SaveOutcome::Failed;
            }
        };
        let tab = &self.tabs[idx];
        let expected = if let Some((_, expected)) = planned {
            expected
        } else if tab.path.as_ref() == Some(&target) {
            tab.disk_hash
        } else {
            actual
        };
        let text = tab.save_text(&target);
        let images = tab.editor.doc.images.for_text(&text);
        let snapshot = Snapshot {
            tab: tab.id,
            version: tab.editor.doc.version(),
            original_path: tab.path.clone(),
            target,
            expected,
            language: tab.language,
            settings: tab.lang_settings.clone(),
            text,
            wxme: tab.wxme || !images.is_empty(),
            images,
            format: tab.text_format.clone(),
            recovery: actual != expected,
            purpose,
        };
        if snapshot.recovery {
            self.tabs[idx].save_failed = true;
        }
        if self.saves.pending.iter().any(|p| p.snapshot.same_request(&snapshot)) {
            return SaveOutcome::Pending;
        }
        if let Some(queued) = self
            .saves
            .queued
            .iter_mut()
            .find(|s| s.tab == snapshot.tab && s.target == snapshot.target)
        {
            *queued = snapshot;
            return SaveOutcome::Pending;
        }
        if self.saves.blocks(&snapshot) || !self.saves.queued.is_empty() || self.saves.pending.len() >= 32 {
            if self.saves.queued.len() + self.saves.pending.len() >= 32 {
                self.tabs[idx].save_failed = true;
                self.toast(
                    "Too many queued saves; retry Save when current writes finish.",
                    ToastKind::Error,
                );
                return SaveOutcome::Failed;
            }
            self.saves.queued.push_back(snapshot);
            return SaveOutcome::Pending;
        }
        self.start_save(snapshot)
    }

    fn start_save(&mut self, mut snapshot: Snapshot) -> SaveOutcome {
        let (temporary, mut file) = match temporary(&snapshot.target) {
            Ok(value) => value,
            Err(e) => {
                self.save_error(&snapshot, format!("Could not prepare save: {e}"));
                return SaveOutcome::Failed;
            }
        };
        if snapshot.wxme {
            drop(file);
            match crate::racket_ui::begin_write_wxme(
                self,
                &temporary,
                &snapshot.text,
                std::mem::take(&mut snapshot.images),
            ) {
                Ok(task) => {
                    self.saves.pending.push(PendingSave {
                        snapshot,
                        temporary,
                        task,
                    });
                    SaveOutcome::Pending
                }
                Err(e) => {
                    let _ = std::fs::remove_file(temporary);
                    self.save_error(&snapshot, format!("Save failed: {e}"));
                    SaveOutcome::Failed
                }
            }
        } else {
            let result = file
                .write_all(&snapshot.format.encode(&snapshot.text))
                .and_then(|_| file.sync_all());
            drop(file);
            if let Err(e) = result {
                let _ = std::fs::remove_file(temporary);
                self.save_error(&snapshot, format!("Save failed: {e}"));
                return SaveOutcome::Failed;
            }
            self.finish_save(snapshot, temporary, Ok(()))
        }
    }
    fn save_error(&mut self, snapshot: &Snapshot, message: String) {
        if let Some(intent) = &mut self.saves.intent
            && (intent.tabs.contains(&snapshot.tab) || matches!(intent.action, AfterSave::Quit { .. }))
        {
            intent.failed = true;
            intent.remaining.clear();
        }
        let parked = self.parked_project_for_tab(snapshot.tab);
        if let Some(k) = parked {
            self.swap_parked_tabs(k);
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == snapshot.tab) {
            tab.save_failed = true;
        }
        if let Some(k) = parked {
            self.swap_parked_tabs(k);
        }
        let message = if snapshot.purpose == SavePurpose::Autosave {
            format!("Autosave paused. {message}")
        } else {
            message
        };
        self.toast(message, ToastKind::Error);
    }
    fn finish_save(&mut self, mut snapshot: Snapshot, temporary: PathBuf, result: Result<(), String>) -> SaveOutcome {
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            self.save_error(&snapshot, format!("Save failed: {error}"));
            return SaveOutcome::Failed;
        }
        // A Save As destination may have acquired another tab while serialization ran.
        // Preserve a recovery copy rather than binding two distinct buffers to one path.
        snapshot.recovery |= self
            .all_file_paths()
            .iter()
            .any(|(id, path)| *id != snapshot.tab && path == &snapshot.target);
        let hash = match publish(&snapshot, &temporary) {
            Ok(hash) => hash,
            Err(error) => {
                self.save_error(&snapshot, error);
                return SaveOutcome::Failed;
            }
        };
        let parked = self.parked_project_for_tab(snapshot.tab);
        if let Some(k) = parked {
            self.swap_parked_tabs(k);
        }
        let bound = self
            .tabs
            .iter_mut()
            .find(|t| t.id == snapshot.tab)
            .is_some_and(|tab| snapshot.apply_to(tab, hash));
        if let Some(k) = parked {
            self.swap_parked_tabs(k);
        }
        self.saves.advance_queued(&snapshot, hash, bound);
        self.recents.add_file(&snapshot.target);
        crate::racket_ui::remember_language(self, &snapshot.target, snapshot.language);
        SaveOutcome::Saved
    }
    /// Called after normal backend polling; never waits for a server response.
    pub(super) fn poll_saves(&mut self) {
        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_millis(3);
        while let Some((snapshot, path, result)) = self.saves.take_finished() {
            self.finish_save(snapshot, path, result);
            if started.elapsed() >= budget {
                self.ctx.request_repaint();
                self.finish_close_intent();
                return;
            }
        }
        self.pump_save_plan();
        while let Some(snapshot) = self.saves.take_ready() {
            self.start_save(snapshot);
            if started.elapsed() >= budget {
                self.ctx.request_repaint();
                break;
            }
        }
        self.finish_close_intent();
    }

    pub(crate) fn save_then(&mut self, action: AfterSave) {
        if self.saves.intent.is_some() {
            self.toast(
                "A save operation is already pending. Wait for it to finish, or press Esc to keep files open.",
                ToastKind::Info,
            );
            return;
        }
        let tabs = self.intent_tabs(action);
        let remaining = if action.discards() {
            VecDeque::new()
        } else {
            tabs.iter().copied().filter(|id| self.tab_is_dirty(*id)).collect()
        };
        let reviewed = self.review_action(action);
        self.saves.intent = Some(CloseIntent {
            action,
            tabs,
            failed: false,
            reviewed,
            remaining,
        });
        self.modal = Modal::None;
        self.pump_save_plan();
        if self.saves.busy() && !matches!(action, AfterSave::SaveAll { .. }) {
            self.toast("Saving before closing. Press Esc to keep files open.", ToastKind::Info);
        }
        self.finish_close_intent();
    }

    /// Stream bulk-save requests in bounded batches. Snapshot text/images are captured
    /// only while the reviewed version is still current; later edits remain unsaved/open.
    pub(crate) fn pump_save_plan(&mut self) {
        let Some(intent) = &self.saves.intent else {
            return;
        };
        let purpose = match intent.action {
            AfterSave::Tab { .. } => SavePurpose::CloseTab,
            AfterSave::Project { .. } => SavePurpose::CloseProject,
            AfterSave::Quit { .. } => SavePurpose::Quit,
            AfterSave::SaveAll { .. } => SavePurpose::Manual,
        };
        let original_project = self.projects[self.current_project].id;
        let started = std::time::Instant::now();
        while self.saves.pending.len() + self.saves.queued.len() < 32 {
            if started.elapsed() >= std::time::Duration::from_millis(3) {
                break;
            }
            let Some(id) = self.saves.intent.as_mut().and_then(|i| i.remaining.pop_front()) else {
                break;
            };
            let reviewed = self
                .saves
                .intent
                .as_ref()
                .and_then(|i| i.reviewed.iter().find(|r| r.tab == id));
            if self.review_for_tab(id).as_ref() != reviewed {
                continue;
            }
            if let Some(k) = self.project_for_tab(id) {
                self.switch_project(k);
            }
            if let Some(index) = self.tabs.iter().position(|t| t.id == id)
                && self.tabs[index].dirty()
                && self.save_tab_for(index, false, purpose) == SaveOutcome::Failed
            {
                self.note_save_intent_failure();
                break;
            }
            if started.elapsed() >= std::time::Duration::from_millis(3) {
                break;
            }
        }
        if let Some(k) = self.projects.iter().position(|p| p.id == original_project) {
            self.switch_project(k);
        }
        if self.saves.intent.as_ref().is_some_and(|i| !i.remaining.is_empty()) {
            self.ctx.request_repaint();
        }
    }

    pub(crate) fn await_save_then(&mut self, action: AfterSave) {
        if self.saves.intent.is_none() {
            self.saves.intent = Some(CloseIntent {
                action,
                tabs: self.intent_tabs(action),
                failed: false,
                reviewed: self.review_action(action),
                remaining: VecDeque::new(),
            });
        } else {
            self.toast(
                "A save operation is pending. Retry closing when it finishes, or press Esc to keep files open.",
                ToastKind::Info,
            );
        }
        self.finish_close_intent();
    }
    fn intent_tabs(&self, action: AfterSave) -> Vec<u64> {
        match action {
            AfterSave::Tab { id, .. } => vec![id],
            AfterSave::Project { id, .. } => self.project_tab_ids(id),
            AfterSave::Quit { .. } | AfterSave::SaveAll { project: None } => self.all_tab_ids(),
            AfterSave::SaveAll { project: Some(id) } => self.project_tab_ids(id),
        }
    }
    fn review_action(&self, action: AfterSave) -> Vec<ReviewStamp> {
        let tabs = self.intent_tabs(action);
        self.save_review()
            .into_iter()
            .filter(|s| tabs.contains(&s.tab))
            .collect()
    }
    fn finish_close_intent(&mut self) {
        let Some(intent) = &self.saves.intent else {
            return;
        };
        let dirty = self.intent_tabs(intent.action).iter().any(|id| self.tab_is_dirty(*id));
        let review_changed = intent.action.discards() && intent.reviewed != self.review_action(intent.action);
        match self.saves.close_decision(dirty, review_changed) {
            CloseDecision::Empty | CloseDecision::Waiting => return,
            CloseDecision::Failed => {
                self.cancel_close_intent();
                return;
            }
            CloseDecision::NewEdits => {
                self.cancel_close_intent();
                self.toast("Newer edits remain open. Save again to include them.", ToastKind::Info);
                return;
            }
            CloseDecision::Ready => {}
        }
        let intent = self.saves.intent.take().unwrap();
        match intent.action {
            AfterSave::Tab { id, then_quit, .. } => {
                self.close_tab(id, true);
                if then_quit {
                    self.quit();
                }
            }
            AfterSave::Project { id, .. } => {
                if let Some(k) = self.projects.iter().position(|p| p.id == id) {
                    self.close_project(k, true);
                }
            }
            AfterSave::SaveAll { .. } => {}
            AfterSave::Quit { .. } => {
                self.quit_confirmed = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    pub(crate) fn note_save_intent_failure(&mut self) {
        if let Some(intent) = &mut self.saves.intent {
            intent.failed = true;
            intent.remaining.clear();
        }
    }
    pub(crate) fn cancel_close_intent(&mut self) {
        if self.saves.cancel_intent() {
            self.restart_on_exit = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        editor::view::EditorView,
        racket_ui::{RacketTab, WxmeWriteTask},
    };
    use std::sync::mpsc;
    fn tab() -> Tab {
        let language = Language::Module;
        let mut tab = Tab {
            id: 1,
            path: Some(PathBuf::from("source.rkt")),
            untitled: 0,
            editor: EditorView::new("#lang racket\n", egui::Id::unique("pending-save-test")),
            language,
            lang_settings: language.default_settings(),
            lang_changed: false,
            disk_mtime: None,
            disk_hash: None,
            disk_version: 0,
            text_format: Default::default(),
            original_header: String::new(),
            wxme: true,
            save_failed: false,
            rk: RacketTab::new(1),
            zoom: 1.0,
            edit_seen: (0, 0.0),
        };
        tab.editor.doc.buffer.set_text("#lang racket\n(define x 1)\n");
        tab
    }
    fn snapshot(tab: &Tab, target: PathBuf) -> Snapshot {
        Snapshot {
            tab: tab.id,
            version: tab.editor.doc.version(),
            original_path: tab.path.clone(),
            target,
            expected: tab.disk_hash,
            language: tab.language,
            settings: tab.lang_settings.clone(),
            text: tab.editor.doc.text(),
            images: SourceImages::new(),
            format: Default::default(),
            wxme: true,
            recovery: false,
            purpose: SavePurpose::Manual,
        }
    }
    fn pending(snapshot: Snapshot) -> (PendingSave, mpsc::Sender<Result<(), String>>) {
        let (tx, rx) = mpsc::channel();
        (
            PendingSave {
                snapshot,
                temporary: PathBuf::from("scratch-temporary"),
                task: WxmeWriteTask::new(1, rx),
            },
            tx,
        )
    }
    #[test]
    fn a_delayed_snapshot_never_marks_intervening_edits_or_language_changes_saved() {
        let mut tab = tab();
        let saved = snapshot(&tab, PathBuf::from("save-as.rkt"));
        tab.editor.doc.buffer.set_text("#lang racket\n(define x 2)\n");
        assert!(saved.apply_to(&mut tab, [1; 32]));
        assert!(tab.dirty());
        assert!(tab.editor.doc.text().contains("x 2"));
        assert_eq!(tab.path.as_deref(), Some(Path::new("save-as.rkt")));
        assert_eq!(tab.disk_hash, Some([1; 32]));
        let saved = snapshot(&tab, PathBuf::from("save-as.rkt"));
        tab.lang_settings.tracing = !tab.lang_settings.tracing;
        tab.lang_changed = true;
        saved.apply_to(&mut tab, [2; 32]);
        assert!(tab.dirty() && tab.lang_changed);
        let saved = snapshot(&tab, PathBuf::from("save-as.rkt"));
        saved.apply_to(&mut tab, [3; 32]);
        assert!(!tab.dirty());
        let saved = snapshot(&tab, PathBuf::from("older-destination.rkt"));
        tab.path = Some(PathBuf::from("renamed-while-saving.rkt"));
        assert!(!saved.apply_to(&mut tab, [4; 32]));
        assert_eq!(tab.path.as_deref(), Some(Path::new("renamed-while-saving.rkt")));
    }
    #[test]
    fn close_project_and_quit_wait_for_success_and_refuse_newer_edits() {
        for action in [
            AfterSave::Tab {
                id: 1,
                then_quit: false,
                discard: false,
            },
            AfterSave::Project { id: 7, discard: false },
            AfterSave::Quit { discard: false },
        ] {
            let mut tab = tab();
            let (job, tx) = pending(snapshot(&tab, PathBuf::from("saved.rkt")));
            let mut state = SaveState {
                pending: vec![job],
                queued: VecDeque::new(),
                intent: Some(CloseIntent {
                    action,
                    tabs: vec![1],
                    failed: false,
                    reviewed: Vec::new(),
                    remaining: VecDeque::new(),
                }),
            };
            assert_eq!(state.close_decision(false, false), CloseDecision::Waiting);
            assert!(state.take_finished().is_none());
            tab.editor.doc.buffer.set_text("newer changes");
            tx.send(Ok(())).unwrap();
            let done = state.take_finished().unwrap();
            assert!(state.take_finished().is_none());
            done.0.apply_to(&mut tab, [1; 32]);
            assert_eq!(state.close_decision(tab.dirty(), false), CloseDecision::NewEdits);
            state.intent.as_mut().unwrap().failed = true;
            assert_eq!(state.close_decision(false, false), CloseDecision::Failed);
        }
    }
    #[test]
    fn destination_and_tab_serialization_keep_tasks_alive_and_report_disconnection_once() {
        let tab = tab();
        let (job, tx) = pending(snapshot(&tab, PathBuf::from("folder/a.rkt")));
        let mut state = SaveState {
            pending: vec![job],
            ..Default::default()
        };
        let mut second = snapshot(&tab, PathBuf::from("folder/b.rkt"));
        second.tab = 2;
        assert!(state.blocks(&second));
        state.queued.push_back(second);
        assert!(state.take_ready().is_none());
        let mut third = snapshot(&tab, PathBuf::from("elsewhere/c.rkt"));
        third.tab = 3;
        state.queued.push_back(third);
        assert_eq!(state.take_ready().unwrap().tab, 3);
        assert!(state.busy());
        drop(tx);
        let completed = state.take_finished().unwrap();
        assert!(completed.2.is_err());
        assert!(state.take_finished().is_none());
        assert_eq!(state.take_ready().unwrap().tab, 2);
        assert!(!state.busy());
    }
    #[test]
    fn external_changes_during_serialization_preserve_both_versions_and_do_not_publish_on_failure() {
        let dir = std::env::temp_dir().join(format!("drrocket-pending-conflict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("source.rkt");
        std::fs::write(&path, b"baseline").unwrap();
        let tab = tab();
        let mut job = snapshot(&tab, path.clone());
        job.expected = Some(file_safety::fingerprint(b"baseline"));
        let (temp, mut file) = temporary(&path).unwrap();
        file.write_all(b"serialized snapshot").unwrap();
        drop(file);
        std::fs::write(&path, b"external change").unwrap();
        let error = publish(&job, &temp).unwrap_err();
        assert!(error.contains("preserved at"));
        assert_eq!(std::fs::read(&path).unwrap(), b"external change");
        assert_eq!(
            std::fs::read(dir.join("source.rkt.drrocket-conflict-0.rkt")).unwrap(),
            b"serialized snapshot"
        );
        job.expected = Some(file_safety::fingerprint(b"external change"));
        let (temp, mut file) = temporary(&path).unwrap();
        file.write_all(b"successful snapshot").unwrap();
        drop(file);
        assert_eq!(
            publish(&job, &temp).unwrap(),
            file_safety::fingerprint(b"successful snapshot")
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"successful snapshot");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn a_new_save_as_destination_created_while_pending_is_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("drrocket-pending-newpath-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("new.rkt");
        let job = snapshot(&tab(), target.clone());
        let (temp, mut file) = temporary(&target).unwrap();
        file.write_all(b"my snapshot").unwrap();
        drop(file);
        std::fs::write(&target, b"someone else's file").unwrap();
        assert!(publish(&job, &temp).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"someone else's file");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancelling_a_close_does_not_cancel_or_drop_its_writer_and_discard_refuses_later_edits() {
        let (job, tx) = pending(snapshot(&tab(), PathBuf::from("source.rkt")));
        let mut state = SaveState {
            pending: vec![job],
            queued: VecDeque::new(),
            intent: Some(CloseIntent {
                action: AfterSave::Quit { discard: true },
                tabs: vec![1],
                failed: false,
                reviewed: Vec::new(),
                remaining: VecDeque::new(),
            }),
        };
        assert_eq!(state.close_decision(true, true), CloseDecision::Waiting);
        assert!(state.cancel_intent());
        assert!(state.busy());
        assert_eq!(state.close_decision(true, false), CloseDecision::Empty);
        state.intent = Some(CloseIntent {
            action: AfterSave::Quit { discard: true },
            tabs: vec![1],
            failed: false,
            reviewed: Vec::new(),
            remaining: VecDeque::new(),
        });
        tx.send(Ok(())).unwrap();
        assert!(state.take_finished().unwrap().2.is_ok());
        assert_eq!(state.close_decision(true, true), CloseDecision::NewEdits);
        assert_eq!(state.close_decision(true, false), CloseDecision::Ready);
    }
    #[test]
    fn own_success_advances_a_queued_snapshot_without_accepting_an_external_version() {
        let mut source = tab();
        source.path = None;
        let first = snapshot(&source, PathBuf::from("new.rkt"));
        source.editor.doc.buffer.set_text("newer requested snapshot");
        let second = snapshot(&source, PathBuf::from("new.rkt"));
        let mut state = SaveState {
            queued: VecDeque::from([second]),
            ..Default::default()
        };
        assert!(first.apply_to(&mut source, [7; 32]));
        state.advance_queued(&first, [7; 32], true);
        let next = state.take_ready().unwrap();
        assert_eq!(next.expected, Some([7; 32]));
        assert_eq!(next.original_path, source.path);
        next.apply_to(&mut source, [8; 32]);
        assert!(!source.dirty());
        let mut unrelated = snapshot(&source, PathBuf::from("new.rkt"));
        unrelated.expected = Some([9; 32]);
        state.queued.push_back(unrelated);
        state.advance_queued(&first, [10; 32], true);
        assert_eq!(state.queued[0].expected, Some([9; 32]));
    }

    #[test]
    fn a_large_bulk_plan_keeps_metadata_only_and_waits_for_unscheduled_files() {
        let mut state = SaveState {
            intent: Some(CloseIntent {
                action: AfterSave::Quit { discard: false },
                tabs: (1..=1000).collect(),
                failed: false,
                reviewed: Vec::new(),
                remaining: (1..=1000).collect(),
            }),
            ..Default::default()
        };
        assert!(state.busy());
        assert!(state.has_tab(1000));
        assert!(state.pending.is_empty() && state.queued.is_empty());
        assert_eq!(state.close_decision(false, false), CloseDecision::Waiting);
        state.intent.as_mut().unwrap().remaining.clear();
        assert!(!state.busy());
        assert_eq!(state.close_decision(false, false), CloseDecision::Ready);
    }

    #[test]
    fn reverting_or_reloading_during_a_save_does_not_leave_the_new_baseline_falsely_clean() {
        let mut source = tab();
        let old = snapshot(&source, PathBuf::from("source.rkt"));
        source.editor.doc.buffer.set_text("reverted disk contents");
        source.editor.doc.buffer.mark_saved();
        source.disk_version = source.editor.doc.version();
        assert!(!source.dirty());
        old.apply_to(&mut source, [5; 32]);
        assert!(source.dirty());
        assert_eq!(source.disk_version, old.version);
        assert_eq!(source.editor.doc.text(), "reverted disk contents");
    }

    #[test]
    fn save_following_pending_save_as_keeps_the_new_destination_and_conflict_baseline() {
        let mut source = tab();
        let mut first = snapshot(&source, PathBuf::from("new-destination.rkt"));
        first.expected = Some([3; 32]);
        let (job, _tx) = pending(first);
        let mut state = SaveState {
            pending: vec![job],
            ..Default::default()
        };
        let logical = state.planned_for(source.id, &source.path).unwrap();
        assert_eq!(logical.target, Path::new("new-destination.rkt"));
        assert_eq!(logical.expected, Some([3; 32]));
        let queued = snapshot(&source, PathBuf::from("latest-save-as.rkt"));
        state.queued.push_back(queued);
        assert_eq!(
            state.planned_for(source.id, &source.path).unwrap().target,
            Path::new("latest-save-as.rkt")
        );
        let old = state.pending.remove(0).snapshot;
        old.apply_to(&mut source, [4; 32]);
        state.advance_queued(&old, [4; 32], true);
        assert_eq!(
            state.planned_for(source.id, &source.path).unwrap().target,
            Path::new("latest-save-as.rkt")
        );
        source.path = Some(PathBuf::from("renamed-while-saving.rkt"));
        assert!(state.planned_for(source.id, &source.path).is_none());
    }

    #[test]
    fn delayed_discard_close_also_waits_and_protects_subsequent_edits() {
        for action in [
            AfterSave::Tab {
                id: 1,
                then_quit: false,
                discard: true,
            },
            AfterSave::Project { id: 7, discard: true },
            AfterSave::Quit { discard: true },
        ] {
            let (job, tx) = pending(snapshot(&tab(), PathBuf::from("source.rkt")));
            let mut state = SaveState {
                pending: vec![job],
                intent: Some(CloseIntent {
                    action,
                    tabs: vec![1],
                    failed: false,
                    reviewed: Vec::new(),
                    remaining: VecDeque::new(),
                }),
                ..Default::default()
            };
            assert_eq!(state.close_decision(true, false), CloseDecision::Waiting);
            tx.send(Ok(())).unwrap();
            assert!(state.take_finished().unwrap().2.is_ok());
            assert_eq!(state.close_decision(true, false), CloseDecision::Ready);
            assert_eq!(state.close_decision(true, true), CloseDecision::NewEdits);
        }
    }
}
