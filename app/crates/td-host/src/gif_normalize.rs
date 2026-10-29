use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use gif::{ColorOutput, DecodeOptions, DisposalMethod, Encoder, Frame, Repeat};
use image::{
    Rgba, RgbaImage,
    imageops::{self, FilterType},
};

use crate::icon_pack;

const SOURCE_BYTES: usize = 16 << 20;
const SOURCE_SIDE: usize = 4096;
const SOURCE_FRAMES: usize = 300;
const SOURCE_RGBA: usize = 64 << 20;
const MAX_DELAY_CS: u32 = u16::MAX as u32;
const MIN_SIDE: u16 = 32;
const MATTE: [u8; 3] = [0, 0, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub width: u16,
    pub height: u16,
    pub fit: Fit,
}

#[derive(Clone, Debug)]
pub struct AnimLimits {
    pub image: (u16, u16),
    pub min_delay_ms: u32,
    pub max_frames: usize,
    pub max_frame_pixels_total: usize,
    pub max_bytes: usize,
}

#[derive(Debug)]
pub struct NormalizedGif {
    pub bytes: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub frames: usize,
    pub delays_ms: Vec<u32>,
    pub fps_shown: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceLimit {
    Bytes,
    Width,
    Height,
    Frames,
    DecodedRgba,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GifError {
    NotGif,
    Corrupt,
    SourceTooLarge {
        kind: SourceLimit,
        limit: usize,
        actual: usize,
    },
    TooLong {
        max_frames: usize,
        needed: usize,
    },
    TooLarge {
        max_bytes: usize,
        smallest: usize,
    },
    PixelLimit {
        limit: usize,
        smallest: usize,
    },
    TooShort {
        min_delay_ms: u32,
        duration_ms: u32,
    },
    InvalidLimits,
}

impl fmt::Display for GifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GIF normalization: {self:?}")
    }
}
impl std::error::Error for GifError {}

struct Composed {
    pixels: RgbaImage,
    delay_cs: u32,
}

/// Fits a GIF to a Tile and Device budget without truncating its loop.
/// Source delays below 20 ms become 100 ms, matching browser playback.
/// Contain centres on a transparent canvas; cover crops the centre. Alpha is
/// thresholded at 128 for GIF's binary transparency. Background disposal clears
/// to transparent, as in browser image playback. Output frames keep the previous
/// frame (disposal 1), so only pixels clear in every frame stay transparent; a
/// pixel clear in some frames is drawn black there. A multi-frame loop shorter
/// than the minimum delay is refused rather than lengthened; a single frame just
/// gets the minimum delay. The canvas shrinks by 7/8 steps while its longer side
/// stays at least 32 px. All returned delays are exact 10 ms units.
pub fn normalize_gif(
    bytes: &[u8],
    target: Target,
    limits: &AnimLimits,
) -> Result<NormalizedGif, GifError> {
    if target.width == 0
        || target.height == 0
        || limits.image.0 == 0
        || limits.image.1 == 0
        || limits.min_delay_ms == 0
        || limits.min_delay_ms > MAX_DELAY_CS * 10
    {
        return Err(GifError::InvalidLimits);
    }
    let source = decode(bytes)?;
    let timeline = resample(&source, limits)?;
    let mut width = target.width.min(limits.image.0).min(SOURCE_SIDE as u16);
    let mut height = target.height.min(limits.image.1).min(SOURCE_SIDE as u16);
    let mut smallest = usize::MAX;
    loop {
        let pixels = usize::from(width) * usize::from(height) * timeline.len();
        if pixels <= limits.max_frame_pixels_total && pixels <= SOURCE_RGBA / 4 {
            let fitted: Vec<_> = timeline
                .iter()
                .map(|&(i, _)| fit(&source[i].pixels, width, height, target.fit))
                .collect();
            let bytes = encode(&fitted, &timeline, width, height)?;
            smallest = smallest.min(bytes.len());
            if bytes.len() <= limits.max_bytes {
                let delays_ms: Vec<_> = timeline.iter().map(|&(_, cs)| cs * 10).collect();
                let duration: u32 = delays_ms.iter().sum();
                return Ok(NormalizedGif {
                    bytes,
                    width,
                    height,
                    frames: timeline.len(),
                    fps_shown: timeline.len() as f64 * 1000.0 / f64::from(duration),
                    delays_ms,
                });
            }
        }
        let (next_w, next_h) = (
            (u32::from(width) * 7 / 8).max(1) as u16,
            (u32::from(height) * 7 / 8).max(1) as u16,
        );
        if next_w.max(next_h) < MIN_SIDE {
            return if smallest == usize::MAX {
                Err(GifError::PixelLimit {
                    limit: limits.max_frame_pixels_total,
                    smallest: pixels,
                })
            } else {
                Err(GifError::TooLarge {
                    max_bytes: limits.max_bytes,
                    smallest,
                })
            };
        }
        (width, height) = (next_w, next_h);
    }
}

fn bound(kind: SourceLimit, actual: usize, limit: usize) -> Result<(), GifError> {
    if actual > limit {
        Err(GifError::SourceTooLarge {
            kind,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}

fn decode(bytes: &[u8]) -> Result<Vec<Composed>, GifError> {
    bound(SourceLimit::Bytes, bytes.len(), SOURCE_BYTES)?;
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return Err(GifError::NotGif);
    }
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::RGBA);
    options.check_lzw_end_code(true);
    let mut decoder = options.read_info(bytes).map_err(|_| GifError::Corrupt)?;
    let (w, h) = (usize::from(decoder.width()), usize::from(decoder.height()));
    bound(SourceLimit::Width, w, SOURCE_SIDE)?;
    bound(SourceLimit::Height, h, SOURCE_SIDE)?;
    if w == 0 || h == 0 {
        return Err(GifError::Corrupt);
    }
    let mut canvas = RgbaImage::new(w as u32, h as u32);
    let mut frames = Vec::new();
    while let Some(info) = decoder.next_frame_info().map_err(|_| GifError::Corrupt)? {
        bound(SourceLimit::Frames, frames.len() + 1, SOURCE_FRAMES)?;
        bound(SourceLimit::Width, usize::from(info.width), SOURCE_SIDE)?;
        bound(SourceLimit::Height, usize::from(info.height), SOURCE_SIDE)?;
        bound(
            SourceLimit::DecodedRgba,
            (frames.len() + 1) * w * h * 4,
            SOURCE_RGBA,
        )?;
        let (left, top, fw, fh, dispose, delay) = (
            u32::from(info.left),
            u32::from(info.top),
            u32::from(info.width),
            u32::from(info.height),
            info.dispose,
            info.delay,
        );
        if fw == 0 || fh == 0 || left + fw > w as u32 || top + fh > h as u32 {
            return Err(GifError::Corrupt);
        }
        let previous = (dispose == DisposalMethod::Previous).then(|| canvas.clone());
        let mut rgba = vec![0; decoder.buffer_size()];
        decoder
            .read_into_buffer(&mut rgba)
            .map_err(|_| GifError::Corrupt)?;
        for (i, p) in rgba.as_chunks::<4>().0.iter().enumerate() {
            if p[3] != 0 {
                canvas.put_pixel(
                    left + i as u32 % fw,
                    top + i as u32 / fw,
                    Rgba([p[0], p[1], p[2], p[3]]),
                );
            }
        }
        frames.push(Composed {
            pixels: canvas.clone(),
            delay_cs: if delay < 2 { 10 } else { u32::from(delay) },
        });
        match dispose {
            DisposalMethod::Background => {
                for y in top..top + fh {
                    for x in left..left + fw {
                        canvas.put_pixel(x, y, Rgba([0; 4]));
                    }
                }
            }
            DisposalMethod::Previous => canvas = previous.ok_or(GifError::Corrupt)?,
            _ => {}
        }
    }
    if frames.is_empty() {
        return Err(GifError::Corrupt);
    }
    Ok(frames)
}

fn resample(source: &[Composed], limits: &AnimLimits) -> Result<Vec<(usize, u32)>, GifError> {
    let min_cs = limits.min_delay_ms.div_ceil(10);
    let total: u32 = source.iter().map(|f| f.delay_cs).sum();
    if source.len() == 1 {
        return Ok(vec![(0, total.max(min_cs))]);
    }
    if total < min_cs {
        return Err(GifError::TooShort {
            min_delay_ms: limits.min_delay_ms,
            duration_ms: total * 10,
        });
    }
    let mut groups = Vec::new();
    let (mut first, mut elapsed) = (0, 0);
    for (index, frame) in source.iter().enumerate() {
        elapsed += frame.delay_cs;
        if elapsed >= min_cs {
            groups.push((first, elapsed));
            first = index + 1;
            elapsed = 0;
        }
    }
    if elapsed != 0 {
        groups.last_mut().ok_or(GifError::Corrupt)?.1 += elapsed;
    }
    let mut timeline = Vec::new();
    for (index, duration) in groups {
        let parts = duration.div_ceil(MAX_DELAY_CS);
        for part in 0..parts {
            let delay = duration * (part + 1) / parts - duration * part / parts;
            if delay < min_cs {
                return Err(GifError::InvalidLimits);
            }
            timeline.push((index, delay));
        }
    }
    if timeline.len() > limits.max_frames {
        return Err(GifError::TooLong {
            max_frames: limits.max_frames,
            needed: timeline.len(),
        });
    }
    Ok(timeline)
}

fn fit(source: &RgbaImage, w: u16, h: u16, mode: Fit) -> RgbaImage {
    let (w, h) = (u32::from(w), u32::from(h));
    if mode == Fit::Contain {
        return icon_pack::fit_tile(source, w, h, 0, None, None);
    }
    let scale =
        (f64::from(w) / f64::from(source.width())).max(f64::from(h) / f64::from(source.height()));
    let crop_w = (f64::from(w) / scale).round().max(1.0) as u32;
    let crop_h = (f64::from(h) / scale).round().max(1.0) as u32;
    let cropped = imageops::crop_imm(
        source,
        (source.width() - crop_w) / 2,
        (source.height() - crop_h) / 2,
        crop_w,
        crop_h,
    )
    .to_image();
    imageops::resize(&cropped, w, h, FilterType::Lanczos3)
}

fn encode(
    images: &[RgbaImage],
    timeline: &[(usize, u32)],
    w: u16,
    h: u16,
) -> Result<Vec<u8>, GifError> {
    let mut clear = vec![true; usize::from(w) * usize::from(h)];
    for image in images {
        for (clear, p) in clear.iter_mut().zip(image.pixels()) {
            *clear &= p[3] < 128;
        }
    }
    let transparent = clear.contains(&true);
    let opaque = || images.iter().flat_map(|i| shown(i, &clear)).flatten();
    let mut unique = BTreeSet::new();
    let coarse = opaque().any(|rgb| {
        unique.insert(rgb);
        unique.len() > 4096
    });
    let key = |rgb: [u8; 3]| {
        if coarse {
            rgb.map(|c| (c & 0xf8) | 4)
        } else {
            rgb
        }
    };
    let mut colors = BTreeMap::<[u8; 3], usize>::new();
    for rgb in opaque() {
        *colors.entry(key(rgb)).or_default() += 1;
    }
    let mut boxes = vec![colors.into_iter().collect::<Vec<_>>()];
    while boxes.len() < 256 - usize::from(transparent) {
        let Some(index) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(i, b)| {
                (
                    b.iter().map(|(_, n)| n).sum::<usize>(),
                    std::cmp::Reverse(*i),
                )
            })
            .map(|(i, _)| i)
        else {
            break;
        };
        let b = &mut boxes[index];
        let axis = (0..3)
            .max_by_key(|&a| {
                b.iter().map(|(c, _)| c[a]).max().unwrap()
                    - b.iter().map(|(c, _)| c[a]).min().unwrap()
            })
            .unwrap();
        b.sort_by_key(|(c, _)| (c[axis], *c));
        let half = b.iter().map(|(_, n)| n).sum::<usize>() / 2;
        let mut weight = 0;
        let split = b
            .iter()
            .position(|(_, n)| {
                weight += n;
                weight >= half
            })
            .unwrap_or(0)
            + 1;
        let other = b.split_off(split.min(b.len() - 1));
        boxes.push(other);
    }
    let mut palette = Vec::new();
    let mut indices = BTreeMap::new();
    if transparent {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    for b in boxes.iter().filter(|b| !b.is_empty()) {
        let index = (palette.len() / 3) as u8;
        let weight: usize = b.iter().map(|(_, n)| n).sum();
        for axis in 0..3 {
            palette.push(
                (b.iter()
                    .map(|(c, n)| usize::from(c[axis]) * n)
                    .sum::<usize>()
                    / weight) as u8,
            );
        }
        for (color, _) in b {
            indices.insert(*color, index);
        }
    }
    if palette.is_empty() {
        palette.extend_from_slice(&[0, 0, 0]);
    }
    let mut bytes = Vec::new();
    {
        let mut encoder =
            Encoder::new(&mut bytes, w, h, &palette).map_err(|_| GifError::Corrupt)?;
        encoder
            .set_repeat(Repeat::Infinite)
            .map_err(|_| GifError::Corrupt)?;
        for (image, &(_, delay)) in images.iter().zip(timeline) {
            let buffer: Vec<_> = shown(image, &clear)
                .map(|rgb| rgb.map_or(0, |rgb| indices[&key(rgb)]))
                .collect();
            encoder
                .write_frame(&Frame {
                    width: w,
                    height: h,
                    delay: delay as u16,
                    dispose: DisposalMethod::Keep,
                    transparent: transparent.then_some(0),
                    buffer: Cow::Owned(buffer),
                    ..Frame::default()
                })
                .map_err(|_| GifError::Corrupt)?;
        }
    }
    Ok(bytes)
}

fn shown<'a>(
    image: &'a RgbaImage,
    clear: &'a [bool],
) -> impl Iterator<Item = Option<[u8; 3]>> + 'a {
    image.pixels().zip(clear).map(|(p, &clear)| match () {
        _ if clear => None,
        _ if p[3] < 128 => Some(MATTE),
        _ => Some([p[0], p[1], p[2]]),
    })
}
