//! Renders the Dr. Rocket app icon from the brand logo (`assets/SVG/doctorocket logo.svg`):
//! the logo centered on a dark rounded-square tile, at every macOS icon size.
//!
//! `cargo run --example make_icon` writes `assets/icon/icon_<size>.png` and
//! `assets/icon/logo_<size>.png` (the bare logo, transparent background).

use resvg::tiny_skia::{self, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader, SpreadMode, Transform};
use resvg::usvg;

const LOGO: &[u8] = include_bytes!("../assets/SVG/doctorocket logo.svg");

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    // Superellipse-ish squircle: cubic corners with a softer handle than a circle.
    let k = r * 0.62;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().expect("valid path")
}

fn draw_logo(pixmap: &mut Pixmap, tree: &usvg::Tree, cx: f32, cy: f32, height: f32) {
    let size = tree.size();
    let scale = height / size.height();
    let w = size.width() * scale;
    let t = Transform::from_scale(scale, scale).post_translate(cx - w / 2.0, cy - height / 2.0);
    resvg::render(tree, t, &mut pixmap.as_mut());
}

fn main() {
    let tree = usvg::Tree::from_data(LOGO, &usvg::Options::default()).expect("logo parses");
    let out = std::path::Path::new("assets/icon");
    std::fs::create_dir_all(out).unwrap();
    for size in [16u32, 32, 64, 128, 256, 512, 1024] {
        let s = size as f32;
        // App icon: dark tile following the macOS grid (shape ~80% of the canvas).
        let mut pm = Pixmap::new(size, size).unwrap();
        let inset = s * 0.1;
        let w = s - 2.0 * inset;
        let radius = w * 0.225;
        // Soft shadow.
        for k in 0..10 {
            let grow = (10 - k) as f32 * s / 512.0 * 1.6;
            let mut p = Paint { anti_alias: true, ..Default::default() };
            p.set_color_rgba8(0, 0, 0, 5);
            let path = rounded_rect(inset - grow, inset + s * 0.012 - grow, w + 2.0 * grow, w + 2.0 * grow, radius + grow);
            pm.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        }
        let tile = rounded_rect(inset, inset, w, w, radius);
        let paint = Paint {
            anti_alias: true,
            shader: LinearGradient::new(
            Point::from_xy(inset, inset),
            Point::from_xy(inset, inset + w),
            vec![GradientStop::new(0.0, tiny_skia::Color::from_rgba8(0x2a, 0x2d, 0x36, 255)), GradientStop::new(1.0, tiny_skia::Color::from_rgba8(0x14, 0x15, 0x1a, 255))],
            SpreadMode::Pad,
            Transform::identity(),
        )
            .unwrap_or(Shader::SolidColor(tiny_skia::Color::from_rgba8(0x18, 0x1a, 0x1f, 255))),
            ..Default::default()
        };
        pm.fill_path(&tile, &paint, FillRule::Winding, Transform::identity(), None);
        // Hairline highlight on the top edge.
        let mut edge = Paint { anti_alias: true, ..Default::default() };
        edge.set_color_rgba8(255, 255, 255, 22);
        let stroke = tiny_skia::Stroke { width: (s / 512.0).max(0.5), ..Default::default() };
        pm.stroke_path(&tile, &edge, &stroke, Transform::identity(), None);
        draw_logo(&mut pm, &tree, s / 2.0, s / 2.0 + w * 0.015, w * 0.66);
        pm.save_png(out.join(format!("icon_{size}.png"))).unwrap();

        // Bare logo.
        let mut lp = Pixmap::new(size, size).unwrap();
        draw_logo(&mut lp, &tree, s / 2.0, s / 2.0, s * 0.96);
        lp.save_png(out.join(format!("logo_{size}.png"))).unwrap();
    }
    println!("wrote assets/icon");
}
