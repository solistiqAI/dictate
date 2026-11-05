//! The menu bar glyph: the voice bars of the app icon (`assets/app-icon.svg`) on their own, as a
//! black template image. The menu bar shows what an app does; a team's mark, if any, stays on the
//! app icon (Dock, Finder, Spotlight).

/// Voice bars in points: height of each, centred on the image's middle, with rounded ends. 16 pt
/// wide and 15 pt tall at most: bars with air between them look smaller than the filled glyphs
/// beside them (16 pt), so they are not drawn any smaller.
const BARS: [f32; 5] = [4.0, 10.0, 15.0, 8.5, 4.5];
const BAR_W: f32 = 2.0;
const BAR_GAP: f32 = 1.5;

/// Pixels per point and supersampling.
const SCALE: f32 = 4.0;
const SUPER: u32 = 4;
/// The menu bar shows the image 18 pt high; a margin of 0.5 pt left and right.
const HEIGHT_PT: f32 = 18.0;
const MARGIN_PT: f32 = 0.5;

/// The bars' total width in points.
fn bars_width() -> f32 {
    BARS.len() as f32 * BAR_W + (BARS.len() - 1) as f32 * BAR_GAP
}

/// RGBA pixels, width, height.
pub fn render() -> (Vec<u8>, u32, u32) {
    let height = (HEIGHT_PT * SCALE) as u32;
    let width = ((bars_width() + 2.0 * MARGIN_PT) * SCALE).ceil() as u32;
    let ox = (width as f32 / SCALE - bars_width()) / 2.0;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for py in 0..height {
        for px in 0..width {
            let mut sum = 0.0;
            for sy in 0..SUPER {
                for sx in 0..SUPER {
                    // Pixel sample → points, x from the first bar's left edge.
                    let x = (px as f32 + (sx as f32 + 0.5) / SUPER as f32) / SCALE - ox;
                    let y = (py as f32 + (sy as f32 + 0.5) / SUPER as f32) / SCALE;
                    sum += sample(x, y);
                }
            }
            let alpha = sum / (SUPER * SUPER) as f32;
            rgba.extend_from_slice(&[0, 0, 0, (alpha * 255.0).round() as u8]);
        }
    }
    (rgba, width, height)
}

/// Opacity at one point: 1 inside a bar, else 0.
fn sample(x: f32, y: f32) -> f32 {
    let hit = BARS.iter().enumerate().any(|(i, &h)| {
        let left = i as f32 * (BAR_W + BAR_GAP);
        in_bar(x, y, left, h)
    });
    if hit { 1.0 } else { 0.0 }
}

/// Point in a vertical bar with fully rounded ends, centred on the image's middle.
fn in_bar(x: f32, y: f32, left: f32, height: f32) -> bool {
    let r = BAR_W / 2.0;
    let cx = left + r;
    let cy_mid = HEIGHT_PT / 2.0;
    let (top, bottom) = (cy_mid - height / 2.0 + r, cy_mid + height / 2.0 - r);
    let cy = y.clamp(top, bottom);
    (x - cx).hypot(y - cy) <= r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_is_menu_bar_sized_bars() {
        let (rgba, w, h) = render();
        assert_eq!(h, 72);
        // About 16 pt wide, like the icons beside it in the menu bar.
        assert!((64..=72).contains(&w), "width {w}");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        let alphas: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
        let at = |x: usize, y: usize| alphas[y * w as usize + x];
        let mid = h as usize / 2;
        // The middle (tallest) bar is solid at the centre and near its top; the short outer ones
        // are not that tall.
        assert_eq!(at(w as usize / 2, mid), 255, "middle bar");
        let near_top = mid - (5.5 * SCALE) as usize;
        assert_eq!(at(w as usize / 2, near_top), 255, "middle bar is tall");
        let first = ((MARGIN_PT + BAR_W / 2.0) * SCALE) as usize;
        assert_eq!(at(first, mid), 255, "first bar");
        assert_eq!(at(first, near_top), 0, "first bar is short");
        // Gaps between bars and the corners are transparent.
        let gap = ((MARGIN_PT + BAR_W + BAR_GAP / 2.0) * SCALE) as usize;
        assert_eq!(at(gap, mid), 0, "gap");
        assert_eq!(alphas[0], 0);
    }

    /// `DICTATE_GLYPH_RGBA=/tmp/glyph.rgba cargo test -- --ignored dump_glyph` writes the raw pixels.
    #[test]
    #[ignore]
    fn dump_glyph() {
        let (rgba, w, h) = render();
        let path = std::env::var("DICTATE_GLYPH_RGBA").expect("DICTATE_GLYPH_RGBA");
        std::fs::write(&path, rgba).unwrap();
        println!("{w}x{h}");
    }
}
