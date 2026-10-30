//! CPU renderer for physical dock faces. Text rasterization follows CoolerScreen's
//! ab_glyph approach (coolerscreen b68d2e8, live/render.rs). No device or account reads.
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const KEY_SIZE: (u32, u32) = (196, 196);
pub const WIDE_SIZE: (u32, u32) = (458, 196);
const BG: [u8; 3] = [15, 17, 21];
const INK: [u8; 3] = [232, 234, 238];
const MUTED: [u8; 3] = [154, 162, 177];

#[derive(Clone, Debug, Default)]
pub struct RenderInputs {
    /// Supplied by the caller; empty means unavailable, never an invented time.
    pub clock: Option<String>,
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub ram: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedFace {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    /// True when the face represents unavailable data.
    pub unknown: bool,
}

fn font() -> Result<&'static FontArc, String> {
    static FONT: OnceLock<Result<FontArc, String>> = OnceLock::new();
    FONT.get_or_init(|| {
        let root =
            std::env::var_os("SystemRoot").ok_or("Windows system font folder is unavailable")?;
        let dir = std::path::PathBuf::from(root).join("Fonts");
        ["segoeuib.ttf", "arialbd.ttf", "segoeui.ttf"]
            .iter()
            .find_map(|name| {
                std::fs::read(dir.join(name))
                    .ok()
                    .and_then(|data| FontArc::try_from_vec(data).ok())
            })
            .ok_or_else(|| "A supported Windows font is unavailable".into())
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Plans visible content before drawing. Usage fields have no numerical input in
/// this API: until a verified reader is connected, all account faces stay unknown.
pub fn visible_content(
    kind: &str,
    text: &str,
    input: &RenderInputs,
) -> Result<(String, String, bool), String> {
    if text.chars().count() > 40 || text.chars().any(|c| c.is_control()) {
        return Err("Display text must contain at most 40 printable characters".into());
    }
    let unavailable = |caption: &str| ("—".to_string(), caption.to_string(), true);
    Ok(match kind {
        "text" => (
            if text.is_empty() { "+" } else { text }.to_string(),
            String::new(),
            false,
        ),
        "icon" => (
            if text.is_empty() { "+" } else { text }.to_string(),
            String::new(),
            false,
        ),
        "clock" => match input.clock.as_deref().filter(|t| !t.is_empty()) {
            Some(t) if t.len() <= 8 && t.chars().all(|c| c.is_ascii_digit() || c == ':') => {
                (t.into(), String::new(), false)
            }
            Some(_) => return Err("Invalid clock display".into()),
            None => unavailable("No time"),
        },
        "claude" | "codex" | "reset" | "trend" => unavailable("No data"),
        "cpu" | "gpu" | "ram" => {
            let value = match kind {
                "cpu" => input.cpu,
                "gpu" => input.gpu,
                _ => input.ram,
            };
            match value.filter(|n| n.is_finite() && (0.0..=100.0).contains(n)) {
                Some(n) => (format!("{n:.0}%"), kind.to_uppercase(), false),
                None => unavailable(&kind.to_uppercase()),
            }
        }
        _ => return Err("Unsupported face presentation".into()),
    })
}

fn width(font: &FontArc, text: &str, size: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars()
        .map(|c| scaled.h_advance(font.glyph_id(c)))
        .sum()
}

fn wrap(font: &FontArc, text: &str, size: f32, max_width: f32) -> Vec<String> {
    let mut rows = vec![String::new()];
    for c in text.chars() {
        let last = rows.last_mut().unwrap();
        let candidate = format!("{last}{c}");
        if !last.is_empty() && width(font, &candidate, size) > max_width {
            rows.push(c.to_string());
        } else {
            last.push(c);
        }
    }
    rows
}

fn draw_text(
    image: &mut [u8],
    dimensions: (u32, u32),
    font: &FontArc,
    text: &str,
    size: f32,
    baseline: f32,
    color: [u8; 3],
) -> Result<(), String> {
    let (w, h) = dimensions;
    let mut pen = (w as f32 - width(font, text, size)) / 2.0;
    let scaled = font.as_scaled(PxScale::from(size));
    for c in text.chars() {
        let id = font.glyph_id(c);
        if id.0 == 0 {
            return Err("The system font does not support a display character".into());
        }
        let glyph = id.with_scale_and_position(size, ab_glyph::point(pen, baseline));
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|x, y, coverage| {
                let xx = bounds.min.x as i32 + x as i32;
                let yy = bounds.min.y as i32 + y as i32;
                if xx >= 0 && xx < w as i32 && yy >= 0 && yy < h as i32 {
                    let at = (yy as usize * w as usize + xx as usize) * 3;
                    for ch in 0..3 {
                        image[at + ch] = (image[at + ch] as f32 * (1.0 - coverage)
                            + color[ch] as f32 * coverage)
                            .round() as u8;
                    }
                }
            });
        }
        pen += scaled.h_advance(id);
    }
    Ok(())
}

/// Render only the two physical geometries, with opaque RGB PNG output.
pub fn render_face(
    kind: &str,
    text: &str,
    wide: bool,
    input: &RenderInputs,
) -> Result<RenderedFace, String> {
    let (content, caption, unknown) = visible_content(kind, text, input)?;
    let dimensions = if wide { WIDE_SIZE } else { KEY_SIZE };
    let (w, h) = dimensions;
    let mut rgb: Vec<u8> = BG.into_iter().cycle().take((w * h * 3) as usize).collect();
    let font = font()?;
    let mut size = if wide { 60.0 } else { 56.0 };
    let mut rows = wrap(font, &content, size, w as f32 - 24.0);
    let max_height = if caption.is_empty() { 168.0 } else { 132.0 };
    while rows.len() as f32 * size * 1.2 > max_height && size > 22.0 {
        size -= 2.0;
        rows = wrap(font, &content, size, w as f32 - 24.0);
    }
    if rows.len() as f32 * size * 1.2 > max_height {
        return Err("Display text cannot fit legibly".into());
    }
    let total = rows.len() as f32 * size * 1.2;
    let center = if caption.is_empty() { 98.0 } else { 80.0 };
    let start = center - total / 2.0 + size * 0.9;
    for (i, row) in rows.iter().enumerate() {
        draw_text(
            &mut rgb,
            dimensions,
            font,
            row,
            size,
            start + i as f32 * size * 1.2,
            INK,
        )?;
    }
    if !caption.is_empty() {
        draw_text(&mut rgb, dimensions, font, &caption, 23.0, 165.0, MUTED)?;
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|e| e.to_string())?
            .write_image_data(&rgb)
            .map_err(|e| e.to_string())?;
    }
    Ok(RenderedFace {
        width: w,
        height: h,
        png,
        unknown,
    })
}

/// Keeps only the last frame for each physical display; unchanged PNGs are not
/// scheduled for transmission. This cache has a fixed 14-display bound.
#[derive(Default)]
pub struct FrameCache {
    frames: HashMap<usize, Vec<u8>>,
}
impl FrameCache {
    pub fn needs_update(&self, index: usize, png: &[u8]) -> Result<bool, String> {
        if index >= 14 || png.len() > 2_000_000 {
            return Err("Invalid dock frame".into());
        }
        Ok(!self.frames.get(&index).is_some_and(|old| old == png))
    }
    /// Call only after the transport accepts the whole update successfully.
    pub fn changed(&mut self, index: usize, png: &[u8]) -> Result<bool, String> {
        if !self.needs_update(index, png)? {
            return Ok(false);
        }
        self.frames.insert(index, png.to_vec());
        Ok(true)
    }
    pub fn clear(&mut self) {
        self.frames.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_accounts_cannot_show_numbers_or_text_injections() {
        for kind in ["claude", "codex", "reset", "trend"] {
            assert_eq!(
                visible_content(kind, "99%", &RenderInputs::default()).unwrap(),
                ("—".into(), "No data".into(), true)
            );
        }
    }
    #[test]
    fn unavailable_and_invalid_inputs_stay_unknown() {
        for value in [None, Some(f32::NAN), Some(-1.0), Some(101.0)] {
            assert!(
                visible_content(
                    "cpu",
                    "",
                    &RenderInputs {
                        cpu: value,
                        ..Default::default()
                    }
                )
                .unwrap()
                .2
            );
        }
        assert!(
            visible_content("clock", "", &RenderInputs::default())
                .unwrap()
                .2
        );
        assert!(visible_content(
            "clock",
            "",
            &RenderInputs {
                clock: Some("bad".into()),
                ..Default::default()
            }
        )
        .is_err());
        assert!(visible_content("text", &"x".repeat(41), &RenderInputs::default()).is_err());
    }
    #[test]
    fn changed_frames_are_bounded_and_suppressed() {
        let mut cache = FrameCache::default();
        assert!(cache.changed(0, b"a").unwrap());
        assert!(!cache.changed(0, b"a").unwrap());
        assert!(cache.changed(0, b"b").unwrap());
        assert!(cache.changed(14, b"a").is_err());
        cache.clear();
        assert!(cache.changed(0, b"b").unwrap());
    }
    #[cfg(windows)]
    #[test]
    fn actual_png_dimensions_and_font_pixels() {
        for wide in [false, true] {
            let frame = render_face("text", "Focus", wide, &RenderInputs::default()).unwrap();
            let decoder = png::Decoder::new(std::io::Cursor::new(&frame.png));
            let mut reader = decoder.read_info().unwrap();
            assert_eq!(
                (reader.info().width, reader.info().height),
                if wide { WIDE_SIZE } else { KEY_SIZE }
            );
            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut buf).unwrap();
            assert!(buf[..info.buffer_size()].chunks_exact(3).any(|p| p != BG));
            assert_eq!(
                frame.png,
                render_face("text", "Focus", wide, &RenderInputs::default())
                    .unwrap()
                    .png
            );
        }
    }
}
