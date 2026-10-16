//! Racket integration: discovery, protocol, runner/stepper/server clients, language levels.
//!
//! The Racket-side programs (`racket/*.rkt`) are embedded in the binary. [`Backend::start`]
//! writes them to a versioned cache directory and compiles them with `raco make` in the
//! background; sessions can start right away (Racket compiles in memory until the `.zo`
//! files exist).
#![allow(dead_code)]

pub mod lang;
pub mod protocol;
pub mod runner;
pub mod server;
pub mod stepper;
mod transport;

#[cfg(test)]
mod tests;

use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

#[allow(unused_imports)]
pub use protocol::*;
#[allow(unused_imports)]
pub use runner::RunSession;
#[allow(unused_imports)]
pub use server::{RequestId, Server};
#[allow(unused_imports)]
pub use stepper::{StepRequest, StepperSession};

/// The backend scripts, embedded at compile time.
const SCRIPTS: &[(&str, &str)] = &[
    ("common.rkt", include_str!("../../racket/common.rkt")),
    ("runner.rkt", include_str!("../../racket/runner.rkt")),
    ("stepper.rkt", include_str!("../../racket/stepper.rkt")),
    ("visualize.rkt", include_str!("../../racket/visualize.rkt")),
    ("server.rkt", include_str!("../../racket/server.rkt")),
];

/// A Racket installation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RacketInstall {
    /// The `racket` executable.
    pub racket: PathBuf,
}

impl RacketInstall {
    /// The `raco` next to `racket`, if present.
    pub fn raco(&self) -> Option<PathBuf> {
        let p = self.racket.with_file_name(exe("raco"));
        p.is_file().then_some(p)
    }

    /// `(version)`, e.g. "9.3". Runs Racket, so call it off the UI thread.
    pub fn version(&self) -> Option<String> {
        let out = transport::no_console(&mut Command::new(&self.racket))
            .args(["-l", "racket/base", "-e", "(display (version))"])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let v = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!v.is_empty()).then_some(v)
    }

    /// The directory containing the executables.
    pub fn bin_dir(&self) -> Option<&Path> {
        self.racket.parent()
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// Find Racket: an explicit override (a `racket` binary or a directory containing one),
/// then `$PATH`, then the usual install locations.
pub fn discover(override_path: Option<&str>) -> Option<RacketInstall> {
    let found = |p: PathBuf| p.is_file().then_some(RacketInstall { racket: p });
    if let Some(o) = override_path.map(str::trim).filter(|s| !s.is_empty()) {
        let p = expand_tilde(o);
        if p.is_dir() {
            return found(p.join(exe("racket"))).or_else(|| found(p.join("bin").join(exe("racket"))));
        }
        return found(p);
    }
    candidate_dirs().into_iter().find_map(|d| found(d.join(exe("racket"))))
}

/// Directories searched for `racket`, in priority order.
pub fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // GUI apps on macOS don't inherit the shell's PATH, so also look in the usual places.
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin"] {
        dirs.push(PathBuf::from(d));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join("racket/bin"));
    }
    let mut installs: Vec<PathBuf> = ["/Applications", "/usr/local", "/opt", "C:\\Program Files"]
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|rd| rd.flatten().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("Racket") || n.starts_with("racket"))
        })
        .collect();
    // Newest version first ("Racket v9.3" > "Racket v8.18").
    installs.sort_by_key(|p| std::cmp::Reverse(version_key(p)));
    for i in installs {
        dirs.push(i.join("bin"));
        dirs.push(i.clone());
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

fn version_key(p: &Path) -> Vec<u32> {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).collect()
}

fn expand_tilde(p: &str) -> PathBuf {
    if let (Some(rest), Some(home)) = (p.strip_prefix("~/"), dirs::home_dir()) {
        home.join(rest)
    } else {
        PathBuf::from(p)
    }
}

/// Compilation state of the embedded scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendStatus {
    /// `raco make` is running; sessions work but start slower.
    Compiling,
    Ready,
    /// Compilation failed (sessions still work, compiling in memory).
    CompileFailed(String),
}

struct BackendInner {
    install: RacketInstall,
    dir: PathBuf,
    status: Mutex<BackendStatus>,
    ctx: Option<egui::Context>,
}

/// Installed backend scripts plus the Racket used to run them. Cheap to clone.
#[derive(Clone)]
pub struct Backend {
    inner: Arc<BackendInner>,
}

impl Backend {
    /// Install the scripts under the cache directory and start compiling them.
    pub fn start(install: RacketInstall, ctx: Option<egui::Context>) -> std::io::Result<Backend> {
        let dir = crate::config::cache_dir().join(format!("racket-{:016x}", scripts_hash(&install)));
        Self::start_in(install, dir, ctx)
    }

    /// Like [`Backend::start`], with an explicit directory.
    pub fn start_in(install: RacketInstall, dir: PathBuf, ctx: Option<egui::Context>) -> std::io::Result<Backend> {
        std::fs::create_dir_all(&dir)?;
        for (name, text) in SCRIPTS {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
                crate::config::atomic_write(&path, text.as_bytes())?;
            }
        }
        let backend = Backend {
            inner: Arc::new(BackendInner {
                install,
                dir,
                status: Mutex::new(BackendStatus::Compiling),
                ctx,
            }),
        };
        let b = backend.clone();
        std::thread::Builder::new().name("raco-make".into()).spawn(move || b.compile())?;
        Ok(backend)
    }

    fn compile(&self) {
        let inner = &self.inner;
        let files: Vec<PathBuf> = ["runner.rkt", "stepper.rkt", "server.rkt"].iter().map(|f| inner.dir.join(f)).collect();
        // `racket -l- raco make` works even when `raco` isn't next to `racket`.
        let out = transport::no_console(&mut Command::new(&inner.install.racket))
            .args(["-l-", "raco", "make"])
            .args(&files)
            .current_dir(&inner.dir)
            .stdin(Stdio::null())
            .output();
        let status = match out {
            Ok(o) if o.status.success() => BackendStatus::Ready,
            Ok(o) => BackendStatus::CompileFailed(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => BackendStatus::CompileFailed(e.to_string()),
        };
        *inner.status.lock() = status;
        if let Some(ctx) = &inner.ctx {
            ctx.request_repaint();
        }
    }

    pub fn install(&self) -> &RacketInstall {
        &self.inner.install
    }

    pub fn status(&self) -> BackendStatus {
        self.inner.status.lock().clone()
    }

    /// Block until compilation finishes or `timeout` elapses. Returns whether it finished.
    pub fn wait_compiled(&self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if self.status() != BackendStatus::Compiling {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    pub fn scripts_dir(&self) -> &Path {
        &self.inner.dir
    }

    pub(crate) fn ctx(&self) -> Option<egui::Context> {
        self.inner.ctx.clone()
    }

    /// `racket <script>` with `cwd` as the working directory.
    pub(crate) fn command(&self, script: &str, cwd: Option<&Path>) -> Command {
        transport::racket_command(&self.inner.install.racket, &self.inner.dir.join(script), cwd)
    }
}

/// Identifies the script set and the Racket that compiles it (FNV-1a).
fn scripts_hash(install: &RacketInstall) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for (name, text) in SCRIPTS {
        feed(name.as_bytes());
        feed(text.as_bytes());
    }
    feed(install.racket.to_string_lossy().as_bytes());
    h
}
