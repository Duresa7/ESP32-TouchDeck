//! Packs an RGBA bitmap into the deck's `TDI1` icon file, per `schema/icon-format.md`: a
//! 12-byte header, 64 by 64 RGB565 pixels, then 64 by 64 A8 values. Pure, so the tests can
//! hold it to the golden file byte for byte.

use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use resvg::{tiny_skia, usvg};
use sha2::{Digest, Sha256};

pub const SIDE: u32 = 64;
pub const HEADER_BYTES: usize = 12;
pub const COLOR_BYTES: usize = (SIDE * SIDE * 2) as usize;
pub const ALPHA_BYTES: usize = (SIDE * SIDE) as usize;
pub const FILE_BYTES: usize = HEADER_BYTES + COLOR_BYTES + ALPHA_BYTES;
const MAGIC: &[u8; 4] = b"TDI1";
const FORMAT_RGB565A8: u16 = 1;

/// Packs a 64 by 64 image. Each channel is truncated to its high bits and alpha is copied.
pub fn pack(image: &RgbaImage) -> Vec<u8> {
    assert_eq!(
        image.dimensions(),
        (SIDE, SIDE),
        "pack takes a 64 by 64 image"
    );
    let mut file = Vec::with_capacity(FILE_BYTES);
    file.extend_from_slice(MAGIC);
    for value in [SIDE as u16, SIDE as u16, FORMAT_RGB565A8, 0] {
        file.extend_from_slice(&value.to_le_bytes());
    }
    for Rgba([r, g, b, _]) in image.pixels() {
        let rgb565 = (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3);
        file.extend_from_slice(&rgb565.to_le_bytes());
    }
    file.extend(image.pixels().map(|Rgba([_, _, _, a])| *a));
    file
}

/// Scales an image of any size to fit 64 by 64, keeping its aspect, and centres it on a
/// transparent square.
pub fn fit(image: &RgbaImage) -> RgbaImage {
    if image.dimensions() == (SIDE, SIDE) {
        return image.clone();
    }
    fit_tile(image, SIDE, SIDE, 0, None, None)
}

/// Fits and packs an image of any size.
pub fn pack_any(image: &RgbaImage) -> Vec<u8> {
    pack(&fit(image))
}

/// The icon's id: the first 16 hex digits of the file's SHA-256.
pub fn icon_id(file: &[u8]) -> String {
    Sha256::digest(file)[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether `file` is a `TDI1` icon the deck draws.
pub fn is_icon(file: &[u8]) -> bool {
    file.len() == FILE_BYTES && file.starts_with(MAGIC) && file[4..12] == [64, 0, 64, 0, 1, 0, 0, 0]
}

/// Renders an SVG into a `side` by `side` image, scaled to fit and centred.
pub fn render_svg(svg: &[u8], side: u32) -> Result<RgbaImage, String> {
    render_svg_size(svg, side, side)
}

/// Renders SVG directly into a rectangular canvas without losing its aspect ratio.
pub fn render_svg_size(svg: &[u8], width: u32, height: u32) -> Result<RgbaImage, String> {
    render_svg_fit(svg, width, height, false)
}

/// Renders directly to the canvas, optionally cropping to cover it.
pub fn render_svg_fit(
    svg: &[u8],
    width: u32,
    height: u32,
    cover: bool,
) -> Result<RgbaImage, String> {
    let tree = parse_svg(svg)?;
    let size = tree.size();
    let sx = width as f32 / size.width();
    let sy = height as f32 / size.height();
    let scale = if cover { sx.max(sy) } else { sx.min(sy) };
    let dx = (width as f32 - size.width() * scale) / 2.0;
    let dy = (height as f32 - size.height() * scale) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("svg: empty canvas")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    let mut image = RgbaImage::new(width, height);
    for (pixel, out) in pixmap.pixels().iter().zip(image.pixels_mut()) {
        let color = pixel.demultiply();
        *out = Rgba([color.red(), color.green(), color.blue(), color.alpha()]);
    }
    Ok(image)
}

/// The ink monochrome icons draw with: the stroke colour of the built-in Lucide icons. An SVG
/// that paints with `currentColor` gets it through its root `color` attribute.
pub const INK: &str = "#f2f5fa";

/// Parses an SVG that may come from anywhere. Nothing outside the document is read: an
/// `<image>` that names a file or URL draws nothing, and only `data:` images inside it render.
pub fn parse_svg(svg: &[u8]) -> Result<usvg::Tree, String> {
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    usvg::Tree::from_data(svg, &options).map_err(|error| format!("svg: {error}"))
}

/// Renders an uploaded SVG so its longer side is `max` pixels, keeping its aspect. A root
/// without a `color` draws `currentColor` in [`INK`], the way a pack icon does.
pub fn render_svg_upload(svg: &[u8], max: u32) -> Result<RgbaImage, String> {
    let text = std::str::from_utf8(svg).map_err(|_| "svg: not UTF-8".to_owned())?;
    let inked = with_root_color(text);
    let tree = parse_svg(inked.as_bytes())?;
    let size = tree.size();
    let scale = max as f32 / size.width().max(size.height());
    let width = ((size.width() * scale).round() as u32).clamp(1, max);
    let height = ((size.height() * scale).round() as u32).clamp(1, max);
    render_svg_fit(inked.as_bytes(), width, height, false)
}

/// Adds `color` to the root `<svg>` tag when it sets none.
fn with_root_color(svg: &str) -> String {
    let Some(start) = svg.find("<svg") else {
        return svg.to_owned();
    };
    let Some(end) = svg[start..].find('>').map(|e| start + e) else {
        return svg.to_owned();
    };
    let tag = &svg[start..end];
    if tag
        .split_ascii_whitespace()
        .any(|attr| attr.starts_with("color="))
    {
        return svg.to_owned();
    }
    format!("{} color=\"{INK}\"{}", &svg[..start + 4], &svg[start + 4..])
}

/// Contains an image inside a padded Tile, then applies tint and composites its background.
/// Callers validate nonzero dimensions and padding before allocating.
pub fn fit_tile(
    image: &RgbaImage,
    width: u32,
    height: u32,
    padding: u32,
    background: Option<[u8; 4]>,
    tint: Option<[u8; 3]>,
) -> RgbaImage {
    let available_w = width - padding * 2;
    let available_h = height - padding * 2;
    let scale =
        (available_w as f64 / image.width() as f64).min(available_h as f64 / image.height() as f64);
    let w = ((image.width() as f64 * scale).round() as u32).clamp(1, available_w);
    let h = ((image.height() as f64 * scale).round() as u32).clamp(1, available_h);
    let mut scaled = if image.dimensions() == (w, h) {
        image.clone()
    } else {
        imageops::resize(image, w, h, FilterType::Lanczos3)
    };
    if let Some([r, g, b]) = tint {
        for pixel in scaled.pixels_mut() {
            pixel.0[..3].copy_from_slice(&[r, g, b]);
        }
    }
    let mut tile = RgbaImage::from_pixel(width, height, Rgba(background.unwrap_or([0; 4])));
    imageops::overlay(
        &mut tile,
        &scaled,
        i64::from((width - w) / 2),
        i64::from((height - h) / 2),
    );
    tile
}
