//! Color resolution and pixel-exact drawing of box-drawing / block-element characters.
//!
//! Terminal UIs (htop, tmux, TUI apps) rely on box-drawing and block characters joining
//! seamlessly across cells. Font glyphs rarely fill a cell whose height includes extra line
//! spacing, so we draw those characters as shapes instead.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

use crate::theme::{TerminalColors, mix};

/// Resolves terminal colors against the theme palette and any OSC 4/10/11 overrides.
pub struct Palette<'a> {
    pub theme: &'a TerminalColors,
    pub overrides: &'a Colors,
}

fn rgb(c: Rgb) -> Color32 {
    Color32::from_rgb(c.r, c.g, c.b)
}

impl Palette<'_> {
    /// Color of the xterm 256-color index `i`.
    pub fn indexed(&self, i: u8) -> Color32 {
        if let Some(c) = self.overrides[i as usize] {
            return rgb(c);
        }
        match i {
            0..=15 => self.theme.ansi[i as usize],
            16..=231 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            _ => {
                let v = 8 + (i - 232) * 10;
                Color32::from_rgb(v, v, v)
            }
        }
    }

    pub fn named(&self, n: NamedColor) -> Color32 {
        let idx = n as usize;
        if let Some(c) = self.overrides[idx] {
            return rgb(c);
        }
        match n {
            NamedColor::Foreground => self.theme.foreground,
            NamedColor::Background => self.theme.background,
            NamedColor::Cursor => self.theme.foreground,
            NamedColor::BrightForeground => self.theme.bright_foreground,
            NamedColor::DimForeground => self.theme.dim_foreground,
            NamedColor::DimBlack => self.theme.dim[0],
            NamedColor::DimRed => self.theme.dim[1],
            NamedColor::DimGreen => self.theme.dim[2],
            NamedColor::DimYellow => self.theme.dim[3],
            NamedColor::DimBlue => self.theme.dim[4],
            NamedColor::DimMagenta => self.theme.dim[5],
            NamedColor::DimCyan => self.theme.dim[6],
            NamedColor::DimWhite => self.theme.dim[7],
            _ if idx < 16 => self.theme.ansi[idx],
            _ => self.theme.foreground,
        }
    }

    pub fn resolve(&self, c: Color) -> Color32 {
        match c {
            Color::Named(n) => self.named(n),
            Color::Indexed(i) => self.indexed(i),
            Color::Spec(c) => rgb(c),
        }
    }

    /// Dimmed foreground (SGR 2).
    pub fn dim(&self, c: Color) -> Color32 {
        match c {
            Color::Named(n) if (n as usize) < 8 => self.theme.dim[n as usize],
            Color::Named(NamedColor::Foreground) => self.theme.dim_foreground,
            other => mix(self.resolve(other), self.theme.background, 0.4),
        }
    }

    /// Is this the terminal's default background (which we don't need to paint)?
    pub fn is_default_bg(&self, c: Color) -> bool {
        matches!(c, Color::Named(NamedColor::Background)) && self.overrides[NamedColor::Background as usize].is_none()
    }
}

/// Line weight of one arm of a box-drawing character.
#[derive(Clone, Copy, PartialEq, Eq)]
enum W {
    N,
    L,
    H,
    D,
}

/// Arms (up, right, down, left) for box-drawing characters we draw ourselves.
fn box_arms(c: char) -> Option<[W; 4]> {
    use W::*;
    Some(match c {
        '─' => [N, L, N, L],
        '━' => [N, H, N, H],
        '│' => [L, N, L, N],
        '┃' => [H, N, H, N],
        '┌' => [N, L, L, N],
        '┍' => [N, H, L, N],
        '┎' => [N, L, H, N],
        '┏' => [N, H, H, N],
        '┐' => [N, N, L, L],
        '┑' => [N, N, L, H],
        '┒' => [N, N, H, L],
        '┓' => [N, N, H, H],
        '└' => [L, L, N, N],
        '┕' => [L, H, N, N],
        '┖' => [H, L, N, N],
        '┗' => [H, H, N, N],
        '┘' => [L, N, N, L],
        '┙' => [L, N, N, H],
        '┚' => [H, N, N, L],
        '┛' => [H, N, N, H],
        '├' => [L, L, L, N],
        '┝' => [L, H, L, N],
        '┠' => [H, L, H, N],
        '┣' => [H, H, H, N],
        '┤' => [L, N, L, L],
        '┥' => [L, N, L, H],
        '┨' => [H, N, H, L],
        '┫' => [H, N, H, H],
        '┬' => [N, L, L, L],
        '┯' => [N, H, L, H],
        '┰' => [N, L, H, L],
        '┳' => [N, H, H, H],
        '┴' => [L, L, N, L],
        '┷' => [L, H, N, H],
        '┸' => [H, L, N, L],
        '┻' => [H, H, N, H],
        '┼' => [L, L, L, L],
        '┿' => [L, H, L, H],
        '╂' => [H, L, H, L],
        '╋' => [H, H, H, H],
        '═' => [N, D, N, D],
        '║' => [D, N, D, N],
        '╔' => [N, D, D, N],
        '╗' => [N, N, D, D],
        '╚' => [D, D, N, N],
        '╝' => [D, N, N, D],
        '╠' => [D, D, D, N],
        '╣' => [D, N, D, D],
        '╦' => [N, D, D, D],
        '╩' => [D, D, N, D],
        '╬' => [D, D, D, D],
        '╴' => [N, N, N, L],
        '╵' => [L, N, N, N],
        '╶' => [N, L, N, N],
        '╷' => [N, N, L, N],
        '╸' => [N, N, N, H],
        '╹' => [H, N, N, N],
        '╺' => [N, H, N, N],
        '╻' => [N, N, H, N],
        '╼' => [N, H, N, L],
        '╽' => [L, N, H, N],
        '╾' => [N, L, N, H],
        '╿' => [H, N, L, N],
        _ => return None,
    })
}

/// Does this character get drawn as shapes rather than a font glyph?
pub fn is_special(c: char) -> bool {
    box_arms(c).is_some() || matches!(c, '╭' | '╮' | '╯' | '╰') || ('\u{2580}'..='\u{259F}').contains(&c)
}

/// Draw `c` filling `cell` (already pixel-aligned). Returns false if `c` isn't special.
pub fn draw_special(painter: &Painter, cell: Rect, c: char, color: Color32, ppp: f32) -> bool {
    let px = 1.0 / ppp;
    let snap = |v: f32| (v * ppp).round() / ppp;
    let light = snap((cell.width() / 9.0).max(px)).max(px);
    let heavy = snap(light * 2.0).max(2.0 * px);
    let cx = snap(cell.center().x);
    let cy = snap(cell.center().y);

    if let Some(arms) = box_arms(c) {
        let [up, right, down, left] = arms;
        let half = |w: W| match w {
            W::N => 0.0,
            W::L => light / 2.0,
            W::H => heavy / 2.0,
            W::D => light * 1.5,
        };
        // Extend each arm into the center far enough to join with the widest crossing arm.
        let join_v = half(left).max(half(right));
        let join_h = half(up).max(half(down));
        let mut fill = |r: Rect| {
            painter.rect_filled(r, 0.0, color);
        };
        let gap = light; // spacing between the two strokes of a double line
        let vert = |fill: &mut dyn FnMut(Rect), w: W, y0: f32, y1: f32| match w {
            W::N => {}
            W::L | W::H => {
                let t = if w == W::L { light } else { heavy };
                fill(Rect::from_min_max(pos2(cx - t / 2.0, y0.min(y1)), pos2(cx + t / 2.0, y0.max(y1))));
            }
            W::D => {
                for dx in [-gap - light / 2.0, gap + light / 2.0] {
                    let x = snap(cx + dx);
                    fill(Rect::from_min_max(pos2(x - light / 2.0, y0.min(y1)), pos2(x + light / 2.0, y0.max(y1))));
                }
            }
        };
        let horiz = |fill: &mut dyn FnMut(Rect), w: W, x0: f32, x1: f32| match w {
            W::N => {}
            W::L | W::H => {
                let t = if w == W::L { light } else { heavy };
                fill(Rect::from_min_max(pos2(x0.min(x1), cy - t / 2.0), pos2(x0.max(x1), cy + t / 2.0)));
            }
            W::D => {
                for dy in [-gap - light / 2.0, gap + light / 2.0] {
                    let y = snap(cy + dy);
                    fill(Rect::from_min_max(pos2(x0.min(x1), y - light / 2.0), pos2(x0.max(x1), y + light / 2.0)));
                }
            }
        };
        vert(&mut fill, up, cell.top(), cy + join_v);
        vert(&mut fill, down, cy - join_v, cell.bottom());
        horiz(&mut fill, left, cell.left(), cx + join_h);
        horiz(&mut fill, right, cx - join_h, cell.right());
        return true;
    }

    // Rounded corners: straight arms joined by a quarter circle.
    let arc = match c {
        '╭' => Some((1.0, 1.0)),
        '╮' => Some((-1.0, 1.0)),
        '╯' => Some((-1.0, -1.0)),
        '╰' => Some((1.0, -1.0)),
        _ => None,
    };
    if let Some((sx, sy)) = arc {
        let r = (cell.width() / 2.0).min(cell.height() / 2.0);
        let center = pos2(cx + sx * r, cy + sy * r);
        let mut pts: Vec<Pos2> = Vec::with_capacity(14);
        // Vertical arm from the cell edge to where the arc starts.
        let edge_y = if sy > 0.0 { cell.bottom() } else { cell.top() };
        let edge_x = if sx > 0.0 { cell.right() } else { cell.left() };
        pts.push(pos2(cx, edge_y));
        let n = 10;
        for i in 0..=n {
            let t = i as f32 / n as f32 * std::f32::consts::FRAC_PI_2;
            // From the point (cx, center.y) to (center.x, cy).
            pts.push(pos2(center.x - sx * r * t.cos(), center.y - sy * r * t.sin()));
        }
        pts.push(pos2(edge_x, cy));
        painter.add(Shape::line(pts, Stroke::new(light, color)));
        return true;
    }

    // Block elements.
    let (x0, x1, y0, y1) = (cell.left(), cell.right(), cell.top(), cell.bottom());
    let w = cell.width();
    let h = cell.height();
    let rect = |l: f32, t: f32, r: f32, b: f32| Rect::from_min_max(pos2(snap(l), snap(t)), pos2(snap(r), snap(b)));
    let quads = |ul: bool, ur: bool, ll: bool, lr: bool| {
        let mut v = Vec::new();
        if ul {
            v.push(rect(x0, y0, cx, cy));
        }
        if ur {
            v.push(rect(cx, y0, x1, cy));
        }
        if ll {
            v.push(rect(x0, cy, cx, y1));
        }
        if lr {
            v.push(rect(cx, cy, x1, y1));
        }
        v
    };
    let rects: Vec<Rect> = match c {
        '▀' => vec![rect(x0, y0, x1, cy)],
        '▁'..='▇' => {
            let eighths = (c as u32 - '▁' as u32 + 1) as f32;
            vec![rect(x0, y1 - h * eighths / 8.0, x1, y1)]
        }
        '█' => vec![cell],
        '▉'..='▏' => {
            let eighths = (8 - (c as u32 - '▉' as u32 + 1)) as f32;
            vec![rect(x0, y0, x0 + w * eighths / 8.0, y1)]
        }
        '▐' => vec![rect(cx, y0, x1, y1)],
        '▔' => vec![rect(x0, y0, x1, y0 + h / 8.0)],
        '▕' => vec![rect(x1 - w / 8.0, y0, x1, y1)],
        '░' | '▒' | '▓' => {
            let a = match c {
                '░' => 0.25,
                '▒' => 0.5,
                _ => 0.75,
            };
            painter.rect_filled(cell, 0.0, color.gamma_multiply(a));
            return true;
        }
        '▖' => quads(false, false, true, false),
        '▗' => quads(false, false, false, true),
        '▘' => quads(true, false, false, false),
        '▙' => quads(true, false, true, true),
        '▚' => quads(true, false, false, true),
        '▛' => quads(true, true, true, false),
        '▜' => quads(true, true, false, true),
        '▝' => quads(false, true, false, false),
        '▞' => quads(false, true, true, false),
        '▟' => quads(false, true, true, true),
        _ => return false,
    };
    for r in rects {
        painter.rect_filled(r, 0.0, color);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_and_grayscale() {
        let theme = crate::theme::ThemeRegistry::load().get_or_default(crate::theme::DEFAULT_THEME).terminal;
        let overrides = Colors::default();
        let p = Palette { theme: &theme, overrides: &overrides };
        assert_eq!(p.indexed(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(p.indexed(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(p.indexed(196), Color32::from_rgb(255, 0, 0));
        assert_eq!(p.indexed(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(p.indexed(1), theme.ansi[1]);
        assert_eq!(p.resolve(Color::Named(NamedColor::BrightBlue)), theme.ansi[12]);
    }

    #[test]
    fn specials() {
        assert!(is_special('─') && is_special('╭') && is_special('█') && is_special('▒'));
        assert!(!is_special('a') && !is_special('┄'));
    }
}
