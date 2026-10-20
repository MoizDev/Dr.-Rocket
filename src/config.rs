//! User settings, recent projects, and on-disk locations.
//!
//! Settings live in `<config_dir>/settings.json`. On first launch the defaults are seeded
//! from Zed's `settings.json` (fonts, cursor, rainbow brackets...) so the editor feels the
//! same as the user's Zed setup.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("DRROCKET_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dr-rocket")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("dr-rocket")
}

/// Ready-made bracket palettes offered in Settings (name, colors).
pub const RAINBOW_PRESETS: &[(&str, &[&str])] = &[
    ("Classic", &["#FFD700", "#DA70D6", "#179FFF"]),
    ("Pastel", &["#F2C14E", "#C792EA", "#82AAFF", "#89DDFF", "#C3E88D"]),
    ("Neon", &["#FF6AC1", "#00F5D4", "#FEE440", "#9B5DE5", "#00BBF9"]),
    ("Sunset", &["#FF8A65", "#FFB74D", "#F06292", "#BA68C8", "#7986CB"]),
    ("Forest", &["#A3BE8C", "#EBCB8B", "#88C0D0", "#B48EAD", "#D08770"]),
    ("Mono", &["#9DA5B4", "#7F848E", "#5C6370"]),
];

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    /// Translucent rainbow bands over each level of indentation (indent-rainbow style).
    Highlights,
    /// A thin guide line per open list.
    Lines,
    Off,
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
    /// Animate the caret gliding to its new position.
    pub smooth_caret: bool,
    /// Show test results in the editor: a gutter tick per passing test, and failures inline.
    pub test_results_in_editor: bool,
    /// Vim (Neovim) keybindings in the editor, with a LazyVim-style Space leader.
    pub vim_mode: bool,
    /// In Vim mode, number lines by their distance from the cursor (the cursor's line keeps its
    /// own number), for counts like `5j`.
    pub relative_line_numbers: bool,
    /// Width of the bar caret (and height of the underline caret) in points.
    pub cursor_width: f32,
    /// Custom caret color (`#rrggbb`); the theme's cursor color when unset.
    pub cursor_color: Option<String>,
    pub rainbow_brackets: bool,
    /// Custom rainbow palette for brackets and indent guides (`#rrggbb` each); the
    /// theme's palette when empty.
    pub rainbow_colors: Vec<String>,
    /// Highlight the bracket pair at the cursor in its own depth color.
    pub bracket_pair_highlight: bool,
    /// How indentation is visualized: rainbow highlight bands, guide lines, or nothing.
    pub indent_style: IndentStyle,
    /// With `IndentStyle::Lines`: color the lines by nesting depth.
    pub rainbow_indent_guides: bool,
    pub show_line_numbers: bool,
    pub highlight_active_line: bool,
    pub wrap_guide_column: Option<usize>,
    pub autosave: bool,
    pub autosave_delay_ms: u64,
    pub show_tab_bar: bool,
    pub auto_close_brackets: bool,
    pub online_check_syntax: bool,
    pub show_binding_arrows: bool,
    /// Pop up completions while typing (Ctrl-Space always opens them).
    pub auto_complete: bool,
    /// Count the stepper's closing "All of the definitions have been successfully evaluated"
    /// screen as a step, as DrRacket does ("9/9" rather than "8 of 8" plus a note).
    pub stepper_count_finish: bool,
    /// Execute and show check-expect and the other test-engine checks while stepping.
    pub stepper_include_tests: bool,
    /// Show the active reduction without accumulated outer recursive expressions.
    pub stepper_functional_focus: bool,
    /// Show only the highlighted result of each reduction.
    pub stepper_after_only: bool,
    /// Step selected expressions using the program definitions as context.
    pub stepper_selection_only: bool,
    pub racket_path: Option<String>,
    /// Pause a running program that uses more memory than this and ask whether to keep going
    /// (DrRacket's memory limit); `None` for no limit.
    pub memory_limit_mb: Option<u64>,
    pub shell: Option<String>,
    pub restore_session: bool,
    /// Look for new releases in the background and install them on quit (update.rs).
    pub check_for_updates: bool,
    /// Language for new files: the one last chosen in the Language dialog (`module` for
    /// `#lang racket`, or a teaching language id).
    pub default_language: String,
    /// The teaching-language options chosen with it.
    pub default_language_settings: Option<crate::racket::lang::TeachingSettings>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: crate::theme::DEFAULT_THEME.to_string(),
            buffer_font_size: 16.0,
            ui_font_size: 14.0,
            terminal_font_size: 15.0,
            line_height: 1.6,
            cursor_shape: CursorShape::Bar,
            cursor_blink: true,
            smooth_caret: true,
            test_results_in_editor: true,
            vim_mode: false,
            relative_line_numbers: true,
            cursor_width: 2.0,
            cursor_color: None,
            rainbow_brackets: true,
            rainbow_colors: Vec::new(),
            bracket_pair_highlight: true,
            indent_style: IndentStyle::Highlights,
            rainbow_indent_guides: true,
            show_line_numbers: true,
            highlight_active_line: true,
            wrap_guide_column: Some(102),
            autosave: false,
            autosave_delay_ms: 1000,
            show_tab_bar: true,
            auto_close_brackets: true,
            online_check_syntax: true,
            show_binding_arrows: false,
            auto_complete: false,
            stepper_count_finish: true,
            stepper_include_tests: true,
            stepper_functional_focus: false,
            stepper_after_only: false,
            stepper_selection_only: false,
            racket_path: None,
            memory_limit_mb: Some(256),
            shell: None,
            restore_session: true,
            check_for_updates: true,
            default_language: "module".to_string(),
            default_language_settings: None,
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
                Err(e) => eprintln!("drrocket: ignoring invalid {}: {e}", path.display()),
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
        if let Some(ms) = v.pointer("/autosave/after_delay/milliseconds").and_then(|x| x.as_u64()) {
            s.autosave = true;
            s.autosave_delay_ms = ms.max(100);
        }
        if let Some(b) = v.get("cursor_blink").and_then(|x| x.as_bool()) {
            s.cursor_blink = b;
        }
        if let Some(b) = v.get("colorize_brackets").and_then(|x| x.as_bool()) {
            s.rainbow_brackets = b;
        }
        if v.pointer("/indent_guides/enabled").and_then(|x| x.as_bool()) == Some(false) {
            s.indent_style = IndentStyle::Off;
        }
        Some(s)
    }
}

/// Write a file atomically (temp file + rename) so a crash never leaves a truncated file.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.drrocket-tmp"));
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

#[cfg(test)]
mod stepper_settings_tests {
    use super::Settings;

    #[test]
    fn stepper_options_default_and_round_trip() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(settings.stepper_include_tests);
        assert!(!settings.stepper_functional_focus);
        assert!(!settings.stepper_after_only);
        assert!(!settings.stepper_selection_only);
        settings.stepper_include_tests = false;
        settings.stepper_functional_focus = true;
        settings.stepper_after_only = true;
        settings.stepper_selection_only = true;
        let restored: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(restored, settings);
    }
}
