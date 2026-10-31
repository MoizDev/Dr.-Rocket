//! A project folder: lazily loaded file tree, background file index for quick open, and a
//! file watcher that keeps both fresh.

use notify::{RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// A row of the flattened, visible tree.
#[derive(Clone, Debug)]
pub struct Row {
    pub entry: Entry,
    pub depth: usize,
    pub expanded: bool,
}

pub struct Workspace {
    pub root: PathBuf,
    pub name: String,
    listings: HashMap<PathBuf, Vec<Entry>>,
    pub expanded: HashSet<PathBuf>,
    /// All files (relative to root), filled in the background.
    pub index: Arc<Mutex<Arc<Vec<PathBuf>>>>,
    index_generation: Arc<AtomicU64>,
    pub index_revision: Arc<AtomicU64>,
    alive: Arc<AtomicBool>,
    active: Arc<AtomicBool>,
    wake: Sender<()>,
    events: Receiver<notify::Event>,
    overflow: Arc<AtomicBool>,
    repaint_pending: Arc<AtomicBool>,
    warnings: Arc<Mutex<Vec<String>>>,
    reindex_due: Option<Instant>,
    /// Files changed on disk since the last `poll` (for reloading open buffers).
    pub changed_files: Vec<PathBuf>,
    rows_cache: Option<Vec<Row>>,
    ctx: egui::Context,
}

/// Directories that are never interesting in the tree.
fn is_noise(name: &str) -> bool {
    matches!(name, ".git" | "compiled" | ".DS_Store" | "node_modules" | "target" | ".drrocket" | ".zed" | ".vscode" | ".idea" | ".claude")
}

impl Workspace {
    pub fn open(root: &Path, ctx: &egui::Context) -> Self {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
        let (tx, rx) = bounded(256);
        let (wake, wake_rx) = bounded(1);
        let index = Arc::new(Mutex::new(Arc::new(Vec::new())));
        let generation = Arc::new(AtomicU64::new(1));
        let revision = Arc::new(AtomicU64::new(0));
        let alive = Arc::new(AtomicBool::new(true));
        let active = Arc::new(AtomicBool::new(true));
        let overflow = Arc::new(AtomicBool::new(false));
        let repaint_pending = Arc::new(AtomicBool::new(false));
        let warnings = Arc::new(Mutex::new(Vec::new()));
        {
            let (root, ctx, index, generation, revision, alive, active, overflow, repaint_pending, warnings) =
                (root.clone(), ctx.clone(), index.clone(), generation.clone(), revision.clone(), alive.clone(), active.clone(), overflow.clone(), repaint_pending.clone(), warnings.clone());
            std::thread::spawn(move || {
                let mut watcher = None;
                let mut indexed = 0;
                let mut watch_attempted = false;
                while alive.load(Ordering::Relaxed) {
                    if !active.load(Ordering::Relaxed) { watcher = None; watch_attempted = false; let _ = wake_rx.recv_timeout(Duration::from_millis(250)); continue; }
                    if watcher.is_none() && !watch_attempted {
                        watch_attempted = true;
                        let (tx, ctx2, root2, overflow, repaint, warnings2) = (tx.clone(),ctx.clone(),root.clone(),overflow.clone(),repaint_pending.clone(),warnings.clone());
                        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                            match result {
                                Ok(mut ev) => {
                                    if matches!(ev.kind, notify::EventKind::Access(_) | notify::EventKind::Other) && !ev.need_rescan() { return; }
                                    ev.paths.retain(|p| relevant_path(&root2,p));
                                    if ev.paths.is_empty() && !ev.need_rescan() && !matches!(ev.kind,notify::EventKind::Any) { return; }
                                    if tx.try_send(ev).is_err() { overflow.store(true,Ordering::Relaxed); }
                                }
                                Err(e) => { let mut w = warnings2.lock(); if w.len() < 8 { w.push(format!("File watcher: {e}")); } }
                            }
                            if !repaint.swap(true,Ordering::Relaxed) { ctx2.request_repaint(); }
                        }).and_then(|mut w| { w.watch(&root,RecursiveMode::Recursive)?; Ok(w) }) {
                            Ok(w) => watcher = Some(w),
                            Err(e) => { warnings.lock().push(format!("Could not watch {}: {e}. Open files still use periodic reconciliation.", root.display())); ctx.request_repaint();
                                // Do not hammer a failed registration. Retry on activation only.
                            }
                        }
                    }
                    let wanted = generation.load(Ordering::Relaxed);
                    if indexed != wanted {
                        let mut files = Vec::new();
                        let walker = ignore::WalkBuilder::new(&root).hidden(true).git_ignore(true)
                            .filter_entry(|e| e.depth() == 0 || !is_noise(&e.file_name().to_string_lossy())).build();
                        for entry in walker.flatten() {
                            if !alive.load(Ordering::Relaxed) || !active.load(Ordering::Relaxed) || generation.load(Ordering::Relaxed) != wanted { break; }
                            if entry.file_type().is_some_and(|t| t.is_file()) && let Ok(rel) = entry.path().strip_prefix(&root) { files.push(rel.to_path_buf()); }
                            if files.len() >= 100_000 { break; }
                        }
                        if alive.load(Ordering::Relaxed) && active.load(Ordering::Relaxed) && generation.load(Ordering::Relaxed) == wanted {
                            files.sort(); *index.lock() = Arc::new(files); indexed = wanted;
                            revision.fetch_add(1,Ordering::Relaxed); ctx.request_repaint();
                        }
                    }
                    let _ = wake_rx.recv_timeout(Duration::from_millis(250));
                }
            });
        }
        let mut ws = Workspace {
            root: root.clone(), name, listings: HashMap::new(), expanded: HashSet::new(),
            index, index_generation: generation, index_revision: revision, alive, active, wake,
            events: rx, overflow, repaint_pending, warnings, reindex_due: None,
            changed_files: Vec::new(), rows_cache: None, ctx: ctx.clone(),
        };
        ws.expanded.insert(root);
        ws
    }

    /// One worker owns indexing and watch registration. New generations cancel stale walks.
    pub fn reindex(&mut self) {
        self.index_generation.fetch_add(1,Ordering::Relaxed);
        let _ = self.wake.try_send(());
    }
    pub fn set_active(&mut self, active: bool) {
        self.active.store(active,Ordering::Relaxed);
        if active { self.invalidate(); }
        let _ = self.wake.try_send(());
    }
    pub fn take_warnings(&self) -> Vec<String> { std::mem::take(&mut *self.warnings.lock()) }

    fn load(&mut self, dir: &Path) -> &Vec<Entry> {
        if !self.listings.contains_key(dir) {
            let mut entries = Vec::new();
            let walker = ignore::WalkBuilder::new(dir)
                .max_depth(Some(1))
                .hidden(false)
                .git_ignore(true)
                .git_global(false)
                .parents(true)
                .build();
            for e in walker.flatten() {
                if e.depth() == 0 {
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                if is_noise(&name) {
                    continue;
                }
                let is_dir = e.file_type().is_some_and(|t| t.is_dir());
                entries.push(Entry { name, path: e.path().to_path_buf(), is_dir });
            }
            entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
            self.listings.insert(dir.to_path_buf(), entries);
        }
        &self.listings[dir]
    }

    pub fn toggle(&mut self, dir: &Path) {
        if !self.expanded.remove(dir) {
            self.expanded.insert(dir.to_path_buf());
        }
        self.rows_cache = None;
    }

    pub fn expand_to(&mut self, file: &Path) {
        let mut p = file.parent();
        while let Some(dir) = p {
            if !dir.starts_with(&self.root) {
                break;
            }
            self.expanded.insert(dir.to_path_buf());
            p = dir.parent();
        }
        self.rows_cache = None;
    }

    /// Keep the folders that were open under `from` open at their new place, `to`.
    pub fn path_moved(&mut self, from: &Path, to: &Path) {
        let moved: Vec<PathBuf> = self.expanded.iter().filter(|p| p.starts_with(from)).cloned().collect();
        for p in moved {
            self.expanded.remove(&p);
            let rest = p.strip_prefix(from).unwrap_or(Path::new(""));
            self.expanded.insert(if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) });
        }
        self.rows_cache = None;
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.expanded.insert(self.root.clone());
        self.rows_cache = None;
    }

    /// The flattened visible tree (cached until something changes).
    pub fn rows(&mut self) -> &[Row] {
        if self.rows_cache.is_none() {
            let mut out = Vec::new();
            let root = self.root.clone();
            self.collect(&root, 0, &mut out);
            self.rows_cache = Some(out);
        }
        self.rows_cache.as_ref().unwrap()
    }

    fn collect(&mut self, dir: &Path, depth: usize, out: &mut Vec<Row>) {
        let entries = self.load(dir).clone();
        for e in entries {
            let expanded = e.is_dir && self.expanded.contains(&e.path);
            out.push(Row { entry: e.clone(), depth, expanded });
            if expanded {
                self.collect(&e.path, depth + 1, out);
            }
        }
    }

    /// Process file-system events. Returns true if the tree changed.
    pub fn poll(&mut self) -> bool {
        self.repaint_pending.store(false,Ordering::Relaxed);
        let rx = &self.events;
        let mut changed_dirs = HashSet::new();
        let mut structural = self.overflow.swap(false,Ordering::Relaxed);
        if structural { self.listings.clear(); }
        for _ in 0..256 {
            let Ok(ev) = rx.try_recv() else { break; };
            if ev.need_rescan() || matches!(ev.kind,notify::EventKind::Any) { structural = true; self.listings.clear(); }
            let create_or_remove = matches!(ev.kind, notify::EventKind::Create(_) | notify::EventKind::Remove(_) | notify::EventKind::Modify(notify::event::ModifyKind::Name(_)));
            for p in ev.paths {
                if !relevant_path(&self.root, &p) {
                    continue;
                }
                if matches!(ev.kind, notify::EventKind::Modify(_)) && self.changed_files.len() < 256 { self.changed_files.push(p.clone()); }
                if create_or_remove && let Some(parent) = p.parent() {
                    changed_dirs.insert(parent.to_path_buf());
                    structural = true;
                }
            }
        }
        for d in &changed_dirs {
            self.listings.remove(d);
        }
        if structural {
            self.rows_cache = None;
            self.reindex_due = Some(Instant::now() + Duration::from_millis(150));
        }
        if let Some(due) = self.reindex_due {
            if Instant::now() >= due { self.reindex_due = None; self.reindex(); }
            else { self.ctx.request_repaint_after(due.saturating_duration_since(Instant::now())); }
        }
        structural
    }

    pub fn relative(&self, p: &Path) -> PathBuf {
        p.strip_prefix(&self.root).map(Path::to_path_buf).unwrap_or_else(|_| p.to_path_buf())
    }

    pub fn invalidate(&mut self) {
        self.listings.clear();
        self.rows_cache = None;
        self.reindex();
    }
}

/// "file2" < "file10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na: String = std::iter::from_fn(|| ai.next_if(|c| c.is_ascii_digit())).collect();
                let nb: String = std::iter::from_fn(|| bi.next_if(|c| c.is_ascii_digit())).collect();
                let o = na.len().cmp(&nb.len()).then(na.cmp(&nb));
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase());
                if o != std::cmp::Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

fn relevant_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|rel| !rel.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        is_noise(&name) || (name.contains(".drrocket-") && name.ends_with(".tmp"))
    }))
}
impl Drop for Workspace {
    fn drop(&mut self) { self.alive.store(false,Ordering::Relaxed); let _ = self.wake.try_send(()); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_ordering() {
        let mut v = vec!["a10", "a2", "B1", "a1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a1", "a2", "a10", "B1"]);
    }
}


#[cfg(test)]
mod scheduling_tests {
    use super::*;
    #[test]
    fn noise_filter_is_relative_to_selected_root() {
        let root = Path::new("/scratch/target/project");
        assert!(relevant_path(root,&root.join("main.rkt")));
        assert!(!relevant_path(root,&root.join("target/build/output")));
        assert!(!relevant_path(root,&root.join(".main.rkt.drrocket-1-0.tmp")));
        assert!(!relevant_path(root,Path::new("/outside/main.rkt")));
    }
    #[test]
    fn structural_batch_is_debounced_and_indexes_reconcile() {
        let dir = std::env::temp_dir().join(format!("drrocket-index-test-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = egui::Context::default();
        let mut workspace = Workspace::open(&dir,&ctx);
        for n in 0..50 { std::fs::write(dir.join(format!("file{n}.rkt")),b"#lang racket\n").unwrap(); }
        workspace.invalidate();
        let deadline = Instant::now()+Duration::from_secs(8);
        while workspace.index.lock().len() != 50 {
            assert!(Instant::now() < deadline,"background index did not reconcile");
            workspace.poll(); std::thread::sleep(Duration::from_millis(20));
        }
        workspace.set_active(false);
        workspace.set_active(true);
        assert_eq!(workspace.index.lock().len(),50);
        drop(workspace); std::fs::remove_dir_all(dir).unwrap();
    }
}
