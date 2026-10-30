use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits, codecs::jpeg::JpegEncoder,
    imageops::FilterType, metadata::Orientation,
};
use std::io::Cursor;

const QUALITIES: [u8; 5] = [85, 75, 65, 55, 45];
const SHRINKS: u32 = 4;
const DECODE_BYTES_MAX: u64 = 256 * 1024 * 1024;

pub(crate) fn prepare(file: &[u8], screen: [u32; 2], limit: usize) -> Result<Vec<u8>, String> {
    let mut reader = ImageReader::new(Cursor::new(file))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODE_BYTES_MAX);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    picture.apply_orientation(orientation);
    let [width, height] = screen;
    if picture.width() > width || picture.height() > height {
        picture = picture.resize(width, height, FilterType::Triangle);
    }
    for _ in 0..=SHRINKS {
        let rgb = picture.to_rgb8();
        for quality in QUALITIES {
            let mut out = Vec::new();
            JpegEncoder::new_with_quality(&mut out, quality)
                .encode_image(&rgb)
                .map_err(|e| e.to_string())?;
            if out.len() <= limit {
                return Ok(out);
            }
        }
        let (w, h) = (picture.width() * 3 / 4, picture.height() * 3 / 4);
        if w == 0 || h == 0 {
            break;
        }
        picture = picture.resize_exact(w, h, FilterType::Triangle);
    }
    Err(format!("the picture doesn't fit in {limit} bytes"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage};

    pub(crate) fn noise_png(width: u32, height: u32, seed: u32) -> Vec<u8> {
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        let picture = RgbImage::from_fn(width, height, |_, _| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let [r, g, b, _] = state.to_le_bytes();
            Rgb([r, g, b])
        });
        let mut out = Cursor::new(Vec::new());
        picture.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn size_of(jpeg: &[u8]) -> (u32, u32) {
        let picture = image::load_from_memory_with_format(jpeg, ImageFormat::Jpeg).unwrap();
        (picture.width(), picture.height())
    }

    #[test]
    fn a_large_picture_fits_inside_the_screen_and_keeps_its_shape() {
        let jpeg = prepare(&noise_png(800, 200, 1), [320, 200], 1 << 20).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]));
        assert_eq!(size_of(&jpeg), (320, 80));
    }

    #[test]
    fn a_small_picture_is_not_enlarged() {
        let jpeg = prepare(&noise_png(40, 30, 2), [320, 200], 1 << 20).unwrap();
        assert_eq!(size_of(&jpeg), (40, 30));
    }

    #[test]
    fn the_jpeg_never_exceeds_the_device_limit() {
        let file = noise_png(320, 200, 3);
        let unbounded = prepare(&file, [320, 200], usize::MAX).unwrap();
        let limit = unbounded.len() / 4;
        let bounded = prepare(&file, [320, 200], limit).unwrap();
        assert!(bounded.len() <= limit, "{} > {limit}", bounded.len());
        let (w, h) = size_of(&bounded);
        assert!(w <= 320 && h <= 200);
        assert!(prepare(&file, [320, 200], 200).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_picture_is_refused() {
        assert!(prepare(b"not a picture", [320, 200], 1 << 20).is_err());
        assert!(prepare(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0], [320, 200], 1 << 20).is_err());
    }
}
