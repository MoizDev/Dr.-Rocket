//! Material Icon Theme lookup (the same theme and mapping Zed uses).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/file_icons.rs"));

#[derive(Deserialize)]
struct IconPath {
    path: String,
}

#[derive(Deserialize)]
struct DirIcons {
    collapsed: String,
    expanded: String,
}

#[derive(Deserialize)]
struct ThemeJson {
    file_icons: HashMap<String, IconPath>,
    directory_icons: DirIcons,
    named_directory_icons: HashMap<String, DirIcons>,
    file_suffixes: HashMap<String, String>,
    file_stems: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Family {
    themes: Vec<ThemeJson>,
}

struct IconTheme {
    theme: ThemeJson,
}

fn theme() -> &'static IconTheme {
    static T: OnceLock<IconTheme> = OnceLock::new();
    T.get_or_init(|| {
        let fam: Family = serde_json::from_str(include_str!("../../assets/icons/material-icon-theme.json")).expect("icon theme json");
        IconTheme { theme: fam.themes.into_iter().next().expect("one icon theme") }
    })
}

fn svg_by_path(path: &str) -> Option<(&'static str, &'static [u8])> {
    let name = path.rsplit('/').next()?;
    let i = FILE_ICON_SVGS.binary_search_by(|(n, _)| (*n).cmp(name)).ok()?;
    Some(FILE_ICON_SVGS[i])
}

fn key_to_svg(key: &str) -> Option<(&'static str, &'static [u8])> {
    theme().theme.file_icons.get(key).and_then(|p| svg_by_path(&p.path))
}

/// Icon for a file name: (stable uri, svg bytes).
pub fn file_icon(file_name: &str) -> (&'static str, &'static [u8]) {
    let t = &theme().theme;
    let lower = file_name.to_ascii_lowercase();
    let key = t
        .file_stems
        .get(file_name)
        .or_else(|| t.file_stems.get(&lower))
        .cloned()
        .or_else(|| {
            // Longest suffix first: "foo.test.rkt" tries "test.rkt" then "rkt".
            let mut rest = lower.as_str();
            while let Some(i) = rest.find('.') {
                rest = &rest[i + 1..];
                if let Some(k) = t.file_suffixes.get(rest) {
                    return Some(k.clone());
                }
            }
            None
        })
        .or_else(|| t.file_stems.get(lower.split('.').next().unwrap_or("")).cloned())
        .or_else(|| {
            // Other Racket file types (Scribble documents, data and include files) share the
            // Racket icon; the icon theme only knows `.rkt`.
            let ext = lower.rsplit('.').next().unwrap_or("");
            matches!(ext, "scrbl" | "rktl" | "rktd" | "ss" | "scm" | "sls").then(|| t.file_suffixes.get("rkt").cloned()).flatten()
        })
        .unwrap_or_else(|| "default".to_string());
    key_to_svg(&key).or_else(|| key_to_svg("default")).or_else(|| svg_by_path("file.svg")).unwrap_or(FILE_ICON_SVGS[0])
}

/// Icon for a folder, open or closed.
pub fn folder_icon(dir_name: &str, open: bool) -> (&'static str, &'static [u8]) {
    let t = &theme().theme;
    let d = t.named_directory_icons.get(dir_name).or_else(|| t.named_directory_icons.get(&dir_name.to_ascii_lowercase())).unwrap_or(&t.directory_icons);
    let path = if open { &d.expanded } else { &d.collapsed };
    svg_by_path(path).unwrap_or(FILE_ICON_SVGS[0])
}

/// An egui image for an icon.
pub fn image(icon: (&'static str, &'static [u8]), size: f32) -> egui::Image<'static> {
    egui::Image::from_bytes(format!("bytes://material/{}", icon.0), icon.1).fit_to_exact_size(egui::Vec2::splat(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn racket_files_get_the_racket_icon() {
        assert_eq!(file_icon("main.rkt").0, "racket.svg");
        assert_eq!(file_icon("README.md").0, "readme.svg");
        assert_eq!(folder_icon("src", false).0, "folder-src.svg");
        assert_eq!(folder_icon("whatever", true).0, "folder-open.svg");
        assert!(file_icon("noext").0.ends_with(".svg"));
    }
}
