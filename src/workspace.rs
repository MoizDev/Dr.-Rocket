//! A project folder: lazily loaded file tree, background file index for quick open, and a
//! file watcher that keeps both fresh.

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

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
    pub index: Arc<Mutex<Vec<PathBuf>>>,
    index_generation: Arc<Mutex<u64>>,
    _watcher: Option<RecommendedWatcher>,
    events: Option<Receiver<notify::Result<notify::Event>>>,
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
        let (tx, rx) = channel();
        let ctx2 = ctx.clone();
        let watcher = notify::recommended_watcher(move |ev| {
            let _ = tx.send(ev);
            ctx2.request_repaint();
        })
        .ok()
        .and_then(|mut w| w.watch(&root, RecursiveMode::Recursive).ok().map(|_| w));
        let mut ws = Workspace {
            root: root.clone(),
            name,
            listings: HashMap::new(),
            expanded: HashSet::new(),
            index: Arc::new(Mutex::new(Vec::new())),
            index_generation: Arc::new(Mutex::new(0)),
            events: watcher.as_ref().map(|_| rx),
            _watcher: watcher,
            changed_files: Vec::new(),
            rows_cache: None,
            ctx: ctx.clone(),
        };
        ws.expanded.insert(root);
        ws.reindex();
        ws
    }

    /// Rebuild the quick-open index on a background thread.
    pub fn reindex(&mut self) {
        let root = self.root.clone();
        let index = self.index.clone();
        let generation = self.index_generation.clone();
        let my_gen = {
            let mut g = generation.lock();
            *g += 1;
            *g
        };
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut files = Vec::new();
            let walker = ignore::WalkBuilder::new(&root)
                .hidden(true)
                .git_ignore(true)
                .filter_entry(|e| !is_noise(&e.file_name().to_string_lossy()))
                .build();
            for e in walker.flatten() {
                if e.file_type().is_some_and(|t| t.is_file())
                    && let Ok(rel) = e.path().strip_prefix(&root)
                {
                    files.push(rel.to_path_buf());
                    if files.len() >= 100_000 {
                        break;
                    }
                }
            }
            files.sort();
            if *generation.lock() == my_gen {
                *index.lock() = files;
                ctx.request_repaint();
            }
        });
    }

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
        let Some(rx) = &self.events else { return false };
        let mut changed_dirs = HashSet::new();
        let mut structural = false;
        while let Ok(ev) = rx.try_recv() {
            let Ok(ev) = ev else { continue };
            let create_or_remove = matches!(ev.kind, notify::EventKind::Create(_) | notify::EventKind::Remove(_) | notify::EventKind::Modify(notify::event::ModifyKind::Name(_)));
            for p in ev.paths {
                if p.components().any(|c| is_noise(&c.as_os_str().to_string_lossy())) {
                    continue;
                }
                if matches!(ev.kind, notify::EventKind::Modify(_)) {
                    self.changed_files.push(p.clone());
                }
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
            self.reindex();
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
