//! UI foundation: fonts, theme -> egui style, design tokens, and shared widgets.

pub mod brand;
pub mod chrome;
pub mod docks;
pub mod icon_glyphs;
pub mod language_dialog;
pub mod palette;
pub mod settings_view;
pub mod sidebar;
pub mod welcome;
pub mod whats_new;
pub mod widgets;

use crate::theme::{Theme, mix, with_alpha};
use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Vec2};
use std::sync::Arc;

pub use icon_glyphs as ic;

pub const FAMILY_ICONS: &str = "icons";
pub const FAMILY_ICONS_FILLED: &str = "icons-filled";
pub const FAMILY_MEDIUM: &str = "medium";
pub const FAMILY_SEMIBOLD: &str = "semibold";

/// Island corner radius, gap between islands, and other layout constants.
pub const ISLAND_RADIUS: u8 = 10;
pub const GAP: f32 = 6.0;
pub const TITLE_BAR_H: f32 = 46.0;
pub const STATUS_BAR_H: f32 = 32.0;
pub const TAB_BAR_H: f32 = 40.0;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "inter", include_bytes!("../../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "inter-medium", include_bytes!("../../assets/fonts/Inter-Medium.ttf"));
    add(&mut fonts, "inter-semibold", include_bytes!("../../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "fira", include_bytes!("../../assets/fonts/FiraCode-Regular.ttf"));
    add(&mut fonts, "fira-medium", include_bytes!("../../assets/fonts/FiraCode-Medium.ttf"));
    add(&mut fonts, "fira-bold", include_bytes!("../../assets/fonts/FiraCode-Bold.ttf"));
    add(&mut fonts, "symbols", include_bytes!("../../assets/fonts/MaterialSymbolsOutlined.ttf"));
    add(&mut fonts, "symbols-filled", include_bytes!("../../assets/fonts/MaterialSymbolsFilled.ttf"));

    // Nerd Font icons (used by many shell prompts and `eza`) come from an installed Nerd Font.
    if let Some(bytes) = find_nerd_font() {
        fonts.font_data.insert("nerd".to_owned(), Arc::new(FontData::from_owned(bytes)));
    }
    let has_nerd = fonts.font_data.contains_key("nerd");
    let prop = fonts.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "inter".into());
    prop.insert(1, "fira".into());
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "fira".into());
    mono.insert(1, "inter".into());
    if has_nerd {
        mono.insert(2, "nerd".into());
    }
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].iter().skip(2).cloned().collect();
    let family = |first: &str| {
        let mut v = vec![first.to_string(), "fira".to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Name(FAMILY_MEDIUM.into()), family("inter-medium"));
    fonts.families.insert(FontFamily::Name(FAMILY_SEMIBOLD.into()), family("inter-semibold"));
    fonts.families.insert(FontFamily::Name("mono-medium".into()), vec!["fira-medium".into(), "inter".into()]);
    let mut bold = vec!["fira-bold".to_string(), "fira".to_string(), "inter".to_string()];
    if has_nerd {
        bold.push("nerd".into());
    }
    fonts.families.insert(FontFamily::Name(crate::terminal::BOLD_FONT_FAMILY.into()), bold);
    fonts.families.insert(FontFamily::Name(FAMILY_ICONS.into()), vec!["symbols".into()]);
    fonts.families.insert(FontFamily::Name(FAMILY_ICONS_FILLED.into()), vec!["symbols-filled".into()]);
    ctx.set_fonts(fonts);
}

/// The first installed "* Nerd Font Mono" (or any Nerd Font) regular face, if any.
fn find_nerd_font() -> Option<Vec<u8>> {
    let mut dirs = vec![std::path::PathBuf::from("/Library/Fonts")];
    if let Some(h) = dirs::home_dir() {
        dirs.insert(0, h.join("Library/Fonts"));
        dirs.push(h.join(".local/share/fonts"));
    }
    let mut best: Option<(u8, std::path::PathBuf)> = None;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !name.contains("nerd") || !(name.ends_with(".ttf") || name.ends_with(".otf")) || !name.contains("regular") {
                continue;
            }
            let rank = if name.contains("nerdfontmono") { 0 } else { 1 };
            if best.as_ref().is_none_or(|b| rank < b.0) {
                best = Some((rank, e.path()));
            }
        }
    }
    best.and_then(|(_, p)| std::fs::read(p).ok())
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_ICONS.into()))
}
pub fn icon_font_filled(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_ICONS_FILLED.into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_MEDIUM.into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_SEMIBOLD.into()))
}
pub fn ui_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Derived colors for the "islands" layout.
#[derive(Clone, Copy)]
pub struct Tokens {
    pub window: Color32,
    pub island: Color32,
    pub island_border: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub selected: Color32,
    pub subtle_border: Color32,
    pub popup: Color32,
    pub shadow: Color32,
    pub run: Color32,
}

impl Tokens {
    pub fn of(t: &Theme) -> Tokens {
        let island = t.editor_bg;
        let window = t.window_bg.unwrap_or(if t.dark { mix(t.background, Color32::BLACK, 0.38) } else { mix(t.background, Color32::BLACK, 0.07) });
        Tokens {
            window,
            island,
            island_border: mix(island, t.text, if t.dark { 0.055 } else { 0.09 }),
            hover: mix(island, t.text, if t.dark { 0.075 } else { 0.06 }),
            pressed: mix(island, t.text, 0.12),
            selected: mix(island, t.text, if t.dark { 0.11 } else { 0.09 }),
            subtle_border: mix(island, t.text, if t.dark { 0.1 } else { 0.14 }),
            popup: if t.dark { mix(island, t.text, 0.045) } else { Color32::WHITE },
            shadow: if t.dark { Color32::from_black_alpha(140) } else { Color32::from_black_alpha(45) },
            run: if t.dark { Color32::from_rgb(0x5f, 0xd0, 0x6a) } else { Color32::from_rgb(0x1f, 0x9d, 0x3a) },
        }
    }
}

/// Apply the theme to egui's global style (popups, menus, text edits, scroll bars...).
pub fn apply_style(ctx: &egui::Context, t: &Theme, ui_size: f32) {
    let tk = Tokens::of(t);
    ctx.all_styles_mut(|style| {
        let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.dark_mode = t.dark;
        v.override_text_color = Some(t.text);
        v.panel_fill = tk.window;
        v.window_fill = tk.popup;
        v.extreme_bg_color = mix(tk.island, Color32::BLACK, if t.dark { 0.18 } else { 0.03 });
        v.faint_bg_color = tk.hover;
        v.code_bg_color = tk.hover;
        v.hyperlink_color = t.text_accent;
        v.warn_fg_color = t.warning;
        v.error_fg_color = t.error;
        v.selection.bg_fill = with_alpha(t.text_accent, 90);
        v.selection.stroke = Stroke::new(1.0, t.text);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_stroke = Stroke::new(1.0, tk.subtle_border);
        v.window_shadow = Shadow { offset: [0, 10], blur: 32, spread: 0, color: tk.shadow };
        v.popup_shadow = Shadow { offset: [0, 8], blur: 24, spread: 0, color: tk.shadow };
        v.resize_corner_size = 10.0;
        v.text_cursor.stroke = Stroke::new(2.0, t.cursor);
        v.striped = false;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.indent_has_left_vline = false;
        v.collapsing_header_frame = false;
        let r = CornerRadius::same(6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = tk.island;
        w.noninteractive.weak_bg_fill = tk.island;
        w.noninteractive.bg_stroke = Stroke::new(1.0, tk.island_border);
        w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
        w.noninteractive.corner_radius = r;
        w.inactive.bg_fill = tk.hover;
        w.inactive.weak_bg_fill = Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0, t.text_muted);
        w.inactive.corner_radius = r;
        w.inactive.expansion = 0.0;
        w.hovered.bg_fill = tk.hover;
        w.hovered.weak_bg_fill = tk.hover;
        w.hovered.bg_stroke = Stroke::NONE;
        w.hovered.fg_stroke = Stroke::new(1.0, t.text);
        w.hovered.corner_radius = r;
        w.hovered.expansion = 0.0;
        w.active.bg_fill = tk.pressed;
        w.active.weak_bg_fill = tk.pressed;
        w.active.bg_stroke = Stroke::NONE;
        w.active.fg_stroke = Stroke::new(1.0, t.text);
        w.active.corner_radius = r;
        w.active.expansion = 0.0;
        w.open.bg_fill = tk.selected;
        w.open.weak_bg_fill = tk.selected;
        w.open.bg_stroke = Stroke::NONE;
        w.open.fg_stroke = Stroke::new(1.0, t.text);
        w.open.corner_radius = r;
        style.visuals = v;

        style.spacing.item_spacing = Vec2::new(6.0, 4.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.window_margin = Margin::same(10);
        style.spacing.interact_size = Vec2::new(24.0, 26.0);
        style.spacing.icon_width = 16.0;
        style.spacing.icon_spacing = 6.0;
        style.spacing.combo_height = 320.0;
        style.spacing.scroll = egui::style::ScrollStyle {
            ..egui::style::ScrollStyle::floating()
        };
        style.spacing.scroll = egui::style::ScrollStyle {
            content_margin: style.spacing.scroll.content_margin,
            fade: style.spacing.scroll.fade,
            kinetic: style.spacing.scroll.kinetic,
            floating: true,
            bar_width: 8.0,
            floating_width: 3.0,
            floating_allocated_width: 0.0,
            handle_min_length: 24.0,
            bar_inner_margin: 2.0,
            bar_outer_margin: 2.0,
            foreground_color: true,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 0.0,
            active_handle_opacity: 0.6,
            interact_handle_opacity: 1.0,
        };
        style.interaction.selectable_labels = false;
        style.interaction.tooltip_delay = 0.35;
        style.interaction.show_tooltips_only_when_still = false;
        style.text_styles = [
            (TextStyle::Small, ui_font(ui_size - 2.5)),
            (TextStyle::Body, ui_font(ui_size)),
            (TextStyle::Button, ui_font(ui_size)),
            (TextStyle::Heading, semibold(ui_size + 6.0)),
            (TextStyle::Monospace, mono(ui_size)),
        ]
        .into();
        style.animation_time = 0.12;
    });
}
pub mod file_icons;
