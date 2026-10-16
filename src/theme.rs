//! Themes. Dr. Rocket reads Zed's theme format (schema v0.2), so every Zed theme works
//! out of the box. Bundled themes live in `assets/themes`; user themes are discovered in
//! `~/.config/zed/themes`, installed Zed theme extensions, and Dr. Rocket's own config dir.

use egui::Color32;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Default theme name.
pub const DEFAULT_THEME: &str = "Moiz One Dark";

const BUNDLED: &[(&str, &str)] = &[
    ("moiz-one-dark.json", include_str!("../assets/themes/moiz-one-dark.json")),
    ("one-dark.json", include_str!("../assets/themes/one-dark.json")),
    ("one-light.json", include_str!("../assets/themes/one-light.json")),
    ("drracket.json", include_str!("../assets/themes/drracket.json")),
    ("catppuccin.json", include_str!("../assets/themes/catppuccin.json")),
    ("gruvbox.json", include_str!("../assets/themes/gruvbox.json")),
    ("tokyo-night.json", include_str!("../assets/themes/tokyo-night.json")),
    ("github-light.json", include_str!("../assets/themes/github-light.json")),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Highlight {
    pub color: Color32,
    pub italic: bool,
    pub bold: bool,
}

impl Highlight {
    const fn plain(color: Color32) -> Self {
        Self { color, italic: false, bold: false }
    }
}

/// Colors for each Racket token class.
#[derive(Clone, Debug)]
pub struct Syntax {
    pub keyword: Highlight,
    pub function: Highlight,
    pub variable: Highlight,
    pub string: Highlight,
    pub string_escape: Highlight,
    pub regex: Highlight,
    pub number: Highlight,
    pub constant: Highlight,
    pub symbol: Highlight,
    pub comment: Highlight,
    pub punctuation: Highlight,
    pub operator: Highlight,
    pub lang: Highlight,
    pub hash_keyword: Highlight,
    pub type_: Highlight,
}

#[derive(Clone, Debug)]
pub struct TerminalColors {
    pub background: Color32,
    pub foreground: Color32,
    pub bright_foreground: Color32,
    pub dim_foreground: Color32,
    /// 0..8 normal, 8..16 bright.
    pub ansi: [Color32; 16],
    pub dim: [Color32; 8],
}

/// Every color a Zed theme can define that the IDE uses. Some are kept for completeness of
/// the mapping even when no widget reads them yet.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub dark: bool,

    pub background: Color32,
    pub surface: Color32,
    pub elevated_surface: Color32,
    pub panel: Color32,
    pub title_bar: Color32,
    pub status_bar: Color32,
    pub tab_bar: Color32,
    pub tab_active: Color32,
    pub tab_inactive: Color32,
    pub toolbar: Color32,

    pub border: Color32,
    pub border_variant: Color32,
    pub border_focused: Color32,
    pub border_selected: Color32,

    pub element: Color32,
    pub element_hover: Color32,
    pub element_active: Color32,
    pub element_selected: Color32,
    pub ghost_hover: Color32,
    pub ghost_selected: Color32,
    pub drop_target: Color32,

    pub text: Color32,
    pub text_muted: Color32,
    pub text_placeholder: Color32,
    pub text_disabled: Color32,
    pub text_accent: Color32,
    pub icon: Color32,
    pub icon_muted: Color32,
    pub icon_accent: Color32,

    pub editor_bg: Color32,
    pub editor_fg: Color32,
    pub gutter_bg: Color32,
    pub active_line: Color32,
    pub line_number: Color32,
    pub active_line_number: Color32,
    pub wrap_guide: Color32,
    pub indent_guide: Color32,
    pub indent_guide_active: Color32,
    pub highlight_read: Color32,
    pub highlight_write: Color32,
    pub bracket_match: Color32,
    pub selection: Color32,
    pub cursor: Color32,
    pub search_match: Color32,
    pub search_active_match: Color32,
    pub scrollbar_thumb: Color32,
    pub scrollbar_thumb_hover: Color32,

    pub error: Color32,
    pub error_bg: Color32,
    pub error_border: Color32,
    pub warning: Color32,
    pub warning_bg: Color32,
    pub warning_border: Color32,
    pub info: Color32,
    pub info_bg: Color32,
    pub info_border: Color32,
    pub hint: Color32,
    pub success: Color32,
    pub success_bg: Color32,
    pub success_border: Color32,
    pub created: Color32,
    pub modified: Color32,
    pub deleted: Color32,
    pub ignored: Color32,

    pub syntax: Syntax,
    pub terminal: TerminalColors,
    /// Rainbow bracket colors by nesting depth.
    pub rainbow: Vec<Color32>,
    /// Collaborator/player palette, used for accents (stepper highlights, arrows...).
    pub accents: Vec<Color32>,
}

pub fn parse_color(s: &str) -> Option<Color32> {
    let s = s.trim().strip_prefix('#')?;
    let hex = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    match s.len() {
        6 => Some(Color32::from_rgb(hex(0)?, hex(2)?, hex(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(hex(0)?, hex(2)?, hex(4)?, hex(6)?)),
        3 => {
            let h = |i: usize| u8::from_str_radix(&s[i..i + 1], 16).ok().map(|v| v * 17);
            Some(Color32::from_rgb(h(0)?, h(1)?, h(2)?))
        }
        _ => None,
    }
}

// Note: `Color32` stores premultiplied alpha, so these helpers work on the unmultiplied
// sRGBA values to blend correctly.

/// Composite `top` (possibly translucent) over an opaque `bottom`.
pub fn over(bottom: Color32, top: Color32) -> Color32 {
    let [br, bg, bb, _] = bottom.to_srgba_unmultiplied();
    let [tr, tg, tb, ta] = top.to_srgba_unmultiplied();
    let a = ta as f32 / 255.0;
    let mix = |b: u8, t: u8| (b as f32 * (1.0 - a) + t as f32 * a).round() as u8;
    Color32::from_rgb(mix(br, tr), mix(bg, tg), mix(bb, tb))
}

/// Linear mix between two colors (in unmultiplied sRGBA).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2]), m(a[3], b[3]))
}

/// The same color with a different (straight) alpha.
pub fn with_alpha(c: Color32, alpha: u8) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

struct Style<'a> {
    map: &'a serde_json::Map<String, Value>,
}

impl Style<'_> {
    fn get(&self, key: &str) -> Option<Color32> {
        self.map.get(key).and_then(Value::as_str).and_then(parse_color)
    }
    /// First present key wins.
    fn any(&self, keys: &[&str]) -> Option<Color32> {
        keys.iter().find_map(|k| self.get(k))
    }
    fn syntax(&self, keys: &[&str]) -> Option<Highlight> {
        let syn = self.map.get("syntax")?.as_object()?;
        keys.iter().find_map(|k| {
            let entry = syn.get(*k)?.as_object()?;
            let color = entry.get("color").and_then(Value::as_str).and_then(parse_color)?;
            let italic = entry.get("font_style").and_then(Value::as_str) == Some("italic");
            let bold = entry.get("font_weight").and_then(Value::as_u64).is_some_and(|w| w >= 600);
            Some(Highlight { color, italic, bold })
        })
    }
}

impl Theme {
    /// Build a theme from one entry of a Zed theme family's `themes` array.
    pub fn from_zed(entry: &Value) -> Option<Theme> {
        let name = entry.get("name")?.as_str()?.to_string();
        let dark = entry.get("appearance").and_then(Value::as_str) != Some("light");
        let st = Style { map: entry.get("style")?.as_object()? };

        // Base fallbacks, chosen per appearance so partial themes still look coherent.
        let (d_bg, d_fg) = if dark {
            (Color32::from_rgb(0x1e, 0x21, 0x27), Color32::from_rgb(0xab, 0xb2, 0xbf))
        } else {
            (Color32::from_rgb(0xfa, 0xfa, 0xfa), Color32::from_rgb(0x38, 0x3a, 0x42))
        };
        let background = opaque(st.get("background").unwrap_or(d_bg), d_bg);
        let editor_bg = opaque(st.any(&["editor.background", "background"]).unwrap_or(background), background);
        let text = st.any(&["text", "editor.foreground"]).unwrap_or(d_fg);
        let text_muted = st.get("text.muted").unwrap_or(mix(text, background, 0.35));
        let text_placeholder = st.get("text.placeholder").unwrap_or(mix(text, background, 0.55));
        let accent = st.any(&["text.accent", "icon.accent", "border.focused"]).unwrap_or(Color32::from_rgb(0x61, 0xaf, 0xef));
        let border = st.get("border").unwrap_or(mix(background, text, 0.12));
        let surface = opaque(st.get("surface.background").unwrap_or(background), background);
        let panel = opaque(st.get("panel.background").unwrap_or(surface), background);
        let element = st.get("element.background").unwrap_or(mix(background, text, 0.12));
        let element_hover = st.any(&["element.hover", "ghost_element.hover"]).unwrap_or(mix(background, text, 0.08));
        let element_selected = st.any(&["element.selected", "ghost_element.selected"]).unwrap_or(mix(background, text, 0.14));
        let editor_fg = st.any(&["editor.foreground", "text"]).unwrap_or(text);

        let players: Vec<(Color32, Color32)> = entry["style"]
            .get("players")
            .and_then(Value::as_array)
            .map(|ps| {
                ps.iter()
                    .filter_map(|p| {
                        let c = p.get("cursor").and_then(Value::as_str).and_then(parse_color)?;
                        let s = p.get("selection").and_then(Value::as_str).and_then(parse_color).unwrap_or(with_alpha(c, 60));
                        Some((c, s))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let cursor = players.first().map(|p| p.0).unwrap_or(accent);
        let selection = st
            .get("element.selection_background")
            .or(players.first().map(|p| p.1))
            .unwrap_or(with_alpha(accent, 70));

        let error = st.get("error").unwrap_or(Color32::from_rgb(0xe0, 0x55, 0x61));
        let warning = st.get("warning").unwrap_or(Color32::from_rgb(0xd1, 0x9a, 0x66));
        let info = st.get("info").unwrap_or(accent);
        let success = st.any(&["success", "created"]).unwrap_or(Color32::from_rgb(0x98, 0xc3, 0x79));
        let tint = |c: Color32| over(editor_bg, with_alpha(c, 32));
        let edge = |c: Color32| over(editor_bg, with_alpha(c, 110));

        let s = |keys: &[&str], fallback: Highlight| st.syntax(keys).unwrap_or(fallback);
        let fallback_kw = Highlight::plain(if dark { Color32::from_rgb(0xc6, 0x78, 0xdd) } else { Color32::from_rgb(0xa6, 0x26, 0xa4) });
        let fallback_fn = Highlight::plain(if dark { Color32::from_rgb(0x61, 0xaf, 0xef) } else { Color32::from_rgb(0x40, 0x78, 0xf2) });
        let fallback_str = Highlight::plain(if dark { Color32::from_rgb(0x98, 0xc3, 0x79) } else { Color32::from_rgb(0x50, 0xa1, 0x4f) });
        let fallback_num = Highlight::plain(if dark { Color32::from_rgb(0xd1, 0x9a, 0x66) } else { Color32::from_rgb(0x98, 0x68, 0x01) });
        let fallback_var = Highlight::plain(editor_fg);
        let comment_default = Highlight { color: mix(editor_fg, editor_bg, 0.45), italic: true, bold: false };
        let constant = s(&["constant", "boolean", "number"], fallback_num);
        let syntax = Syntax {
            keyword: s(&["keyword"], fallback_kw),
            function: s(&["function", "function.method"], fallback_fn),
            variable: s(&["variable"], fallback_var),
            string: s(&["string"], fallback_str),
            string_escape: s(&["string.escape", "string.special"], fallback_num),
            regex: s(&["string.regex", "string.special"], fallback_str),
            number: s(&["number", "constant"], fallback_num),
            constant: s(&["constant.builtin", "boolean", "constant"], constant),
            symbol: s(&["string.special.symbol", "constant", "string.special"], constant),
            comment: s(&["comment"], comment_default),
            punctuation: s(&["punctuation.bracket", "punctuation"], Highlight::plain(text_muted)),
            operator: s(&["operator", "punctuation.special"], Highlight::plain(editor_fg)),
            lang: s(&["variable.special", "type"], fallback_num),
            hash_keyword: s(&["constant", "attribute"], constant),
            type_: s(&["type", "constructor"], fallback_num),
        };

        let t = |k: &str, d: Color32| st.get(k).unwrap_or(d);
        let terminal_bg = opaque(st.get("terminal.background").unwrap_or(panel), panel);
        let terminal = TerminalColors {
            background: terminal_bg,
            foreground: t("terminal.foreground", editor_fg),
            bright_foreground: t("terminal.bright_foreground", text),
            dim_foreground: t("terminal.dim_foreground", text_muted),
            ansi: [
                t("terminal.ansi.black", Color32::from_rgb(0x3f, 0x44, 0x51)),
                t("terminal.ansi.red", Color32::from_rgb(0xe0, 0x55, 0x61)),
                t("terminal.ansi.green", Color32::from_rgb(0x8c, 0xc2, 0x65)),
                t("terminal.ansi.yellow", Color32::from_rgb(0xd1, 0x8f, 0x52)),
                t("terminal.ansi.blue", Color32::from_rgb(0x4a, 0xa5, 0xf0)),
                t("terminal.ansi.magenta", Color32::from_rgb(0xc1, 0x62, 0xde)),
                t("terminal.ansi.cyan", Color32::from_rgb(0x42, 0xb3, 0xc2)),
                t("terminal.ansi.white", Color32::from_rgb(0xd7, 0xda, 0xe0)),
                t("terminal.ansi.bright_black", Color32::from_rgb(0x4f, 0x56, 0x66)),
                t("terminal.ansi.bright_red", Color32::from_rgb(0xff, 0x61, 0x6e)),
                t("terminal.ansi.bright_green", Color32::from_rgb(0xa5, 0xe0, 0x75)),
                t("terminal.ansi.bright_yellow", Color32::from_rgb(0xf0, 0xa4, 0x5d)),
                t("terminal.ansi.bright_blue", Color32::from_rgb(0x4d, 0xc4, 0xff)),
                t("terminal.ansi.bright_magenta", Color32::from_rgb(0xde, 0x73, 0xff)),
                t("terminal.ansi.bright_cyan", Color32::from_rgb(0x4c, 0xd1, 0xe0)),
                t("terminal.ansi.bright_white", Color32::from_rgb(0xff, 0xff, 0xff)),
            ],
            dim: [Color32::PLACEHOLDER; 8],
        };
        let mut terminal = terminal;
        let dim_keys = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
        for (i, k) in dim_keys.iter().enumerate() {
            terminal.dim[i] = st.get(&format!("terminal.ansi.dim_{k}")).unwrap_or(mix(terminal.ansi[i], terminal_bg, 0.4));
        }

        let accents: Vec<Color32> = entry["style"]
            .get("accents")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().and_then(parse_color)).collect::<Vec<_>>())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| players.iter().map(|p| p.0).collect());
        let rainbow = if dark {
            vec![
                Color32::from_rgb(0xe5, 0xc0, 0x7b),
                Color32::from_rgb(0xd5, 0x5f, 0xde),
                Color32::from_rgb(0x52, 0xad, 0xf2),
            ]
        } else {
            vec![
                Color32::from_rgb(0xb0, 0x7d, 0x00),
                Color32::from_rgb(0xa6, 0x26, 0xa4),
                Color32::from_rgb(0x40, 0x78, 0xf2),
            ]
        };

        Some(Theme {
            name,
            dark,
            background,
            surface,
            elevated_surface: opaque(st.get("elevated_surface.background").unwrap_or(surface), background),
            panel,
            title_bar: opaque(st.get("title_bar.background").unwrap_or(background), background),
            status_bar: opaque(st.get("status_bar.background").unwrap_or(background), background),
            tab_bar: opaque(st.get("tab_bar.background").unwrap_or(background), background),
            tab_active: opaque(st.get("tab.active_background").unwrap_or(editor_bg), editor_bg),
            tab_inactive: opaque(st.get("tab.inactive_background").unwrap_or(background), background),
            toolbar: opaque(st.get("toolbar.background").unwrap_or(editor_bg), editor_bg),
            border,
            border_variant: st.get("border.variant").unwrap_or(border),
            border_focused: st.get("border.focused").unwrap_or(accent),
            border_selected: st.get("border.selected").unwrap_or(element_selected),
            element,
            element_hover,
            element_active: st.get("element.active").unwrap_or(element_selected),
            element_selected,
            ghost_hover: st.get("ghost_element.hover").unwrap_or(element_hover),
            ghost_selected: st.get("ghost_element.selected").unwrap_or(element_selected),
            drop_target: st.get("drop_target.background").unwrap_or(with_alpha(accent, 50)),
            text,
            text_muted,
            text_placeholder,
            text_disabled: st.get("text.disabled").unwrap_or(text_placeholder),
            text_accent: accent,
            icon: st.get("icon").unwrap_or(text_muted),
            icon_muted: st.get("icon.muted").unwrap_or(text_placeholder),
            icon_accent: st.get("icon.accent").unwrap_or(accent),
            editor_bg,
            editor_fg,
            gutter_bg: opaque(st.get("editor.gutter.background").unwrap_or(editor_bg), editor_bg),
            active_line: st.any(&["editor.active_line.background", "editor.highlighted_line.background"]).unwrap_or(mix(editor_bg, text, 0.05)),
            line_number: st.get("editor.line_number").unwrap_or(text_placeholder),
            active_line_number: st.get("editor.active_line_number").unwrap_or(text),
            wrap_guide: st.get("editor.wrap_guide").unwrap_or(with_alpha(text, 30)),
            indent_guide: st.get("editor.indent_guide").unwrap_or(mix(editor_bg, text, 0.12)),
            indent_guide_active: st.get("editor.indent_guide_active").unwrap_or(mix(editor_bg, text, 0.3)),
            highlight_read: st.get("editor.document_highlight.read_background").unwrap_or(with_alpha(accent, 40)),
            highlight_write: st.get("editor.document_highlight.write_background").unwrap_or(with_alpha(accent, 60)),
            bracket_match: st.get("editor.document_highlight.bracket_background").unwrap_or(with_alpha(accent, 60)),
            selection,
            cursor,
            search_match: st.get("search.match_background").unwrap_or(with_alpha(accent, 60)),
            search_active_match: st.get("search.active_match_background").unwrap_or(with_alpha(accent, 110)),
            scrollbar_thumb: st.get("scrollbar.thumb.background").unwrap_or(with_alpha(text, 40)),
            scrollbar_thumb_hover: st.get("scrollbar.thumb.hover_background").unwrap_or(with_alpha(text, 70)),
            error,
            error_bg: st.get("error.background").map(|c| over(editor_bg, c)).unwrap_or(tint(error)),
            error_border: st.get("error.border").unwrap_or(edge(error)),
            warning,
            warning_bg: st.get("warning.background").map(|c| over(editor_bg, c)).unwrap_or(tint(warning)),
            warning_border: st.get("warning.border").unwrap_or(edge(warning)),
            info,
            info_bg: st.get("info.background").map(|c| over(editor_bg, c)).unwrap_or(tint(info)),
            info_border: st.get("info.border").unwrap_or(edge(info)),
            hint: st.get("hint").unwrap_or(text_muted),
            success,
            success_bg: st.get("success.background").map(|c| over(editor_bg, c)).unwrap_or(tint(success)),
            success_border: st.get("success.border").unwrap_or(edge(success)),
            created: st.get("created").unwrap_or(success),
            modified: st.get("modified").unwrap_or(warning),
            deleted: st.get("deleted").unwrap_or(error),
            ignored: st.get("ignored").unwrap_or(text_placeholder),
            syntax,
            terminal,
            rainbow,
            accents,
        })
    }

    /// Color for a bracket at nesting `depth`.
    pub fn rainbow(&self, depth: usize) -> Color32 {
        self.rainbow[depth % self.rainbow.len()]
    }
}

/// Zed themes may use translucent backgrounds for blurred windows; we composite them onto
/// an opaque fallback so the UI stays readable on a normal window.
fn opaque(c: Color32, base: Color32) -> Color32 {
    if c.a() == 255 { c } else { over(if base.a() == 255 { base } else { Color32::BLACK }, c) }
}

#[derive(Clone, Debug)]
pub struct ThemeEntry {
    pub theme: Theme,
    /// The theme's `style` JSON, used to skip identical copies imported from Zed.
    style_json: String,
    /// Where it came from, for the theme picker.
    pub source: ThemeSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeSource {
    Bundled,
    Zed,
    User,
}

impl ThemeSource {
    pub fn label(&self) -> &'static str {
        match self {
            ThemeSource::Bundled => "Built-in",
            ThemeSource::Zed => "From Zed",
            ThemeSource::User => "User",
        }
    }
}

#[derive(Default)]
pub struct ThemeRegistry {
    pub themes: Vec<ThemeEntry>,
}

impl ThemeRegistry {
    pub fn load() -> Self {
        let mut reg = ThemeRegistry::default();
        for (_, json) in BUNDLED {
            reg.add_family(json, ThemeSource::Bundled);
        }
        for dir in zed_theme_dirs() {
            reg.add_dir(&dir, ThemeSource::Zed);
        }
        reg.add_dir(&crate::config::config_dir().join("themes"), ThemeSource::User);
        reg
    }

    fn add_dir(&mut self, dir: &Path, source: ThemeSource) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        paths.sort();
        for p in paths {
            if let Ok(s) = std::fs::read_to_string(&p) {
                self.add_family(&s, source.clone());
            }
        }
    }

    fn add_family(&mut self, json: &str, source: ThemeSource) {
        let Ok(v) = serde_json::from_str::<Value>(&strip_json_comments(json)) else { return };
        let Some(list) = v.get("themes").and_then(Value::as_array) else { return };
        for t in list {
            if let Some(theme) = Theme::from_zed(t) {
                let style_json = t.get("style").map(|s| s.to_string()).unwrap_or_default();
                // First registration of a name wins (bundled before Zed copies), and a Zed
                // theme identical to a bundled one (e.g. the source of Moiz One Dark) is skipped.
                if !self.themes.iter().any(|e| e.theme.name == theme.name || e.style_json == style_json) {
                    self.themes.push(ThemeEntry { theme, style_json, source: source.clone() });
                }
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&Theme> {
        self.themes.iter().find(|e| e.theme.name == name).map(|e| &e.theme)
    }

    pub fn get_or_default(&self, name: &str) -> Theme {
        self.get(name)
            .or_else(|| self.get(DEFAULT_THEME))
            .cloned()
            .expect("bundled default theme must parse")
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|e| e.theme.name.clone()).collect()
    }
}

fn zed_theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".config/zed/themes"));
        let ext = home.join("Library/Application Support/Zed/extensions/installed");
        if let Ok(rd) = std::fs::read_dir(&ext) {
            let mut exts: Vec<PathBuf> = rd.flatten().map(|e| e.path().join("themes")).filter(|p| p.is_dir()).collect();
            exts.sort();
            dirs.extend(exts);
        }
    }
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("zed/extensions/installed"));
    }
    dirs
}

/// Zed theme files are JSON but occasionally carry `//` comments or trailing commas.
pub fn strip_json_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    // Remove trailing commas before } or ].
    let re = regex::Regex::new(r",(\s*[}\]])").unwrap();
    re.replace_all(&out, "$1").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_parses_with_exact_colors() {
        let reg = ThemeRegistry { themes: vec![] };
        let mut reg = reg;
        reg.add_family(BUNDLED[0].1, ThemeSource::Bundled);
        let t = reg.get(DEFAULT_THEME).unwrap();
        assert!(t.dark);
        assert_eq!(t.editor_bg, Color32::from_rgb(0x18, 0x1a, 0x1f));
        assert_eq!(t.syntax.keyword.color, Color32::from_rgb(0xd5, 0x5f, 0xde));
        assert!(t.syntax.comment.italic);
        assert_eq!(t.terminal.ansi[1], Color32::from_rgb(0xe0, 0x55, 0x61));
    }

    /// Moiz One Dark is the user's Zed "One Dark Darker" theme under a new name: every style
    /// value must match the original (checked when the Zed theme is installed).
    #[test]
    fn default_theme_is_one_dark_darker() {
        let Some(path) = dirs::home_dir().map(|h| h.join(".config/zed/themes/one-dark-darker.json")) else { return };
        let Ok(original) = std::fs::read_to_string(&path) else { return };
        let parse = |s: &str| serde_json::from_str::<Value>(&strip_json_comments(s)).unwrap();
        let (ours, theirs) = (parse(BUNDLED[0].1), parse(&original));
        let (ours, theirs) = (ours["themes"].as_array().unwrap(), theirs["themes"].as_array().unwrap());
        assert_eq!(ours.len(), theirs.len());
        for (a, b) in ours.iter().zip(theirs) {
            assert_eq!(a["appearance"], b["appearance"]);
            assert_eq!(a["style"], b["style"], "Moiz One Dark differs from One Dark Darker");
        }
    }

    #[test]
    fn all_bundled_themes_parse() {
        for (name, json) in BUNDLED {
            let mut reg = ThemeRegistry::default();
            reg.add_family(json, ThemeSource::Bundled);
            assert!(!reg.themes.is_empty(), "{name} did not parse");
        }
    }

    #[test]
    fn blending_respects_premultiplied_alpha() {
        let bg = Color32::from_rgb(0, 0, 0);
        let half_white = Color32::from_rgba_unmultiplied(255, 255, 255, 128);
        let o = over(bg, half_white);
        assert!((o.r() as i32 - 128).abs() <= 1, "{o:?}");
        assert_eq!(with_alpha(half_white, 255), Color32::WHITE);
        let m = mix(Color32::BLACK, Color32::WHITE, 0.5);
        assert!((m.r() as i32 - 128).abs() <= 1);
    }

    #[test]
    fn strips_comments_and_trailing_commas() {
        let s = strip_json_comments("{\"a\": \"//x\", // c\n \"b\": [1,2,],}");
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["a"], "//x");
    }
}
