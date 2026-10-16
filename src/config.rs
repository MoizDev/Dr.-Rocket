//! User settings, recent projects, and on-disk locations.
//!
//! Settings live in `<config_dir>/settings.json`. On first launch the defaults are seeded
//! from Zed's `settings.json` (fonts, cursor, rainbow brackets...) so the editor feels the
//! same as the user's Zed setup.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RACKETEER_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("racketeer")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("racketeer")
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorShape {
    Bar,
    Block,
    Underline,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub buffer_font_size: f32,
    pub ui_font_size: f32,
    pub terminal_font_size: f32,
    pub line_height: f32,
    pub cursor_shape: CursorShape,
    pub cursor_blink: bool,
    pub rainbow_brackets: bool,
    pub indent_guides: bool,
    pub show_line_numbers: bool,
    pub highlight_active_line: bool,
    pub wrap_guide_column: Option<usize>,
    pub autosave: bool,
    pub autosave_delay_ms: u64,
    pub show_tab_bar: bool,
    pub auto_close_brackets: bool,
    pub online_check_syntax: bool,
    pub show_binding_arrows: bool,
    pub racket_path: Option<String>,
    pub shell: Option<String>,
    pub restore_session: bool,
    /// Default language for new, unsaved files: `#lang racket` or a teaching language id.
    pub default_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: crate::theme::DEFAULT_THEME.to_string(),
            buffer_font_size: 16.0,
            ui_font_size: 14.0,
            terminal_font_size: 15.0,
            line_height: 1.55,
            cursor_shape: CursorShape::Bar,
            cursor_blink: true,
            rainbow_brackets: true,
            indent_guides: true,
            show_line_numbers: true,
            highlight_active_line: true,
            wrap_guide_column: Some(102),
            autosave: false,
            autosave_delay_ms: 1000,
            show_tab_bar: true,
            auto_close_brackets: true,
            online_check_syntax: true,
            show_binding_arrows: true,
            racket_path: None,
            shell: None,
            restore_session: true,
            default_language: "module".to_string(),
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.json")
    }

    /// Load settings, seeding from Zed on first launch.
    pub fn load() -> Self {
        let path = Self::path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            let cleaned = crate::theme::strip_json_comments(&text);
            match serde_json::from_str(&cleaned) {
                Ok(s) => return s,
                Err(e) => eprintln!("racketeer: ignoring invalid {}: {e}", path.display()),
            }
            return Settings::default();
        }
        let s = Self::seed_from_zed().unwrap_or_default();
        let _ = s.save();
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).expect("settings serialize");
        atomic_write(&path, text.as_bytes())
    }

    fn seed_from_zed() -> Option<Settings> {
        let path = dirs::home_dir()?.join(".config/zed/settings.json");
        let text = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&crate::theme::strip_json_comments(&text)).ok()?;
        let mut s = Settings::default();
        if let Some(f) = v.get("buffer_font_size").and_then(|x| x.as_f64()) {
            s.buffer_font_size = f as f32;
        }
        if let Some(f) = v.get("ui_font_size").and_then(|x| x.as_f64()) {
            s.ui_font_size = (f as f32 - 1.0).max(11.0);
        }
        if let Some(f) = v.pointer("/terminal/font_size").and_then(|x| x.as_f64()) {
            s.terminal_font_size = f as f32;
        }
        match v.get("cursor_shape").and_then(|x| x.as_str()) {
            Some("block") => s.cursor_shape = CursorShape::Block,
            Some("underline") => s.cursor_shape = CursorShape::Underline,
            Some("bar") => s.cursor_shape = CursorShape::Bar,
            _ => {}
        }
        if let Some(b) = v.get("cursor_blink").and_then(|x| x.as_bool()) {
            s.cursor_blink = b;
        }
        if let Some(b) = v.get("colorize_brackets").and_then(|x| x.as_bool()) {
            s.rainbow_brackets = b;
        }
        if let Some(b) = v.pointer("/indent_guides/enabled").and_then(|x| x.as_bool()) {
            s.indent_guides = b;
        }
        if let Some(t) = v.pointer("/theme/dark").and_then(|x| x.as_str()) {
            s.theme = t.to_string();
        } else if let Some(t) = v.get("theme").and_then(|x| x.as_str()) {
            s.theme = t.to_string();
        }
        Some(s)
    }
}

/// Write a file atomically (temp file + rename) so a crash never leaves a truncated file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.racketeer-tmp"));
    std::fs::write(&tmp, bytes)?;
    // Preserve permissions of an existing file.
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecentProject {
    pub path: PathBuf,
    /// Unix seconds.
    pub opened_at: u64,
}

/// Recently opened projects (folders) and files, newest first.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Recents {
    pub projects: Vec<RecentProject>,
    pub files: Vec<RecentProject>,
}

impl Recents {
    const MAX: usize = 24;

    fn path() -> PathBuf {
        config_dir().join("recent.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = atomic_write(&Self::path(), s.as_bytes());
        }
    }

    fn push(list: &mut Vec<RecentProject>, path: &Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        list.retain(|p| p.path != path);
        list.insert(0, RecentProject { path, opened_at: now_secs() });
        list.truncate(Self::MAX);
    }

    pub fn add_project(&mut self, path: &Path) {
        Self::push(&mut self.projects, path);
        self.save();
    }

    pub fn add_file(&mut self, path: &Path) {
        Self::push(&mut self.files, path);
        self.save();
    }

    /// Drop entries that no longer exist on disk.
    pub fn prune(&mut self) {
        self.projects.retain(|p| p.path.exists());
        self.files.retain(|p| p.path.exists());
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "3 minutes ago"-style relative time.
pub fn relative_time(then: u64) -> String {
    let d = now_secs().saturating_sub(then);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => plural(d / 60, "minute"),
        3600..=86_399 => plural(d / 3600, "hour"),
        86_400..=2_591_999 => plural(d / 86_400, "day"),
        _ => plural(d / 2_592_000, "month"),
    }
}

fn plural(n: u64, unit: &str) -> String {
    if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") }
}

/// Shorten a path for display by replacing the home directory with `~`.
pub fn tildify(p: &Path) -> String {
    if let Some(home) = dirs::home_dir()
        && let Ok(rest) = p.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    p.display().to_string()
}
