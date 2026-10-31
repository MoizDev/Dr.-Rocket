//! Debounced, cancellable project search with bounded requests and results.
use super::SearchHit;
use std::path::PathBuf;

struct SearchRequest {
    generation: u64,
    query: String,
    root: Option<PathBuf>,
    index: std::sync::Arc<Vec<PathBuf>>,
    open: std::collections::HashMap<PathBuf, String>,
    ctx: egui::Context,
}
pub(super) struct SearchWorker {
    requests: crossbeam_channel::Sender<SearchRequest>,
    results: crossbeam_channel::Receiver<(u64, Vec<SearchHit>)>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub(super) needs_request: bool,
}
impl Default for SearchWorker {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::bounded::<SearchRequest>(1);
        let (results, out) = crossbeam_channel::bounded(1);
        let generation = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let current = generation.clone();
        std::thread::spawn(move || {
            while let Ok(request) = rx.recv() {
                if current.load(std::sync::atomic::Ordering::Relaxed) != request.generation {
                    continue;
                }
                let hits = search_files(&request, &current);
                if current.load(std::sync::atomic::Ordering::Relaxed) == request.generation {
                    // A full result queue is drained by the UI; keep only the newest valid job.
                    if results.send((request.generation, hits)).is_err() {
                        break;
                    }
                    request.ctx.request_repaint();
                }
            }
        });
        Self {
            requests: tx,
            results: out,
            generation,
            needs_request: false,
        }
    }
}
impl SearchWorker {
    pub(super) fn cancel(&mut self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.needs_request = true;
        while self.results.try_recv().is_ok() {}
    }
    pub(super) fn request(
        &mut self,
        query: String,
        root: Option<PathBuf>,
        index: std::sync::Arc<Vec<PathBuf>>,
        open: std::collections::HashMap<PathBuf, String>,
        ctx: egui::Context,
    ) {
        let generation = self.generation.load(std::sync::atomic::Ordering::Relaxed);
        if self
            .requests
            .try_send(SearchRequest {
                generation,
                query,
                root,
                index,
                open,
                ctx: ctx.clone(),
            })
            .is_ok()
        {
            self.needs_request = false;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(25));
        }
    }
    pub(super) fn poll(&self) -> Option<Vec<SearchHit>> {
        let mut latest = None;
        while let Ok((generation, hits)) = self.results.try_recv() {
            if generation == self.generation.load(std::sync::atomic::Ordering::Relaxed) {
                latest = Some(hits);
            }
        }
        latest
    }
}
impl Drop for SearchWorker {
    fn drop(&mut self) {
        self.generation.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn search_files(job: &SearchRequest, current: &std::sync::atomic::AtomicU64) -> Vec<SearchHit> {
    use std::io::Read;
    if job.query.chars().count() < 2 {
        return Vec::new();
    }
    let Ok(pattern) = regex::RegexBuilder::new(&regex::escape(&job.query))
        .case_insensitive(true)
        .build()
    else {
        return Vec::new();
    };
    let cancelled = || current.load(std::sync::atomic::Ordering::Relaxed) != job.generation;
    let mut paths: std::collections::HashSet<PathBuf> = job
        .index
        .iter()
        .filter_map(|p| job.root.as_ref().map(|r| r.join(p)))
        .collect();
    paths.extend(job.open.keys().cloned());
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    let mut out = Vec::new();
    for path in paths {
        if cancelled() {
            return Vec::new();
        }
        if !matches!(
            path.extension().and_then(|s| s.to_str()).unwrap_or(""),
            "rkt" | "rktl" | "rktd" | "scrbl" | "ss" | "scm" | "md" | "txt" | "json" | "toml"
        ) {
            continue;
        }
        let text = match job.open.get(&path) {
            Some(t) => t.clone(),
            None => {
                let Ok(file) = std::fs::File::open(&path) else {
                    continue;
                };
                if !file.metadata().is_ok_and(|m| m.is_file() && m.len() <= 2_000_000) {
                    continue;
                }
                let mut bytes = Vec::new();
                if file.take(2_000_001).read_to_end(&mut bytes).is_err() || bytes.len() > 2_000_000 {
                    continue;
                }
                let Ok(text) = String::from_utf8(bytes) else {
                    continue;
                };
                text
            }
        };
        for (line, text) in text.lines().enumerate() {
            if cancelled() {
                return Vec::new();
            }
            if let Some(hit) = pattern.find(text) {
                out.push(SearchHit {
                    path: path.clone(),
                    line,
                    col: text[..hit.start()].chars().count(),
                    len: hit.as_str().chars().count(),
                    text: text.to_owned(),
                });
                if out.len() >= 2000 {
                    return out;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };
    #[test]
    fn latest_query_wins_and_dirty_buffers_are_searched_with_original_unicode_columns() {
        let mut worker = SearchWorker::default();
        let open = HashMap::from([(PathBuf::from("scratch.rkt"), "İλ target\nother\n".to_string())]);
        worker.cancel();
        worker.request(
            "other".into(),
            None,
            Arc::default(),
            open.clone(),
            egui::Context::default(),
        );
        worker.cancel();
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.needs_request {
            worker.request(
                "target".into(),
                None,
                Arc::default(),
                open.clone(),
                egui::Context::default(),
            );
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        loop {
            if let Some(hits) = worker.poll() {
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].line, 0);
                assert_eq!(hits[0].col, 3);
                assert_eq!(hits[0].len, 6);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn cancellation_stops_stale_work_and_oversized_files_are_skipped_before_reading() {
        let dir = std::env::temp_dir().join(format!("drrocket-search-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("large.rkt");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(100_000_000).unwrap();
        let job = SearchRequest {
            generation: 1,
            query: "needle".into(),
            root: Some(dir.clone()),
            index: Arc::new(vec![PathBuf::from("large.rkt")]),
            open: HashMap::new(),
            ctx: egui::Context::default(),
        };
        let generation = AtomicU64::new(1);
        assert!(search_files(&job, &generation).is_empty());
        generation.store(2, Ordering::Relaxed);
        assert!(search_files(&job, &generation).is_empty());
        drop(f);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
