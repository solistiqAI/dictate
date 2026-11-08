//! The menu bar image: the app icon's mark and voice bars (`assets/app-icon.svg`), drawn for 18 pt
//! as a black template image (`assets/menubar.svg` → `assets/MenuBarIcon.png`, embedded). A team's
//! build can bring its own: `MenuBarIcon.png` in its brand folder, which `scripts/bundle.sh` copies
//! into the app's Resources.
//!
//! Only the image's opacity counts: macOS tints a template image like the icons beside it (Windows
//! whitens it, `app.rs`). The menu bar shows it 18 pt high, so 72 px for 4 px per point.

/// The built-in image (`assets/menubar.svg`, rendered as the "Icon" line of `AGENTS.md` says).
const DEFAULT: &[u8] = include_bytes!("../assets/MenuBarIcon.png");

/// A brand's file in `Dictate.app/Contents/Resources` (or next to the executable elsewhere).
const BRAND_FILE: &str = "MenuBarIcon.png";

/// RGBA pixels, width, height: the brand's image if the app has one, else the built-in one.
pub fn render() -> (Vec<u8>, u32, u32) {
    brand().unwrap_or_else(|| decode(DEFAULT).expect("assets/MenuBarIcon.png"))
}

/// The brand's image; `None` without one, or when it does not decode (logged).
fn brand() -> Option<(Vec<u8>, u32, u32)> {
    let exe = std::env::current_exe()
        .and_then(|e| e.canonicalize())
        .ok()?;
    let dir = exe.parent()?;
    let candidates = [
        dir.join("../Resources").join(BRAND_FILE),
        dir.join(BRAND_FILE),
    ];
    let path = candidates.iter().find(|p| p.is_file())?;
    match std::fs::read(path)
        .map_err(anyhow::Error::from)
        .and_then(|b| decode(&b))
    {
        Ok(image) => Some(image),
        Err(e) => {
            eprintln!(
                "menu bar image {}: {e:#}; the built-in one instead",
                path.display()
            );
            None
        }
    }
}

/// A PNG as black pixels with its opacity (RGBA, width, height).
fn decode(bytes: &[u8]) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palette, grey and 16-bit images to 8-bit RGB(A) or grey(+alpha).
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or_else(|| anyhow::anyhow!("too large"))?
    ];
    let info = reader.next_frame(&mut buf)?;
    let (width, height) = (info.width, info.height);
    anyhow::ensure!(
        width > 0 && height > 0 && width <= 1024 && height <= 256,
        "size {width}×{height}"
    );
    let channels = info.color_type.samples();
    let alpha = |px: &[u8]| match info.color_type {
        png::ColorType::Rgba | png::ColorType::GrayscaleAlpha => px[channels - 1],
        _ => 255,
    };
    let rgba = buf[..info.buffer_size()]
        .chunks_exact(channels)
        .flat_map(|px| [0, 0, 0, alpha(px)])
        .collect();
    Ok((rgba, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_image_is_menu_bar_sized() {
        let (rgba, w, h) = decode(DEFAULT).unwrap();
        assert_eq!(h, 72, "18 pt at 4 px per point");
        // Mark and bars: wider than tall, but not a banner.
        assert!((72..=120).contains(&w), "width {w}");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        let alphas: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alphas[0], 0, "transparent corner");
        assert!(
            alphas.iter().filter(|&&a| a == 255).count() > 500,
            "solid shapes"
        );
    }

    fn png(color: png::ColorType, width: u32, height: u32, data: &[u8]) -> Vec<u8> {
        let mut out = vec![];
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
        out
    }

    #[test]
    fn image_keeps_only_its_opacity() {
        // Two pixels: white half-transparent, red opaque → black with the same opacity.
        let bytes = png(
            png::ColorType::Rgba,
            2,
            1,
            &[255, 255, 255, 128, 255, 0, 0, 255],
        );
        let (rgba, w, h) = decode(&bytes).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgba, [0, 0, 0, 128, 0, 0, 0, 255]);
        // Without an alpha channel every pixel is opaque.
        let (rgba, _, _) = decode(&png(png::ColorType::Grayscale, 1, 1, &[7])).unwrap();
        assert_eq!(rgba, [0, 0, 0, 255]);
        assert!(decode(b"not a png").is_err());
    }
}
