//! One non-recursive watcher per open-file parent, backed by periodic content checks.
//! Checks and watch registration happen off the UI thread, including for parked projects.
use super::file_safety::fingerprint_path;
use crossbeam_channel::{Receiver, Sender, bounded};
use notify::{RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub struct DiskChange {
    pub id: u64,
    pub path: PathBuf,
    pub hash: Option<[u8; 32]>,
}
pub struct FileMonitor {
    paths: Vec<(u64, PathBuf)>,
    requests: Sender<Vec<(u64, PathBuf)>>,
    pub changes: Receiver<(Vec<DiskChange>, Vec<String>)>,
    alive: Arc<AtomicBool>,
}
impl FileMonitor {
    pub fn new(ctx: &egui::Context) -> Self {
        let (tx, rx) = bounded::<Vec<(u64, PathBuf)>>(1);
        let (results, changes) = bounded(1);
        let alive = Arc::new(AtomicBool::new(true));
        let (ctx, running) = (ctx.clone(), alive.clone());
        std::thread::spawn(move || {
            let (trigger, events) = bounded(1);
            let runtime_warnings = Arc::new(parking_lot::Mutex::new(Vec::new()));
            let watch_warnings = runtime_warnings.clone();
            let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Err(e) = &event {
                    let mut warnings = watch_warnings.lock();
                    if warnings.len() < 8 {
                        warnings.push(format!("Open-file watcher: {e}. Periodic checks remain active."));
                    }
                }
                if !event
                    .as_ref()
                    .is_ok_and(|e| matches!(e.kind, notify::EventKind::Access(_)))
                {
                    let _ = trigger.try_send(());
                }
            });
            let mut paths = Vec::new();
            let mut parents = HashSet::new();
            let mut previous: HashMap<(u64, PathBuf), Option<[u8; 32]>> = HashMap::new();
            while running.load(Ordering::Relaxed) {
                let mut warnings = std::mem::take(&mut *runtime_warnings.lock());
                while let Ok(p) = rx.try_recv() {
                    paths = p;
                }
                let wanted: HashSet<_> = paths
                    .iter()
                    .filter_map(|(_, p)| p.parent().map(|p| p.to_path_buf()))
                    .collect();
                match &mut watcher {
                    Ok(w) => {
                        for p in parents.difference(&wanted) {
                            let _ = w.unwatch(p);
                        }
                        for p in wanted.difference(&parents) {
                            if let Err(e) = w.watch(p, RecursiveMode::NonRecursive) {
                                warnings.push(format!("Could not watch {}: {e}. Using periodic checks.", p.display()));
                            }
                        }
                    }
                    Err(e) if parents != wanted => {
                        warnings.push(format!("Open-file watcher unavailable: {e}. Using periodic checks."))
                    }
                    Err(_) => {}
                }
                parents = wanted;
                let keys: HashSet<_> = paths.iter().cloned().collect();
                previous.retain(|key, _| keys.contains(key));
                let mut changed = Vec::new();
                for (id, path) in &paths {
                    if !running.load(Ordering::Relaxed) {
                        return;
                    }
                    let hash = fingerprint_path(path).ok();
                    let key = (*id, path.clone());
                    if previous.get(&key) != Some(&hash) {
                        changed.push(DiskChange {
                            id: *id,
                            path: path.clone(),
                            hash,
                        });
                    }
                }
                if !changed.is_empty() || !warnings.is_empty() {
                    // Do not lose notifications when the UI is busy: retry on the next pass.
                    let delivered: Vec<_> = changed.iter().map(|c| ((c.id, c.path.clone()), c.hash)).collect();
                    match results.try_send((changed, warnings)) {
                        Ok(()) => {
                            previous.extend(delivered);
                            ctx.request_repaint();
                        }
                        Err(crossbeam_channel::TrySendError::Full((_, warnings))) => {
                            let mut pending = runtime_warnings.lock();
                            pending.extend(warnings);
                            pending.truncate(8);
                        }
                        Err(_) => break,
                    }
                }
                // Debounced events expedite the next reconciliation; the timer is the fallback.
                crossbeam_channel::select! {
                    recv(rx) -> request => match request { Ok(p) => paths = p, Err(_) => break },
                    recv(events) -> _ => { std::thread::sleep(Duration::from_millis(100)); while events.try_recv().is_ok() {} },
                    default(Duration::from_secs(2)) => {},
                }
            }
        });
        Self {
            paths: Vec::new(),
            requests: tx,
            changes,
            alive,
        }
    }
    pub fn sync_paths(&mut self, paths: Vec<(u64, PathBuf)>) {
        if paths != self.paths && self.requests.try_send(paths.clone()).is_ok() {
            self.paths = paths;
        }
    }
}
impl Drop for FileMonitor {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reconciles_loose_and_parked_files_including_same_size_replacements() {
        let dir = std::env::temp_dir().join(format!("drrocket-monitor-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("loose.rkt");
        let b = dir.join("parked.rkt");
        std::fs::write(&a, b"first").unwrap();
        std::fs::write(&b, b"first").unwrap();
        let mut monitor = FileMonitor::new(&egui::Context::default());
        monitor.sync_paths(vec![(1, a.clone()), (2, b.clone())]);
        let (initial, _) = monitor.changes.recv_timeout(Duration::from_secs(8)).unwrap();
        assert_eq!(initial.len(), 2);
        std::fs::write(&a, b"other").unwrap();
        let tmp = dir.join("replacement");
        std::fs::write(&tmp, b"other").unwrap();
        std::fs::rename(tmp, &b).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let mut seen = HashSet::new();
        while seen.len() < 2 {
            let (changes, _) = monitor
                .changes
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap();
            for c in changes {
                if c.hash == Some(super::super::file_safety::fingerprint(b"other")) {
                    seen.insert(c.id);
                }
            }
        }
        drop(monitor);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
