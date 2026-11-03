use std::{borrow::Cow, path::PathBuf};
use td_host::gif_normalize::{AnimLimits, Fit, Target, normalize_gif};

fn source(side: u16, colors: u8, seeds: &[u32]) -> Vec<u8> {
    let palette: Vec<u8> = (0..usize::from(colors))
        .flat_map(|i| [(i * 37) as u8, (i * 91) as u8, (i * 151) as u8])
        .collect();
    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, side, side, &palette).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for &seed in seeds {
            let mut x = seed.wrapping_mul(2654435761).max(1);
            let pixels = (0..usize::from(side) * usize::from(side))
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    (x % u32::from(colors)) as u8
                })
                .collect();
            encoder
                .write_frame(&gif::Frame {
                    width: side,
                    height: side,
                    delay: 10,
                    dispose: gif::DisposalMethod::Keep,
                    buffer: Cow::Owned(pixels),
                    ..Default::default()
                })
                .unwrap();
        }
    }
    bytes
}

fn main() {
    let out = PathBuf::from(std::env::args_os().nth(1).unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let limits = AnimLimits {
        image: (128, 128),
        min_delay_ms: 100,
        max_frames: 60,
        max_frame_pixels_total: 983040,
        max_bytes: 262144,
    };
    let mut manifest = String::new();
    for case in 0..32u16 {
        let (side, colors, seeds) = if case < 24 {
            (128, 255, vec![u32::from(case) + 1])
        } else {
            (
                32,
                [2, 3, 4, 5, 8, 16, 32, 64][usize::from(case - 24)],
                vec![7, 21, 94],
            )
        };
        let target = Target {
            width: if case < 24 { 128 - case } else { side },
            height: side,
            fit: if case % 2 == 0 {
                Fit::Contain
            } else {
                Fit::Cover
            },
        };
        let normalized = normalize_gif(&source(side, colors, &seeds), target, &limits).unwrap();
        let name = format!("normalizer-{case:02}.gif");
        std::fs::write(out.join(&name), &normalized.bytes).unwrap();
        manifest.push_str(&format!(
            "{name}\t{}\t{}\t{}\n",
            normalized.width,
            normalized.height,
            normalized
                .delays_ms
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    std::fs::write(out.join("manifest.tsv"), manifest).unwrap();
}
