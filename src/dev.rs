//! Developer mode (`DRROCKET_DEV=<repo root>`, set by `tools/dev.sh`): bundled themes are read
//! from the repo instead of the binary and reloaded live when their files change.

use crossbeam_channel::{Receiver, unbounded};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;

/// The repo root when running in developer mode.
pub fn root() -> Option<PathBuf> {
    std::env::var_os("DRROCKET_DEV").map(PathBuf::from).filter(|p| p.join("assets/themes").is_dir())
}

/// A bundled theme's JSON from the repo, when in developer mode.
pub fn bundled_theme(file: &str) -> Option<String> {
    std::fs::read_to_string(root()?.join("assets/themes").join(file)).ok()
}

pub struct ThemeWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<()>,
}

impl ThemeWatcher {
    pub fn start(ctx: &egui::Context) -> Option<Self> {
        let dir = root()?.join("assets/themes");
        let (tx, events) = unbounded();
        let ctx = ctx.clone();
        let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            if ev.is_ok_and(|e| e.kind.is_modify() || e.kind.is_create()) {
                let _ = tx.send(());
                ctx.request_repaint();
            }
        })
        .ok()?;
        watcher.watch(&dir, RecursiveMode::NonRecursive).ok()?;
        Some(Self { _watcher: watcher, events })
    }

    /// True (once) after theme files changed.
    pub fn changed(&self) -> bool {
        self.events.try_iter().count() > 0
    }
}
