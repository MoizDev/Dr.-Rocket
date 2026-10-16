//! Dr. Rocket branding: the logo (assets/SVG/doctorocket logo.svg) and app name.

use egui::{Image, Vec2};

pub const APP_NAME: &str = "Dr. Rocket";

const LOGO_SVG: &[u8] = include_bytes!("../../assets/SVG/doctorocket logo.svg");
/// Width / height of the logo's view box.
const LOGO_ASPECT: f32 = 94.53 / 145.79;

/// The logo at the given height (rendered from the SVG at the display's pixel density).
pub fn logo(height: f32) -> Image<'static> {
    Image::from_bytes("bytes://brand/doctorocket-logo.svg", LOGO_SVG).fit_to_exact_size(Vec2::new(height * LOGO_ASPECT, height))
}

/// Window icon for the Dock when running outside the app bundle.
pub fn window_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon/icon_256.png")).ok()
}
