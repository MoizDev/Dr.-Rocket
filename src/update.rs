//! Updates: find a newer release on doctorocket.com, download it in the background, and swap it
//! in when the app quits (or restarts for it).
//!
//! The feed is `https://doctorocket.com/version.json` (website/app/version.json/route.ts):
//! `{"version": "1.0.0-beta.5", "downloads": {"mac": {"url", "sha256", "size"}, ...}}`.
//! A newer version is downloaded to `<cache>/update/`, checked against its SHA-256, unpacked, and
//! asked for its `--version` before it counts as ready. On quit the app replaces itself:
//!
//! - macOS: the whole `Dr. Rocket.app` bundle (from the DMG).
//! - Windows and Linux: the executable (from the zip or tarball). A running exe can be renamed
//!   but not deleted on Windows, so the old one is left as `<exe>.old` and removed next launch.
//!
//! Where the app can't replace itself (macOS running it from the disk image or a translocated
//! copy, a folder it can't write to, a development build), it only says that an update is
//! available and links to the download.
//!
//! Downloads and unpacking use the tools the OS ships (curl or wget, hdiutil, tar), so the app
//! carries no TLS stack. `DRROCKET_UPDATE_FEED` points at another feed (any curl URL, including
//! `file://`), for testing; it also enables checks in automation runs and development builds.

use parking_lot::Mutex;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const FEED: &str = "https://doctorocket.com/version.json";
const FEED_VAR: &str = "DRROCKET_UPDATE_FEED";
/// How often a running app looks for a new release.
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// The first check waits for startup to settle.
const FIRST_CHECK: Duration = Duration::from_secs(5);
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[cfg(target_os = "macos")]
const OS: &str = "mac";
#[cfg(windows)]
const OS: &str = "windows";
#[cfg(all(unix, not(target_os = "macos")))]
const OS: &str = "linux";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    /// A newer version to download from the website by hand; `why` says why it can't install
    /// itself.
    Available { version: String, why: String },
    Downloading { version: String },
    /// Downloaded and verified: installs when the app quits.
    Ready { version: String },
    Failed(String),
}

#[derive(Deserialize)]
struct Feed {
    version: String,
    #[serde(default)]
    downloads: std::collections::HashMap<String, Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    url: String,
    sha256: String,
}

/// What the app replaces when it updates.
#[derive(Clone, Debug)]
enum Target {
    /// macOS: the `.app` bundle.
    Bundle(PathBuf),
    /// Windows and Linux: the executable.
    Exe(PathBuf),
}

struct Shared {
    status: Mutex<Status>,
    /// Set when the status changes, with whether the change came from a check the user asked for.
    changed: Mutex<Option<bool>>,
    manual: AtomicBool,
    enabled: AtomicBool,
    ctx: egui::Context,
}

pub struct Updater {
    shared: Arc<Shared>,
    wake: Option<crossbeam_channel::Sender<()>>,
}

impl Updater {
    /// Starts background checks (every few hours, when `enabled`), and picks up an update
    /// downloaded by an earlier run.
    pub fn start(ctx: &egui::Context, enabled: bool) -> Updater {
        cleanup_old_exe();
        let shared = Arc::new(Shared {
            status: Mutex::new(Status::Idle),
            changed: Mutex::new(None),
            manual: AtomicBool::new(false),
            enabled: AtomicBool::new(enabled),
            ctx: ctx.clone(),
        });
        let overridden = std::env::var_os(FEED_VAR).is_some();
        // Scripted runs don't touch the network unless a test feed asks them to.
        if std::env::var_os("DRROCKET_AUTOMATION").is_some() && !overridden {
            return Updater { shared, wake: None };
        }
        if let Some(version) = staged_version() {
            *shared.status.lock() = Status::Ready { version };
        }
        let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let s = shared.clone();
        let _ = std::thread::Builder::new().name("updates".into()).spawn(move || {
            let mut wait = FIRST_CHECK;
            loop {
                let asked = match rx.recv_timeout(wait) {
                    Ok(()) => true,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => false,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
                };
                wait = INTERVAL;
                if asked || s.enabled.load(Ordering::Relaxed) {
                    s.manual.store(asked, Ordering::Relaxed);
                    check(&s);
                }
            }
        });
        Updater { shared, wake: Some(tx) }
    }

    pub fn status(&self) -> Status {
        self.shared.status.lock().clone()
    }

    pub fn set_enabled(&self, on: bool) {
        self.shared.enabled.store(on, Ordering::Relaxed);
    }

    /// Check now (Check for Updates…). Returns false when updates are off for this run.
    pub fn check_now(&self) -> bool {
        match &self.wake {
            Some(tx) => tx.send(()).is_ok(),
            None => false,
        }
    }

    /// The status, when it changed since the last call, and whether the user asked for it.
    pub fn take_change(&self) -> Option<(Status, bool)> {
        let asked = self.shared.changed.lock().take()?;
        Some((self.status(), asked))
    }

    /// The new version when one is downloaded and ready to install.
    pub fn ready(&self) -> Option<String> {
        match self.status() {
            Status::Ready { version } => Some(version),
            _ => None,
        }
    }

    /// Install a downloaded update (called as the app exits), and with `relaunch` start the new
    /// version. Returns true when the new version was started. A failed install leaves the app
    /// as it was and is reported on the next launch.
    pub fn install(&self, relaunch: bool) -> bool {
        let Some(version) = self.ready() else { return false };
        let result = target().and_then(|t| replace(&t, &staged_payload(&version)?).map(|()| t));
        match result {
            Ok(t) => {
                let _ = std::fs::remove_dir_all(update_dir());
                relaunch && launch(&t)
            }
            Err(e) => {
                eprintln!("update: install failed: {e}");
                let _ = std::fs::remove_dir_all(update_dir());
                let _ = std::fs::create_dir_all(update_dir());
                let _ = std::fs::write(update_dir().join("failed"), format!("{version}\n{e}\n"));
                false
            }
        }
    }

    /// A failed install from the last run, once: (version, reason).
    pub fn take_failure(&self) -> Option<(String, String)> {
        let path = update_dir().join("failed");
        let text = std::fs::read_to_string(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        let mut lines = text.lines();
        Some((lines.next()?.to_string(), lines.next().unwrap_or("unknown error").to_string()))
    }
}

fn set(s: &Shared, status: Status) {
    *s.status.lock() = status;
    *s.changed.lock() = Some(s.manual.load(Ordering::Relaxed));
    s.ctx.request_repaint();
}

fn check(s: &Shared) {
    let current = s.status.lock().clone();
    if let Status::Ready { .. } | Status::Downloading { .. } = current {
        // Already have it; a manual check just says so again.
        if s.manual.load(Ordering::Relaxed) {
            set(s, current);
        }
        return;
    }
    set(s, Status::Checking);
    let feed = match fetch_feed() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("update: check failed: {e}");
            return set(s, Status::Failed(format!("Couldn't reach doctorocket.com ({e}).")));
        }
    };
    if !newer(&feed.version, CURRENT) {
        return set(s, Status::UpToDate);
    }
    let version = feed.version.clone();
    let manual_only = |why: String| Status::Available { version: version.clone(), why };
    let asset = match feed.downloads.get(OS) {
        Some(a) => a.clone(),
        None => return set(s, manual_only("The update for this system isn't ready to install automatically yet.".into())),
    };
    if let Err(why) = target() {
        return set(s, manual_only(why));
    }
    set(s, Status::Downloading { version: version.clone() });
    match download(&version, &asset) {
        Ok(()) => set(s, Status::Ready { version }),
        Err(e) => {
            eprintln!("update: download failed: {e}");
            let _ = std::fs::remove_dir_all(update_dir());
            set(s, manual_only(format!("The download didn't finish ({e}).")))
        }
    }
}

/// The website's download link for this system (counts the download, then sends the file).
pub fn download_page() -> String {
    format!("https://doctorocket.com/download/{OS}")
}

fn feed_url() -> String {
    std::env::var(FEED_VAR).unwrap_or_else(|_| FEED.to_string())
}

fn fetch_feed() -> Result<Feed, String> {
    let dir = update_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("feed-{}.json", std::process::id()));
    let fetched = fetch(&feed_url(), &path, Duration::from_secs(30));
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    fetched?;
    serde_json::from_str(&text.map_err(|e| e.to_string())?).map_err(|e| format!("bad feed: {e}"))
}

// ---- Versions ----

/// Whether version `a` comes after `b` (semver precedence: 1.0.0-beta.2 < 1.0.0-beta.10 < 1.0.0).
pub fn newer(a: &str, b: &str) -> bool {
    match (parse(a), parse(b)) {
        (Some(a), Some(b)) => cmp(&a, &b) == std::cmp::Ordering::Greater,
        _ => false,
    }
}

type Version<'a> = ([u64; 3], Vec<&'a str>);

fn parse(v: &str) -> Option<Version<'_>> {
    let v = v.trim().trim_start_matches('v');
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, p.split('.').collect()),
        None => (v, Vec::new()),
    };
    let mut nums = core.split('.').map(|n| n.parse::<u64>().ok());
    let out = [nums.next()??, nums.next()??, nums.next()??];
    nums.next().is_none().then_some((out, pre))
}

fn cmp(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    a.0.cmp(&b.0).then_with(|| match (a.1.is_empty(), b.1.is_empty()) {
        (true, true) => Equal,
        // A release comes after its prereleases.
        (true, false) => Greater,
        (false, true) => Less,
        (false, false) => {
            for (x, y) in a.1.iter().zip(&b.1) {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Less,
                    (Err(_), Ok(_)) => Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if o != Equal {
                    return o;
                }
            }
            a.1.len().cmp(&b.1.len())
        }
    })
}

// ---- Where the app lives ----

/// What to replace, or why this copy can't update itself.
fn target() -> Result<Target, String> {
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| format!("Couldn't find the app on disk ({e})."))?;
    if std::env::var_os(FEED_VAR).is_none() && is_dev_build(&exe) {
        return Err("This is a development build.".into());
    }
    let t = if cfg!(target_os = "macos") {
        let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).ok_or("Dr. Rocket isn't running from an app bundle.")?;
        let s = bundle.to_string_lossy();
        if s.contains("/AppTranslocation/") || s.starts_with("/Volumes/") {
            return Err("Dr. Rocket is running from the download, not from Applications. Drag it into Applications to get updates automatically.".into());
        }
        Target::Bundle(bundle.to_path_buf())
    } else {
        Target::Exe(exe)
    };
    let path = match &t {
        Target::Bundle(p) | Target::Exe(p) => p,
    };
    let dir = path.parent().ok_or("The app has no folder.")?;
    if !writable(dir) {
        return Err(format!("Dr. Rocket can't write to {}, so it can't replace itself.", dir.display()));
    }
    Ok(t)
}

/// Whether this is a development build (see `is_dev_build`).
pub fn running_dev_build() -> bool {
    std::env::current_exe().and_then(|p| p.canonicalize()).is_ok_and(|exe| is_dev_build(&exe))
}

/// A binary in a Cargo target directory (it holds a CACHEDIR.TAG).
fn is_dev_build(exe: &Path) -> bool {
    exe.ancestors().any(|p| p.file_name().is_some_and(|n| n == "target") && p.join("CACHEDIR.TAG").is_file())
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".drrocket-update-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn update_dir() -> PathBuf {
    // A test config (DRROCKET_CONFIG_DIR) gets its own, so a test download is never what the
    // real app finds and installs.
    match std::env::var_os("DRROCKET_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir).join("update"),
        None => crate::config::cache_dir().join("update"),
    }
}

fn staged_dir(version: &str) -> PathBuf {
    update_dir().join(version)
}

/// The unpacked app or executable for `version`, once verified.
fn staged_payload(version: &str) -> Result<PathBuf, String> {
    let dir = staged_dir(version);
    let name = std::fs::read_to_string(dir.join("ready")).map_err(|_| "The downloaded update is missing.".to_string())?;
    let p = dir.join(name.trim());
    if p.exists() { Ok(p) } else { Err("The downloaded update is missing.".into()) }
}

/// A verified update left by an earlier run, if it's still newer than this one. Clears out
/// anything else in the update folder.
fn staged_version() -> Option<String> {
    let entries = std::fs::read_dir(update_dir()).ok()?;
    let mut best: Option<String> = None;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "failed" {
            continue;
        }
        let keep = newer(&name, CURRENT) && staged_payload(&name).is_ok() && best.as_deref().is_none_or(|b| newer(&name, b));
        if keep {
            if let Some(old) = best.replace(name) {
                let _ = std::fs::remove_dir_all(staged_dir(&old));
            }
        } else {
            let _ = if e.path().is_dir() { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
        }
    }
    best
}

// ---- Download and unpack ----

fn download(version: &str, asset: &Asset) -> Result<(), String> {
    let dir = staged_dir(version);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(match OS {
        "mac" => "update.dmg",
        "windows" => "update.zip",
        _ => "update.tar.gz",
    });
    fetch(&asset.url, &file, Duration::from_secs(30 * 60))?;
    let sum = sha256(&file).map_err(|e| e.to_string())?;
    if !sum.eq_ignore_ascii_case(asset.sha256.trim()) {
        return Err("the file didn't match its checksum".into());
    }
    let payload = unpack(&file, &dir)?;
    let _ = std::fs::remove_file(&file);
    let exe = match OS {
        "mac" => payload.join("Contents/MacOS/drrocket"),
        _ => payload.clone(),
    };
    let reported = version_of(&exe)?;
    if reported != version {
        return Err(format!("the download is version {reported}, not {version}"));
    }
    let name = payload.strip_prefix(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("ready"), name.to_string_lossy().as_bytes()).map_err(|e| e.to_string())
}

/// Unpacks the download into `dir` and returns the app bundle or executable in it.
fn unpack(file: &Path, dir: &Path) -> Result<PathBuf, String> {
    let out = dir.join("app");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    if cfg!(target_os = "macos") {
        let mnt = dir.join("mnt");
        std::fs::create_dir_all(&mnt).map_err(|e| e.to_string())?;
        run(Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-noverify", "-mountpoint"]).arg(&mnt).arg(file))?;
        let copied = (|| {
            let app = std::fs::read_dir(&mnt)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e == "app"))
                .ok_or("no app in the disk image")?;
            let dest = out.join(app.file_name().unwrap_or_default());
            run(Command::new("ditto").arg(&app).arg(&dest))?;
            Ok::<_, String>(dest)
        })();
        let _ = run(Command::new("hdiutil").args(["detach", "-force"]).arg(&mnt));
        let _ = std::fs::remove_dir(&mnt);
        let app = copied?;
        let _ = run(Command::new("xattr").args(["-dr", "com.apple.quarantine"]).arg(&app));
        Ok(app)
    } else {
        // Windows 10 and later ship bsdtar, which reads zips too.
        let tar = if cfg!(windows) { system_tool("tar.exe") } else { PathBuf::from("tar") };
        run(tool(&tar).arg("-xf").arg(file).arg("-C").arg(&out))?;
        let name = if cfg!(windows) { "Dr. Rocket.exe" } else { "drrocket" };
        find_file(&out, name, 3).ok_or_else(|| format!("no {name} in the download"))
    }
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    if let Some(p) = entries.iter().find(|p| p.is_file() && p.file_name().is_some_and(|n| n == name)) {
        return Some(p.clone());
    }
    if depth == 0 {
        return None;
    }
    entries.iter().filter(|p| p.is_dir()).find_map(|p| find_file(p, name, depth - 1))
}

/// The version an executable reports with `--version` (it must answer within a few seconds).
fn version_of(exe: &Path) -> Result<String, String> {
    let mut child = tool(exe).arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("the new version won't start ({e})"))?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < Duration::from_secs(15) => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return Err("the new version didn't answer".into());
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take().ok_or("no output")?, &mut out).map_err(|e| e.to_string())?;
    Ok(out.trim().to_string())
}

/// Download `url` to `dest` with curl (or wget where there's no curl).
fn fetch(url: &str, dest: &Path, timeout: Duration) -> Result<(), String> {
    let agent = format!("Dr. Rocket/{CURRENT} ({OS})");
    let secs = timeout.as_secs().to_string();
    let curl = if cfg!(windows) { system_tool("curl.exe") } else { PathBuf::from("curl") };
    let mut c = tool(&curl);
    c.args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "--max-time", &secs, "-A", &agent, "-o"]).arg(dest).arg(url);
    if cfg!(windows) {
        // Behind proxies that block certificate revocation lookups, check when possible only.
        c.arg("--ssl-revoke-best-effort");
    }
    match c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(first_line(&o.stderr).unwrap_or_else(|| format!("curl exited with {}", o.status))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !cfg!(windows) => {
            let o = tool(Path::new("wget")).args(["-q", "-T", "20", "-U", &agent, "-O"]).arg(dest).arg(url).stdin(Stdio::null()).output().map_err(|_| "neither curl nor wget is installed".to_string())?;
            if o.status.success() { Ok(()) } else { Err(format!("wget exited with {}", o.status)) }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes).lines().map(str::trim).find(|l| !l.is_empty()).map(String::from)
}

fn sha256(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// A command for a helper tool, without a console window flashing up on Windows.
fn tool(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// `C:\Windows\System32\<name>`, so a same-named program elsewhere on PATH isn't picked up.
fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(name)
}

fn run(c: &mut Command) -> Result<(), String> {
    let o = c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output().map_err(|e| format!("{:?}: {e}", c.get_program()))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(first_line(&o.stderr).unwrap_or_else(|| format!("{:?} exited with {}", c.get_program(), o.status)))
    }
}

// ---- Install ----

/// The previous executable, renamed out of the way (see `replace`).
fn old_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Delete the executable a Windows update left behind (it was still running then). Right after
/// Restart to Update the old process may still be exiting, so keep trying for a while.
fn cleanup_old_exe() {
    let Ok(exe) = std::env::current_exe() else { return };
    let old = old_path(&exe);
    if !old.exists() {
        return;
    }
    let _ = std::thread::Builder::new().name("update-cleanup".into()).spawn(move || {
        for _ in 0..120 {
            if std::fs::remove_file(&old).is_ok() || !old.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

/// Swap `new` in for the installed app, putting the old one back if anything fails.
fn replace(t: &Target, new: &Path) -> Result<(), String> {
    match t {
        Target::Bundle(app) => {
            let parent = app.parent().ok_or("no folder")?;
            let name = app.file_name().ok_or("no name")?.to_string_lossy().into_owned();
            let aside = parent.join(format!(".{name}.old-{}", std::process::id()));
            // The running app keeps working from memory while its bundle moves.
            std::fs::rename(app, &aside).map_err(|e| format!("couldn't move the old app aside ({e})"))?;
            if let Err(e) = move_or_copy(new, app) {
                let _ = std::fs::remove_dir_all(app);
                let _ = std::fs::rename(&aside, app);
                return Err(format!("couldn't put the new app in place ({e})"));
            }
            let _ = std::fs::remove_dir_all(&aside);
            Ok(())
        }
        Target::Exe(exe) => {
            let old = old_path(exe);
            let _ = std::fs::remove_file(&old);
            std::fs::rename(exe, &old).map_err(|e| format!("couldn't move the old version aside ({e})"))?;
            if let Err(e) = move_or_copy(new, exe) {
                let _ = std::fs::remove_file(exe);
                let _ = std::fs::rename(&old, exe);
                return Err(format!("couldn't put the new version in place ({e})"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755));
            }
            // Windows can't delete the running exe: the next launch does (cleanup_old_exe).
            if !cfg!(windows) {
                let _ = std::fs::remove_file(&old);
            }
            Ok(())
        }
    }
}

/// Rename, or copy when `from` is on another volume.
fn move_or_copy(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if cfg!(target_os = "macos") {
        run(Command::new("ditto").arg(from).arg(to))
    } else {
        std::fs::copy(from, to).map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Start the newly installed version.
fn launch(t: &Target) -> bool {
    // Scripted runs start the binary directly so the new copy inherits their environment
    // (an invisible window, a test config); otherwise macOS apps open through Launch Services.
    let automated = std::env::var_os("DRROCKET_AUTOMATION").is_some();
    match t {
        Target::Bundle(app) if !automated => Command::new("open").arg("-n").arg(app).spawn().is_ok(),
        Target::Bundle(app) => Command::new(app.join("Contents/MacOS/drrocket")).spawn().is_ok(),
        Target::Exe(exe) => Command::new(exe).spawn().is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn versions_order_like_semver() {
        assert!(newer("1.0.0-beta.5", "1.0.0-beta.4"));
        assert!(newer("1.0.0-beta.10", "1.0.0-beta.9"));
        assert!(newer("1.0.0", "1.0.0-beta.9"));
        assert!(newer("1.0.1", "1.0.0"));
        assert!(newer("v1.1.0", "1.0.9"));
        assert!(newer("1.0.0-rc.1", "1.0.0-beta.9"));
        assert!(!newer("1.0.0-beta.4", "1.0.0-beta.4"));
        assert!(!newer("1.0.0-beta.3", "1.0.0-beta.4"));
        assert!(!newer("1.0.0-beta.9", "1.0.0"));
        assert!(!newer("garbage", "1.0.0"));
    }
}
