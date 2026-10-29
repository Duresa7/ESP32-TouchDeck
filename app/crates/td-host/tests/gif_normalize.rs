use std::borrow::Cow;
use td_host::gif_normalize::{AnimLimits, Fit, GifError, SourceLimit, Target, normalize_gif};

fn limits() -> AnimLimits {
    AnimLimits {
        image: (128, 128),
        min_delay_ms: 100,
        max_frames: 60,
        max_frame_pixels_total: 983040,
        max_bytes: 262144,
    }
}

fn target() -> Target {
    Target {
        width: 4,
        height: 4,
        fit: Fit::Contain,
    }
}

fn source(width: u16, height: u16, frames: &[gif::Frame<'_>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(
            &mut bytes,
            width,
            height,
            &[0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255],
        )
        .unwrap();
        for frame in frames {
            encoder.write_frame(frame).unwrap();
        }
    }
    bytes
}

fn frame(width: u16, height: u16, color: u8, delay: u16) -> gif::Frame<'static> {
    gif::Frame {
        width,
        height,
        delay,
        dispose: gif::DisposalMethod::Keep,
        buffer: Cow::Owned(vec![color; usize::from(width) * usize::from(height)]),
        ..Default::default()
    }
}

#[test]
fn normalizes_time_and_wire_contract() {
    let input = source(
        4,
        4,
        &(0..40)
            .map(|i| frame(4, 4, 1 + i % 3, 3))
            .collect::<Vec<_>>(),
    );
    let output = normalize_gif(&input, target(), &limits()).unwrap();
    assert_eq!(output.delays_ms.iter().sum::<u32>(), 1200);
    assert!(output.delays_ms.iter().all(|d| *d >= 100));
    assert_eq!(&output.bytes[..6], b"GIF89a");
    let mut decoder = gif::DecodeOptions::new()
        .read_info(output.bytes.as_slice())
        .unwrap();
    assert!(decoder.global_palette().is_some());
    let mut count = 0;
    while let Some(f) = decoder.read_next_frame().unwrap() {
        assert_eq!((f.left, f.top, f.width, f.height), (0, 0, 4, 4));
        assert_eq!(f.dispose, gif::DisposalMethod::Keep);
        assert!(!f.interlaced);
        assert!(f.palette.is_none());
        count += 1;
    }
    assert_eq!(count, output.frames);
    assert_eq!(decoder.repeat(), gif::Repeat::Infinite);
}

#[test]
fn preserves_long_pauses_and_merges_a_short_tail() {
    let input = source(
        4,
        4,
        &[frame(4, 4, 1, 10), frame(4, 4, 2, 90), frame(4, 4, 3, 3)],
    );
    let output = normalize_gif(&input, target(), &limits()).unwrap();
    assert_eq!(output.delays_ms, [100, 930]);
}

fn pixels(bytes: &[u8]) -> Vec<image::RgbaImage> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).unwrap();
    let mut images = Vec::new();
    while let Some(f) = decoder.read_next_frame().unwrap() {
        images.push(
            image::RgbaImage::from_raw(f.width.into(), f.height.into(), f.buffer.to_vec()).unwrap(),
        );
    }
    images
}

#[test]
fn composes_disposal_transparency_local_palettes_and_offsets() {
    for disposal in [
        gif::DisposalMethod::Any,
        gif::DisposalMethod::Keep,
        gif::DisposalMethod::Background,
        gif::DisposalMethod::Previous,
    ] {
        let mut patch = frame(2, 2, 1, 10);
        patch.left = 1;
        patch.top = 1;
        patch.dispose = disposal;
        patch.palette = Some(vec![0, 0, 0, 0, 255, 0]);
        patch.transparent = Some(0);
        patch.buffer = Cow::Owned(vec![1, 0, 0, 1]);
        let mut last = frame(1, 1, 3, 10);
        last.left = 3;
        let input = source(4, 4, &[frame(4, 4, 1, 10), patch, last]);
        let result = normalize_gif(&input, target(), &limits()).unwrap();
        let images = pixels(&result.bytes);
        assert_eq!(images[1].get_pixel(1, 1).0, [0, 255, 0, 255]);
        assert_eq!(images[1].get_pixel(2, 1).0, [255, 0, 0, 255]);
        let expected = match disposal {
            gif::DisposalMethod::Background => [0, 0, 0, 0],
            gif::DisposalMethod::Previous => [255, 0, 0, 255],
            _ => [0, 255, 0, 255],
        };
        assert_eq!(images[2].get_pixel(1, 1).0, expected);
        assert_eq!(images[2].get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(images[2].get_pixel(3, 0).0, [0, 0, 255, 255]);
    }
}

#[test]
fn deinterlaces_four_pass_rows() {
    let mut f = frame(4, 8, 0, 10);
    f.interlaced = true;
    f.buffer = Cow::Owned(
        [1, 2, 3, 1, 2, 3, 1, 2]
            .into_iter()
            .flat_map(|i| [i; 4])
            .collect(),
    );
    let input = source(4, 8, &[f]);
    let output = normalize_gif(
        &input,
        Target {
            width: 4,
            height: 8,
            fit: Fit::Contain,
        },
        &limits(),
    )
    .unwrap();
    let image = &pixels(&output.bytes)[0];
    for (y, color) in [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [255, 0, 0, 255],
        [0, 255, 0, 255],
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(&image.get_pixel(0, y as u32).0, color);
    }
}

#[test]
fn fitting_matches_static_contain_and_centered_cover() {
    let input = source(8, 4, &[frame(8, 4, 1, 10)]);
    let mut t = Target {
        width: 64,
        height: 64,
        fit: Fit::Contain,
    };
    let output = normalize_gif(&input, t, &limits()).unwrap();
    let expected = td_host::icon_pack::fit(&image::RgbaImage::from_pixel(
        8,
        4,
        image::Rgba([255, 0, 0, 255]),
    ));
    assert_eq!(pixels(&output.bytes)[0], expected);
    t.fit = Fit::Cover;
    let output = normalize_gif(&input, t, &limits()).unwrap();
    assert!(
        pixels(&output.bytes)[0]
            .pixels()
            .all(|p| p.0 == [255, 0, 0, 255])
    );
    let mut striped = frame(8, 4, 1, 10);
    for y in 0..4 {
        for x in 2..6 {
            striped.buffer.to_mut()[y * 8 + x] = 2;
        }
    }
    let output = normalize_gif(
        &source(8, 4, &[striped]),
        Target {
            width: 4,
            height: 4,
            fit: Fit::Cover,
        },
        &limits(),
    )
    .unwrap();
    assert!(
        pixels(&output.bytes)[0]
            .pixels()
            .all(|p| p.0 == [0, 255, 0, 255])
    );
}

#[test]
fn zero_and_tiny_delays_follow_browser_timing_without_rounding_drift() {
    let input = source(
        4,
        4,
        &[frame(4, 4, 1, 0), frame(4, 4, 2, 1), frame(4, 4, 3, 2)],
    );
    let output = normalize_gif(&input, target(), &limits()).unwrap();
    assert_eq!(output.delays_ms, [100, 120]);
    let mut caps = limits();
    caps.min_delay_ms = 101;
    let output = normalize_gif(&input, target(), &caps).unwrap();
    assert_eq!(output.delays_ms, [220]);
    assert!(matches!(
        normalize_gif(&source(4, 4, &[frame(4, 4, 1, 2)]), target(), &caps),
        Err(GifError::TooShort {
            duration_ms: 20,
            ..
        })
    ));
}

#[test]
fn refuses_long_loops_instead_of_truncating() {
    let input = source(4, 4, &vec![frame(4, 4, 1, 10); 61]);
    assert_eq!(
        normalize_gif(&input, target(), &limits()).unwrap_err(),
        GifError::TooLong {
            max_frames: 60,
            needed: 61
        }
    );
}

#[test]
fn shrinks_for_byte_and_pixel_budgets_and_reports_the_smallest_attempt() {
    let input = source(128, 128, &[frame(128, 128, 1, 10)]);
    let t = Target {
        width: 128,
        height: 128,
        fit: Fit::Contain,
    };
    let mut caps = limits();
    caps.max_bytes = 80;
    let output = normalize_gif(&input, t, &caps).unwrap();
    assert!(output.width < 128 && output.bytes.len() <= 80);
    caps.max_bytes = 1;
    assert!(
        matches!(normalize_gif(&input,t,&caps), Err(GifError::TooLarge { max_bytes: 1, smallest }) if smallest > 1 && smallest <= 80)
    );
    caps = limits();
    caps.max_frame_pixels_total = 100;
    let output = normalize_gif(&input, t, &caps).unwrap();
    assert!(usize::from(output.width) * usize::from(output.height) <= 100);
    caps.max_frame_pixels_total = 0;
    assert_eq!(
        normalize_gif(&input, t, &caps).unwrap_err(),
        GifError::PixelLimit {
            limit: 0,
            smallest: 1
        }
    );
}

#[test]
fn rejects_each_source_limit_before_decoding_the_offending_frame() {
    let mut input = vec![0; (16 << 20) + 1];
    input[..6].copy_from_slice(b"GIF89a");
    assert!(matches!(
        normalize_gif(&input, target(), &limits()),
        Err(GifError::SourceTooLarge {
            kind: SourceLimit::Bytes,
            ..
        })
    ));
    for (offset, kind) in [(6, SourceLimit::Width), (8, SourceLimit::Height)] {
        let mut input = source(1, 1, &[frame(1, 1, 1, 10)]);
        input[offset..offset + 2].copy_from_slice(&4097u16.to_le_bytes());
        assert!(
            matches!(normalize_gif(&input,target(),&limits()), Err(GifError::SourceTooLarge { kind: actual, limit: 4096, actual: 4097 }) if actual == kind)
        );
    }
    let input = source(1, 1, &vec![frame(1, 1, 1, 10); 301]);
    assert!(matches!(
        normalize_gif(&input, target(), &limits()),
        Err(GifError::SourceTooLarge {
            kind: SourceLimit::Frames,
            actual: 301,
            ..
        })
    ));
    let mut input = source(4096, 4096, &[frame(1, 1, 1, 10)]);
    input.pop();
    input.extend_from_slice(&[0x2c, 0, 0, 0, 0, 0, 16, 0, 16, 0, 2, 1, 0]);
    assert_eq!(
        normalize_gif(&input, target(), &limits()).unwrap_err(),
        GifError::SourceTooLarge {
            kind: SourceLimit::DecodedRgba,
            limit: 64 << 20,
            actual: 128 << 20
        }
    );
}

#[test]
fn typed_errors_for_non_gif_corrupt_and_invalid_targets() {
    assert_eq!(
        normalize_gif(b"png", target(), &limits()).unwrap_err(),
        GifError::NotGif
    );
    assert_eq!(
        normalize_gif(b"GIF89a", target(), &limits()).unwrap_err(),
        GifError::Corrupt
    );
    let input = source(4, 4, &[frame(4, 4, 1, 10)]);
    assert_eq!(
        normalize_gif(&input[..input.len() - 3], target(), &limits()).unwrap_err(),
        GifError::Corrupt
    );
    let mut t = target();
    t.width = 0;
    assert_eq!(
        normalize_gif(&input, t, &limits()).unwrap_err(),
        GifError::InvalidLimits
    );
}

#[test]
fn deterministic_global_palette_across_more_than_256_colors() {
    use sha2::{Digest, Sha256};
    let mut frames = Vec::new();
    for offset in [0, 61, 127] {
        let mut f = frame(16, 16, 0, 10);
        f.palette = Some(
            (0..256)
                .flat_map(|i| [i as u8, (i as u8).wrapping_add(offset), offset])
                .collect(),
        );
        f.buffer = Cow::Owned((0..256).map(|i| i as u8).collect());
        f.transparent = Some(0);
        frames.push(f);
    }
    let input = source(16, 16, &frames);
    let t = Target {
        width: 16,
        height: 16,
        fit: Fit::Contain,
    };
    let a = normalize_gif(&input, t, &limits()).unwrap();
    let b = normalize_gif(&input, t, &limits()).unwrap();
    assert_eq!(Sha256::digest(&a.bytes), Sha256::digest(&b.bytes));
    let mut decoder = gif::DecodeOptions::new()
        .read_info(a.bytes.as_slice())
        .unwrap();
    assert!(decoder.global_palette().unwrap().len() <= 768);
    while let Some(f) = decoder.read_next_frame().unwrap() {
        assert_eq!(f.transparent, Some(0));
        assert!(f.palette.is_none());
    }
    assert_eq!(pixels(&a.bytes)[0].get_pixel(0, 0)[3], 0);
}

#[test]
#[ignore = "manual performance evidence; no CI timing threshold"]
fn benchmark_large_source() {
    for (side, count) in [(1024, 300), (128, 300)] {
        let input = source(side, side, &vec![frame(side, side, 1, 3); count]);
        let start = std::time::Instant::now();
        let output = normalize_gif(
            &input,
            Target {
                width: 128,
                height: 128,
                fit: Fit::Contain,
            },
            &AnimLimits {
                max_frames: 300,
                ..limits()
            },
        );
        println!(
            "{count} frames {side}x{side}: {:?}; {:?}",
            start.elapsed(),
            output
                .as_ref()
                .map(|o| (o.width, o.height, o.frames, o.bytes.len()))
        );
        if side == 1024 {
            assert!(matches!(
                output,
                Err(GifError::SourceTooLarge {
                    kind: SourceLimit::DecodedRgba,
                    actual: 71303168,
                    ..
                })
            ));
        } else {
            assert!(output.is_ok());
        }
    }
}

#[test]
fn handles_transparent_only_and_extreme_delays() {
    let mut transparent = frame(4, 4, 0, 10);
    transparent.transparent = Some(0);
    let output = normalize_gif(&source(4, 4, &[transparent]), target(), &limits()).unwrap();
    assert!(pixels(&output.bytes)[0].pixels().all(|p| p[3] == 0));
    let input = source(4, 4, &[frame(4, 4, 1, u16::MAX), frame(4, 4, 2, 2)]);
    let output = normalize_gif(&input, target(), &limits()).unwrap();
    assert_eq!(output.delays_ms.iter().sum::<u32>(), 655370);
    assert!(output.delays_ms.iter().all(|d| *d >= 100 && *d <= 655350));
}

#[test]
fn caps_canvas_at_both_device_and_tile_and_rejects_invalid_rectangles() {
    let input = source(4, 4, &[frame(4, 4, 1, 10)]);
    let output = normalize_gif(
        &input,
        Target {
            width: 200,
            height: 40,
            fit: Fit::Contain,
        },
        &limits(),
    )
    .unwrap();
    assert_eq!((output.width, output.height), (128, 40));
    let mut patch = frame(2, 2, 1, 10);
    patch.left = 3;
    assert_eq!(
        normalize_gif(&source(4, 4, &[patch]), target(), &limits()).unwrap_err(),
        GifError::Corrupt
    );
    let mut input = source(4, 4, &[frame(4, 4, 1, 10)]);
    let descriptor = input.iter().position(|b| *b == 0x2c).unwrap();
    input[descriptor + 5..descriptor + 7].copy_from_slice(&4097u16.to_le_bytes());
    assert!(matches!(
        normalize_gif(&input, target(), &limits()),
        Err(GifError::SourceTooLarge {
            kind: SourceLimit::Width,
            actual: 4097,
            ..
        })
    ));
}

#[test]
fn many_fitted_colors_use_a_deterministic_bounded_palette() {
    let mut frames = Vec::new();
    for offset in 0..20 {
        let mut f = frame(16, 16, 0, 10);
        f.palette = Some(
            (0..256)
                .flat_map(|i| [i as u8, offset * 12, (255 - i) as u8])
                .collect(),
        );
        f.buffer = Cow::Owned((0..256).map(|i| i as u8).collect());
        frames.push(f);
    }
    let input = source(16, 16, &frames);
    let t = Target {
        width: 16,
        height: 16,
        fit: Fit::Contain,
    };
    let a = normalize_gif(&input, t, &limits()).unwrap();
    let b = normalize_gif(&input, t, &limits()).unwrap();
    assert_eq!(a.bytes, b.bytes);
    let decoded = pixels(&a.bytes);
    assert_eq!(decoded.len(), 20);
    assert!(decoded.iter().flat_map(|f| f.pixels()).all(|p| p[3] == 255));
}
