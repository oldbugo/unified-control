//! CPU renderer for physical dock faces. Text rasterization follows CoolerScreen's
//! ab_glyph approach (coolerscreen b68d2e8, live/render.rs). No device or account reads.
use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, OnceLock};

pub const KEY_SIZE: (u32, u32) = (196, 196);
pub const WIDE_SIZE: (u32, u32) = (458, 196);
const BG: [u8; 3] = [15, 17, 21];
const INK: [u8; 3] = [232, 234, 238];
const MUTED: [u8; 3] = [154, 162, 177];

#[derive(Clone, Debug, Default)]
pub struct RenderInputs {
    /// Supplied by the caller; empty means unavailable, never an invented time.
    pub clock: Option<String>,
    /// YYYY-MM-DD, supplied by the caller alongside its local clock.
    pub date: Option<String>,
    /// One of ICON_NAMES; absent uses the reviewed app icon.
    pub icon: Option<String>,
    /// Already decoded by an explicit import/asset lookup. Never a path or URL.
    pub picture: Option<Arc<PictureAsset>>,
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub ram: Option<f32>,
}

pub const MAX_PICTURE_BYTES: usize = 4 * 1024 * 1024;
pub const ICON_NAMES: &[&str] = &[
    "app", "folder", "back", "play", "pause", "next", "previous", "volume", "mute", "monitor",
    "globe", "keyboard", "clock",
];

/// Bounded PNG/JPEG import, normalized once to <=1024 on its long side.
/// The caller owns local asset IDs; this renderer never opens files or URLs.
#[derive(Clone, Debug)]
pub struct PictureAsset {
    rgba: image::RgbaImage,
    normalized_png: Vec<u8>,
}
impl PictureAsset {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use image::{ImageFormat, ImageReader};
        if bytes.is_empty() || bytes.len() > MAX_PICTURE_BYTES {
            return Err("Choose a PNG or JPEG of at most 4 MiB".into());
        }
        let format = image::guess_format(bytes).map_err(|_| "Choose a valid PNG or JPEG")?;
        if !matches!(format, ImageFormat::Png | ImageFormat::Jpeg) {
            return Err("Only PNG and JPEG pictures are supported".into());
        }
        let (w, h) = ImageReader::with_format(Cursor::new(bytes), format)
            .into_dimensions()
            .map_err(|_| "The picture header is invalid")?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 || u64::from(w) * u64::from(h) > 16_000_000 {
            return Err(
                "Pictures must be at most 4096 pixels per side and 16 million pixels".into(),
            );
        }
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader
            .decode()
            .map_err(|_| "The picture could not be decoded within the import limits")?;
        let rgba = if w > 1024 || h > 1024 {
            decoded.thumbnail(1024, 1024).to_rgba8()
        } else {
            decoded.to_rgba8()
        };
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(rgba.clone())
            .write_to(&mut out, ImageFormat::Png)
            .map_err(|_| "The imported picture could not be stored")?;
        let normalized_png = out.into_inner();
        if normalized_png.len() > MAX_PICTURE_BYTES {
            return Err("The normalized picture exceeds 4 MiB".into());
        }
        Ok(Self {
            rgba,
            normalized_png,
        })
    }
    pub fn png(&self) -> &[u8] {
        &self.normalized_png
    }
    pub fn dimensions(&self) -> (u32, u32) {
        self.rgba.dimensions()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderedFace {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    /// True when the face represents unavailable data.
    pub unknown: bool,
    /// Deadline for current numerical account pixels. Rechecked at batch send.
    pub valid_until_ms: Option<u64>,
}
impl RenderedFace {
    pub fn guard_delivery(&mut self, now: u64) -> Result<(), String> {
        if self.valid_until_ms.is_some_and(|deadline| deadline <= now) {
            *self = render_face(
                "reset",
                "",
                self.width == WIDE_SIZE.0,
                &RenderInputs::default(),
            )?;
        }
        Ok(())
    }
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
        "icon" => {
            if !ICON_NAMES.contains(&input.icon.as_deref().unwrap_or("app")) {
                return Err("Unsupported icon".into());
            }
            (text.to_string(), String::new(), false)
        }
        "picture" => {
            if input.picture.is_some() {
                (text.to_string(), String::new(), false)
            } else {
                unavailable("No picture")
            }
        }
        "clock" => match input.clock.as_deref().filter(|t| !t.is_empty()) {
            Some(t) if valid_clock(t) => (t.into(), String::new(), false),
            Some(_) => return Err("Invalid clock display".into()),
            None => unavailable("No time"),
        },
        "date" => match input.date.as_deref() {
            Some(t) if valid_date(t) => (t.into(), String::new(), false),
            Some(_) => return Err("Invalid date display".into()),
            None => unavailable("No date"),
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

fn valid_date(s: &str) -> bool {
    if s.len() != 10 || !s.is_ascii() {
        return false;
    }
    let b = s.as_bytes();
    if b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = s[..4].parse::<u32>().unwrap_or(0);
    let month = s[5..7].parse::<u32>().unwrap_or(0);
    let day = s[8..].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}
fn valid_clock(s: &str) -> bool {
    if !s.is_ascii() || !matches!(s.len(), 5 | 8) {
        return false;
    }
    let chunks: Vec<_> = s.split(':').collect();
    chunks.len() == if s.len() == 5 { 2 } else { 3 }
        && chunks
            .iter()
            .all(|v| v.len() == 2 && v.bytes().all(|b| b.is_ascii_digit()))
        && chunks[0].parse::<u8>().is_ok_and(|n| n < 24)
        && chunks[1..]
            .iter()
            .all(|s| s.parse::<u8>().is_ok_and(|n| n < 60))
}

fn blend(rgb: &mut [u8], dimensions: (u32, u32), x: u32, y: u32, color: [u8; 3], alpha: f32) {
    let (w, h) = dimensions;
    if x >= w || y >= h {
        return;
    }
    let at = ((y * w + x) * 3) as usize;
    for c in 0..3 {
        rgb[at + c] = (rgb[at + c] as f32 * (1.0 - alpha) + color[c] as f32 * alpha).round() as u8;
    }
}
// Small fixed vector set drawn from reviewed geometry, never arbitrary SVG input.
fn draw_icon(rgb: &mut [u8], dimensions: (u32, u32), name: &str, has_label: bool) {
    let size = if has_label { 104.0 } else { 128.0 };
    let left = (dimensions.0 as f32 - size) / 2.0;
    let top = if has_label { 22.0 } else { 34.0 };
    let mut line = |a: (f32, f32), b: (f32, f32)| {
        let (ax, ay) = (left + a.0 / 100.0 * size, top + a.1 / 100.0 * size);
        let (bx, by) = (left + b.0 / 100.0 * size, top + b.1 / 100.0 * size);
        let radius = 3.0f32;
        for y in (ay.min(by) - radius - 1.0).max(0.0) as u32
            ..=((ay.max(by) + radius + 1.0) as u32).min(dimensions.1 - 1)
        {
            for x in (ax.min(bx) - radius - 1.0).max(0.0) as u32
                ..=((ax.max(bx) + radius + 1.0) as u32).min(dimensions.0 - 1)
            {
                let (dx, dy) = (bx - ax, by - ay);
                let len = dx * dx + dy * dy;
                let t = if len > 0.0 {
                    (((x as f32 + 0.5 - ax) * dx + (y as f32 + 0.5 - ay) * dy) / len)
                        .clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let distance = ((x as f32 + 0.5 - ax - t * dx).powi(2)
                    + (y as f32 + 0.5 - ay - t * dy).powi(2))
                .sqrt();
                let a = (radius + 0.5 - distance).clamp(0.0, 1.0);
                if a > 0.0 {
                    blend(rgb, dimensions, x, y, INK, a);
                }
            }
        }
    };
    let paths: Vec<Vec<(f32, f32)>> = match name {
        "folder" => vec![vec![
            (8., 25.),
            (38., 25.),
            (47., 35.),
            (92., 35.),
            (86., 82.),
            (8., 82.),
            (8., 25.),
        ]],
        "back" => vec![
            vec![(58., 20.), (25., 50.), (58., 80.)],
            vec![(25., 50.), (88., 50.)],
        ],
        "play" => vec![vec![(25., 15.), (85., 50.), (25., 85.), (25., 15.)]],
        "pause" => vec![vec![(30., 15.), (30., 85.)], vec![(70., 15.), (70., 85.)]],
        "next" => vec![
            vec![(15., 20.), (65., 50.), (15., 80.), (15., 20.)],
            vec![(80., 20.), (80., 80.)],
        ],
        "previous" => vec![
            vec![(85., 20.), (35., 50.), (85., 80.), (85., 20.)],
            vec![(20., 20.), (20., 80.)],
        ],
        "volume" | "mute" => {
            let mut v = vec![vec![
                (12., 36.),
                (30., 36.),
                (52., 16.),
                (52., 84.),
                (30., 64.),
                (12., 64.),
                (12., 36.),
            ]];
            if name == "mute" {
                v.extend([vec![(69., 35.), (91., 65.)], vec![(91., 35.), (69., 65.)]]);
            } else {
                v.extend([
                    vec![(68., 30.), (79., 50.), (68., 70.)],
                    vec![(82., 18.), (97., 50.), (82., 82.)],
                ]);
            }
            v
        }
        "monitor" => vec![
            vec![(5., 15.), (95., 15.), (95., 72.), (5., 72.), (5., 15.)],
            vec![(50., 72.), (50., 90.)],
            vec![(30., 90.), (70., 90.)],
        ],
        "keyboard" => vec![
            vec![(4., 24.), (96., 24.), (96., 80.), (4., 80.), (4., 24.)],
            vec![(25., 64.), (75., 64.)],
            vec![(22., 43.), (25., 43.)],
            vec![(40., 43.), (43., 43.)],
            vec![(58., 43.), (61., 43.)],
            vec![(76., 43.), (79., 43.)],
        ],
        "clock" | "globe" => {
            let circle = (0..=64)
                .map(|i| {
                    let a = i as f32 / 64.0 * std::f32::consts::TAU;
                    (50. + 41. * a.cos(), 50. + 41. * a.sin())
                })
                .collect();
            let mut v = vec![circle];
            if name == "clock" {
                v.push(vec![(50., 24.), (50., 50.), (72., 61.)]);
            } else {
                v.extend([
                    vec![(9., 50.), (91., 50.)],
                    vec![(50., 9.), (31., 30.), (31., 70.), (50., 91.)],
                    vec![(50., 9.), (69., 30.), (69., 70.), (50., 91.)],
                ]);
            }
            v
        }
        _ => vec![
            vec![(12., 12.), (42., 12.), (42., 42.), (12., 42.), (12., 12.)],
            vec![(58., 12.), (88., 12.), (88., 42.), (58., 42.), (58., 12.)],
            vec![(12., 58.), (42., 58.), (42., 88.), (12., 88.), (12., 58.)],
            vec![(58., 58.), (88., 58.), (88., 88.), (58., 88.), (58., 58.)],
        ],
    };
    for path in paths {
        for pair in path.windows(2) {
            line(pair[0], pair[1]);
        }
    }
}

fn draw_label(rgb: &mut [u8], dimensions: (u32, u32), text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let font = font()?;
    let mut size = 28.;
    while width(font, text, size) > dimensions.0 as f32 - 24. && size > 22. {
        size -= 1.;
    }
    let rows = wrap(font, text, size, dimensions.0 as f32 - 24.);
    if rows.len() > 2 {
        return Err("Picture and icon labels must fit in two lines".into());
    }
    let start = if rows.len() == 1 { 173. } else { 151. };
    for (i, row) in rows.iter().enumerate() {
        draw_text(
            rgb,
            dimensions,
            font,
            row,
            size,
            start + i as f32 * 27.,
            INK,
        )?;
    }
    Ok(())
}

fn draw_picture(rgb: &mut [u8], dimensions: (u32, u32), picture: &PictureAsset, has_label: bool) {
    let max_height = if has_label { 126 } else { dimensions.1 - 16 };
    let image = image::imageops::thumbnail(&picture.rgba, dimensions.0 - 16, max_height);
    let x0 = (dimensions.0 - image.width()) / 2;
    let y0 = if has_label {
        8 + (max_height - image.height()) / 2
    } else {
        (dimensions.1 - image.height()) / 2
    };
    for (x, y, p) in image.enumerate_pixels() {
        blend(
            rgb,
            dimensions,
            x + x0,
            y + y0,
            [p[0], p[1], p[2]],
            p[3] as f32 / 255.,
        );
    }
}

fn encode_rgb(rgb: &[u8], dimensions: (u32, u32), unknown: bool) -> Result<RenderedFace, String> {
    let (w, h) = dimensions;
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|e| e.to_string())?
        .write_image_data(rgb)
        .map_err(|e| e.to_string())?;
    Ok(RenderedFace {
        width: w,
        height: h,
        png,
        unknown,
        valid_until_ms: None,
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
    if kind == "icon" || (kind == "picture" && input.picture.is_some()) {
        if kind == "icon" {
            draw_icon(
                &mut rgb,
                dimensions,
                input.icon.as_deref().unwrap_or("app"),
                !text.is_empty(),
            );
        } else {
            draw_picture(
                &mut rgb,
                dimensions,
                input.picture.as_ref().unwrap(),
                !text.is_empty(),
            );
        }
        draw_label(&mut rgb, dimensions, text)?;
        return encode_rgb(&rgb, dimensions, false);
    }
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
    encode_rgb(&rgb, dimensions, unknown)
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
    fn picture_import_rejects_oversize_unknown_and_broken_images() {
        assert!(PictureAsset::decode(&[]).is_err());
        assert!(PictureAsset::decode(&vec![0; MAX_PICTURE_BYTES + 1]).is_err());
        assert!(PictureAsset::decode(b"<svg></svg>").is_err());
        let mut header = Vec::new();
        png::Encoder::new(&mut header, 4001, 4001)
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; 4001 * 4001])
            .unwrap();
        assert!(PictureAsset::decode(&header)
            .unwrap_err()
            .contains("16 million"));
        assert!(PictureAsset::decode(&[137, 80, 78, 71, 13, 10, 26, 10]).is_err());
    }
    #[test]
    fn png_jpeg_normalize_and_transparency_composites() {
        use image::{DynamicImage, ImageFormat, RgbImage, Rgba, RgbaImage};
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2048, 1024, Rgba([255, 0, 0, 128])))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let asset = PictureAsset::decode(bytes.get_ref()).unwrap();
        assert_eq!(asset.dimensions(), (1024, 512));
        assert!(asset.png().len() <= MAX_PICTURE_BYTES);
        let mut rgb = BG
            .into_iter()
            .cycle()
            .take((196 * 196 * 3) as usize)
            .collect::<Vec<_>>();
        draw_picture(&mut rgb, KEY_SIZE, &asset, false);
        let at = (98 * 196 + 98) * 3;
        assert!(rgb[at] > 120 && rgb[at] < 150);
        assert_eq!(&rgb[..3], &BG);
        let mut jpeg = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::new(12, 8))
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        assert_eq!(
            PictureAsset::decode(jpeg.get_ref()).unwrap().dimensions(),
            (12, 8)
        );
    }
    #[test]
    fn fixed_icons_have_different_pixels_at_both_geometries() {
        for wide in [false, true] {
            let mut seen = Vec::new();
            for name in ICON_NAMES {
                let frame = render_face(
                    "icon",
                    "",
                    wide,
                    &RenderInputs {
                        icon: Some((*name).into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(!frame.unknown);
                assert!(!seen.contains(&frame.png));
                seen.push(frame.png);
            }
        }
        assert!(render_face(
            "icon",
            "",
            false,
            &RenderInputs {
                icon: Some("untrusted-svg".into()),
                ..Default::default()
            }
        )
        .is_err());
    }
    #[test]
    fn calendar_and_clock_inputs_are_strict() {
        for s in ["2024-02-29", "2026-10-31"] {
            assert!(valid_date(s));
        }
        for s in [
            "2026-02-29",
            "2026-13-01",
            "2026-04-31",
            "0000-01-01",
            "99%",
        ] {
            assert!(!valid_date(s));
        }
        for s in ["00:00", "23:59:59"] {
            assert!(valid_clock(s));
        }
        for s in ["25:00", "23:99", ":::::", "1:20", "99%"] {
            assert!(!valid_clock(s));
        }
    }
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
