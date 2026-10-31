//! Updates: find a newer release on doctorocket.com, download it in the background, and swap it
//! in when the app quits (or restarts for it).
//!
//! The feed is `https://doctorocket.com/version.json` (website/app/version.json/route.ts):
//! `{"version": "1.0.0-beta.5", "downloads": {"mac": {"url", "sha256", "size"}, ...}}`.
//! A newer version requires an Ed25519 signature from the key pinned at build time. Its
//! archive is checked against the signed SHA-256, retained, and reverified before extraction.
//! Downloaded programs are never executed to inspect their version. On quit the app replaces itself:
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
use serde::{Deserialize, Serialize};
use base64::Engine;
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
#[cfg(not(feature = "updater-e2e"))]
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
#[cfg(feature = "updater-e2e")]
include!(concat!(env!("OUT_DIR"), "/update_test_config.rs"));
#[cfg(feature = "updater-e2e")]
pub const CURRENT: &str = TEST_VERSION;

#[cfg(feature = "updater-e2e")]
fn test_sandbox() -> Result<(), String> {
    let root = Path::new(TEST_ROOT);
    let config = std::env::var_os("DRROCKET_CONFIG_DIR").ok_or("E2E config directory is required")?;
    let config = Path::new(&config).canonicalize().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| e.to_string())?;
    if !config.starts_with(root) || config == root || !exe.starts_with(root) {
        return Err("E2E executable and config must stay inside the compiled scratch root".into());
    }
    Ok(())
}

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

#[derive(Deserialize, Serialize, Clone)]
struct Asset {
    url: String,
    sha256: String,
    #[serde(default)]
    signature: String,
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
        let shared = Arc::new(Shared {
            status: Mutex::new(Status::Idle),
            changed: Mutex::new(None),
            manual: AtomicBool::new(false),
            enabled: AtomicBool::new(enabled),
            ctx: ctx.clone(),
        });
        #[cfg(feature = "updater-e2e")]
        if let Err(why) = test_sandbox() {
            *shared.status.lock() = Status::Failed(why);
            return Updater { shared, wake: None };
        }
        cleanup_old_exe();
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
    if let Err(why) = verify_asset(&version, &asset) {
        return set(s, manual_only(why));
    }
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
        (Some(a), Some(b)) => a.cmp_precedence(&b) == std::cmp::Ordering::Greater,
        _ => false,
    }
}

fn parse(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.strip_prefix('v').unwrap_or(v)).ok()
}

// ---- Where the app lives ----

/// What to replace, or why this copy can't update itself.
fn target() -> Result<Target, String> {
    #[cfg(feature = "updater-e2e")]
    test_sandbox()?;
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
    let ok = std::fs::OpenOptions::new().write(true).create_new(true).open(&probe).is_ok();
    if ok { let _ = std::fs::remove_file(&probe); }
    ok
}

fn update_dir() -> PathBuf {
    #[cfg(feature = "updater-e2e")]
    test_sandbox().expect("scratch-only updater test build used outside its compiled sandbox");
    // A test config (DRROCKET_CONFIG_DIR) gets its own, so a test download is never what the
    // real app finds and installs.
    match std::env::var_os("DRROCKET_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir).join("update"),
        None => crate::config::cache_dir().join("update"),
    }
}

fn staged_dir(version: &str) -> Result<PathBuf, String> {
    parse(version).ok_or("Invalid update version")?;
    let dir = update_dir().join(version);
    if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Update folder must not be a symbolic link".into());
    }
    Ok(dir)
}

#[derive(Deserialize, Serialize)]
struct Receipt {
    version: String,
    asset: Asset,
}

fn signature_message(version: &str, os: &str, asset: &Asset) -> String {
    format!("drrocket-update-v1\n{version}\n{os}\n{}\n{}\n", asset.sha256, asset.url)
}

fn verify_with_key(version: &str, os: &str, asset: &Asset, key: &str) -> Result<(), String> {
    parse(version).ok_or("Invalid signed version")?;
    if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || !asset.url.starts_with("https://") || asset.url.chars().any(|c| c.is_control()) {
        return Err("Invalid signed update metadata".into());
    }
    let decode = |s: &str| base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| "Invalid update signature encoding".to_string());
    let key: [u8; 32] = decode(key)?.try_into().map_err(|_| "Invalid update public key")?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| "Invalid update public key")?;
    let signature = ed25519_dalek::Signature::from_slice(&decode(&asset.signature)?).map_err(|_| "Invalid update signature")?;
    key.verify_strict(signature_message(version, os, asset).as_bytes(), &signature).map_err(|_| "The update is not signed by the trusted release key".into())
}

fn verify_asset(version: &str, asset: &Asset) -> Result<(), String> {
    #[cfg(not(feature = "updater-e2e"))]
    { verify_with_key(version, OS, asset, RELEASE_KEY.trim()) }
    #[cfg(feature = "updater-e2e")]
    {
        test_sandbox()?;
        if !asset.url.starts_with(&format!("{TEST_ORIGIN}/")) { return Err("E2E assets must use the compiled loopback HTTPS origin".into()); }
        verify_with_key(version, OS, asset, TEST_PUBLIC_KEY)
    }
}

/// The public half of the release signing key (`release-key.pub`, base64 of the raw 32 bytes).
/// Every build has it, so no release can ship without the ability to verify its updates.
#[cfg_attr(feature = "updater-e2e", allow(dead_code))]
const RELEASE_KEY: &str = include_str!("../release-key.pub");

fn archive_name() -> &'static str {
    match OS { "mac" => "update.dmg", "windows" => "update.zip", _ => "update.tar.gz" }
}

/// Recheck signatures and bytes on every launch and immediately before installing.
fn verified_archive(version: &str) -> Result<PathBuf, String> {
    let dir = staged_dir(version)?;
    let receipt: Receipt = serde_json::from_slice(&std::fs::read(dir.join("receipt.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if receipt.version != version { return Err("Cached update version mismatch".into()); }
    verify_asset(version, &receipt.asset)?;
    let archive = dir.join(archive_name());
    if std::fs::symlink_metadata(&archive).map_err(|e| e.to_string())?.file_type().is_symlink() {
        return Err("Cached archive must not be a symbolic link".into());
    }
    if sha256(&archive).map_err(|e| e.to_string())? != receipt.asset.sha256 {
        return Err("Cached update checksum mismatch".into());
    }
    Ok(archive)
}

fn staged_payload(version: &str) -> Result<PathBuf, String> {
    let archive = verified_archive(version)?;
    let dir = staged_dir(version)?;
    // Never install an extracted tree left by another run.
    let _ = std::fs::remove_dir_all(dir.join("app"));
    unpack(&archive, &dir)
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
        let keep = newer(&name, CURRENT) && verified_archive(&name).is_ok() && best.as_deref().is_none_or(|b| newer(&name, b));
        if keep {
            if let Some(old) = best.replace(name)
                && let Ok(dir) = staged_dir(&old) {
                let _ = std::fs::remove_dir_all(dir);
            }
        } else {
            let _ = if e.path().is_dir() { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
        }
    }
    best
}

// ---- Download and unpack ----

fn download(version: &str, asset: &Asset) -> Result<(), String> {
    verify_asset(version, asset)?;
    let dir = staged_dir(version)?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(archive_name());
    fetch(&asset.url, &file, Duration::from_secs(30 * 60))?;
    if sha256(&file).map_err(|e| e.to_string())? != asset.sha256 {
        return Err("the file didn't match its signed checksum".into());
    }
    let receipt = Receipt { version: version.into(), asset: asset.clone() };
    crate::config::atomic_write(&dir.join("receipt.json"), &serde_json::to_vec(&receipt).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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

/// Download `url` to `dest` with curl (or wget where there's no curl).
fn fetch(url: &str, dest: &Path, timeout: Duration) -> Result<(), String> {
    let agent = format!("Dr. Rocket/{CURRENT} ({OS})");
    let secs = timeout.as_secs().to_string();
    let curl = if cfg!(windows) { system_tool("curl.exe") } else { PathBuf::from("curl") };
    let mut c = tool(&curl);
    // A feed override can be a local JSON fixture; signed assets always require HTTPS.
    // HTTPS requests may never redirect to HTTP/file, including test builds.
    if url.starts_with("https://") {
        c.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    #[cfg(feature = "updater-e2e")]
    {
        test_sandbox()?;
        if !url.starts_with(&format!("{TEST_ORIGIN}/")) { return Err("E2E fetch outside loopback origin".into()); }
        let ca = update_dir().join("test-ca.pem");
        std::fs::create_dir_all(update_dir()).map_err(|e| e.to_string())?;
        std::fs::write(&ca, TEST_CA).map_err(|e| e.to_string())?;
        c.arg("--cacert").arg(ca);
    }
    c.args(["-fsSL", "--retry", "2", "--connect-timeout", "20", "--max-time", &secs, "-A", &agent, "-o"]).arg(dest).arg(url);
    if cfg!(windows) {
        // Behind proxies that block certificate revocation lookups, check when possible only.
        c.arg("--ssl-revoke-best-effort");
    }
    match c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(first_line(&o.stderr).unwrap_or_else(|| format!("curl exited with {}", o.status))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !cfg!(windows) && !cfg!(feature = "updater-e2e") => {
            let o = tool(Path::new("wget")).args(["--https-only", "-q", "-T", "20", "-U", &agent, "-O"]).arg(dest).arg(url).stdin(Stdio::null()).output().map_err(|_| "neither curl nor wget is installed".to_string())?;
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
    // Scripted runs start directly so the new copy inherits the automation/config sandbox.
    let automated = std::env::var_os("DRROCKET_AUTOMATION").is_some();
    let mut command = match t {
        Target::Bundle(app) if !automated => {
            let mut c = Command::new("open");
            c.arg("-n").arg(app);
            c
        }
        Target::Bundle(app) => Command::new(app.join("Contents/MacOS/drrocket")),
        Target::Exe(exe) => Command::new(exe),
    };
    match command.spawn() {
        Ok(child) => {
            // Lets the scratch harness reap a relaunched app even if its assertion fails.
            #[cfg(feature = "updater-e2e")]
            let _ = std::fs::write(Path::new(TEST_ROOT).join("child.pid"), child.id().to_string());
            let _ = child;
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn signatures_bind_version_platform_checksum_and_url() {
        use base64::Engine;
        use ed25519_dalek::Signer;
        let key = ed25519_dalek::SigningKey::from_bytes(&[42; 32]);
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let public = encode(key.verifying_key().as_bytes());
        let mut asset = super::Asset { url: "https://doctorocket.com/downloads/test.zip".into(), sha256: "a".repeat(64), signature: String::new() };
        asset.signature = encode(&key.sign(super::signature_message("1.2.3", "windows", &asset).as_bytes()).to_bytes());
        assert!(super::verify_with_key("1.2.3", "windows", &asset, &public).is_ok());
        assert!(super::verify_with_key("1.2.4", "windows", &asset, &public).is_err());
        assert!(super::verify_with_key("1.2.3", "linux", &asset, &public).is_err());
        assert!(super::verify_with_key("1.2.3", "windows", &asset, "").is_err());
        let other = ed25519_dalek::SigningKey::from_bytes(&[43; 32]);
        assert!(super::verify_with_key("1.2.3", "windows", &asset, &encode(other.verifying_key().as_bytes())).is_err());
        let mut changed = asset.clone();
        changed.sha256 = "b".repeat(64);
        assert!(super::verify_with_key("1.2.3", "windows", &changed, &public).is_err());
        changed = asset.clone();
        changed.url.push_str("?tampered");
        assert!(super::verify_with_key("1.2.3", "windows", &changed, &public).is_err());
        assert!(super::verify_with_key("1.2.3", "windows", &asset, super::RELEASE_KEY.trim()).is_err());
        #[cfg(not(feature = "updater-e2e"))]
        {
            let mut native = asset.clone();
            native.signature = encode(&key.sign(super::signature_message("1.2.3", super::OS, &native).as_bytes()).to_bytes());
            assert!(super::verify_asset("1.2.3", &native).is_err(), "production must reject test keys, including with feed overrides");
        }
        for url in ["file:///tmp/test.zip", "http://localhost/test.zip"] {
            let mut insecure = asset.clone();
            insecure.url = url.into();
            insecure.signature = encode(&key.sign(super::signature_message("1.2.3", "windows", &insecure).as_bytes()).to_bytes());
            assert!(super::verify_with_key("1.2.3", "windows", &insecure, &public).is_err());
        }
        changed = asset;
        changed.signature.clear();
        assert!(super::verify_with_key("1.2.3", "windows", &changed, &public).is_err());
    }

    /// Transport/staging/receipt regression using the HTTPS fixture orchestrator. On macOS this
    /// stops before native extraction; the harness also runs the real app for native installs.
    #[cfg(feature = "updater-e2e")]
    #[test]
    #[ignore = "requires tools/update-e2e.py scratch HTTPS fixture"]
    fn signed_https_staging() {
        use super::*;
        let feed = fetch_feed().expect("local HTTPS feed");
        let asset = &feed.downloads[OS];
        download(&feed.version, asset).expect("signed HTTPS download");
        let archive = verified_archive(&feed.version).expect("reverified receipt and bytes");
        assert_eq!(staged_version().as_deref(), Some("99.0.0"));
        let receipt = staged_dir(&feed.version).unwrap().join("receipt.json");
        let good = std::fs::read(&receipt).unwrap();
        let mut bad: Receipt = serde_json::from_slice(&good).unwrap();
        bad.version = "99.0.1".into();
        std::fs::write(&receipt, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(verified_archive(&feed.version).is_err());
        std::fs::write(&receipt, good).unwrap();
        std::fs::write(&archive, b"corrupted after download").unwrap();
        assert!(verified_archive(&feed.version).is_err());
        assert!(staged_payload(&feed.version).is_err());
        let mut mismatched = asset.clone();
        mismatched.sha256 = "0".repeat(64);
        assert!(download(&feed.version, &mismatched).is_err());
        assert!(download("99.0.1", asset).is_err());
        assert!(download(&feed.version, &Asset { signature: String::new(), ..asset.clone() }).is_err());
        for name in ["mismatch", "wrong-key"] {
            let path = Path::new(TEST_ROOT).join(format!("{name}.json"));
            let bad: Feed = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert!(download(&bad.version, &bad.downloads[OS]).is_err());
        }
        download(&feed.version, asset).unwrap();
        assert!(verified_archive(&feed.version).is_ok());
    }

    #[test]
    fn reject_versions_and_payloads_that_escape_staging() {
        for version in ["99.0.0-payload/../../outside", "99.0.0+../../outside", "99.0.0-..", "99.0.0-", "01.0.0", "99.0.0-beta.01", " 99.0.0", "99.0.0-evil\\path"] {
            assert!(!newer(version, "1.0.0"), "{version}");
            assert!(super::staged_dir(version).is_err(), "{version}");
        }

    }

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
