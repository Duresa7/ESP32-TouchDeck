//! Cover art for `image` live values: the source picture a Control holds, turned into a square baseline JPEG Asset
//! sized to the Tile's art box, and the few cover Assets each Device keeps.

use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits, codecs::jpeg::JpegEncoder,
    imageops::FilterType, metadata::Orientation,
};
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Arc;
use td_link::Session;
use td_link::asset::{self, AssetError, AssetFormat, AssetReason};

const SOURCE_BYTES_MAX: usize = 4 * 1024 * 1024;
const SOURCE_SIDE_MAX: u32 = 4096;
const DECODED_BYTES_MAX: u64 = 64 * 1024 * 1024;
const COVER_BYTES_MAX: usize = 32 * 1024;
pub(super) const COVER_SIDE_MIN: u16 = 48;
const CACHED: usize = 16;
const KEPT: usize = 4;
/// `v2.asset.list` pages read by the session-start sweep, four Assets each.
const SWEEP_PAGES: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Cover {
    pub id: String,
    pub bytes: Arc<[u8]>,
}

#[derive(Default)]
pub(super) struct CoverCache {
    entries: VecDeque<((String, u16), Arc<Cover>)>,
}

impl CoverCache {
    pub fn get(&mut self, source: &str, side: u16) -> Option<Arc<Cover>> {
        let at = self
            .entries
            .iter()
            .position(|((id, s), _)| id == source && *s == side)?;
        let entry = self.entries.remove(at)?;
        let cover = entry.1.clone();
        self.entries.push_back(entry);
        Some(cover)
    }

    pub fn prepare(&mut self, source: &str, bytes: &[u8], side: u16) -> Result<Arc<Cover>, String> {
        if let Some(cover) = self.get(source, side) {
            return Ok(cover);
        }
        let cover = Arc::new(encode(bytes, side)?);
        if self.entries.len() == CACHED {
            self.entries.pop_front();
        }
        self.entries
            .push_back(((source.to_owned(), side), cover.clone()));
        Ok(cover)
    }
}

fn encode(source: &[u8], side: u16) -> Result<Cover, String> {
    if source.len() > SOURCE_BYTES_MAX {
        return Err("The cover picture is over 4 MiB.".into());
    }
    if !(COVER_SIDE_MIN..=256).contains(&side) {
        return Err("The cover size is out of range.".into());
    }
    let mut reader = ImageReader::new(Cursor::new(source))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODED_BYTES_MAX);
    limits.max_image_width = Some(SOURCE_SIDE_MAX);
    limits.max_image_height = Some(SOURCE_SIDE_MAX);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    if decoder.total_bytes() > DECODED_BYTES_MAX {
        return Err("The cover picture is too large to decode.".into());
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut picture = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    picture.apply_orientation(orientation);
    let side = u32::from(side);
    let rgb = picture
        .resize_to_fill(side, side, FilterType::Triangle)
        .to_rgb8();
    for quality in crate::photos::encode::QUALITIES {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, quality)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        if bytes.len() <= COVER_BYTES_MAX {
            return Ok(Cover {
                id: asset::asset_id(&bytes),
                bytes: bytes.into(),
            });
        }
    }
    Err("The cover doesn't fit in 32 KiB.".into())
}

#[derive(Debug, Default)]
pub(super) struct CoverStore {
    known: VecDeque<String>,
}

impl CoverStore {
    /// Deletes the JPEG Assets a Device holds when a session starts. In this release JPEG Assets are only covers,
    /// and a cover a Layout or a live value still shows is refused `in-use` and stays.
    pub fn sweep(&mut self, session: &Session) -> Result<(), AssetError> {
        let mut after = None;
        let mut jpegs = Vec::new();
        for _ in 0..SWEEP_PAGES {
            let page = asset::list_asset_page(session, after.as_deref())?;
            jpegs.extend(
                page.items
                    .into_iter()
                    .filter(|a| a.format == AssetFormat::Jpeg)
                    .map(|a| a.id),
            );
            if page.next.is_none() || page.next == after {
                break;
            }
            after = page.next;
        }
        for id in jpegs {
            delete_unless_shown(session, &id)?;
        }
        self.known.clear();
        Ok(())
    }

    pub fn upload(&mut self, session: &Session, cover: &Cover) -> Result<(), AssetError> {
        if self.known.contains(&cover.id) {
            self.forget(&cover.id);
            self.known.push_back(cover.id.clone());
            return Ok(());
        }
        while self.known.len() >= KEPT && self.evict_oldest(session)? {}
        if let Err(error) = asset::put_asset(session, &cover.bytes, AssetFormat::Jpeg) {
            if error.reason() != Some(&AssetReason::Full) {
                return Err(error);
            }
            while self.evict_oldest(session)? {}
            asset::put_asset(session, &cover.bytes, AssetFormat::Jpeg)?;
        }
        self.known.push_back(cover.id.clone());
        Ok(())
    }

    pub fn forget(&mut self, id: &str) {
        self.known.retain(|known| known != id);
    }

    fn evict_oldest(&mut self, session: &Session) -> Result<bool, AssetError> {
        for id in self.known.clone() {
            if delete_unless_shown(session, &id)? {
                self.forget(&id);
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn delete_unless_shown(session: &Session, id: &str) -> Result<bool, AssetError> {
    match asset::delete_asset(session, id) {
        Ok(_) => Ok(true),
        Err(error) if error.reason() == Some(&AssetReason::InUse) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::photos::encode::tests::noise_png;
    use td_link::{MockDeck, SessionConfig};

    fn deck() -> (MockDeck, Session) {
        let deck = MockDeck::v2();
        deck.set_media_capabilities(true);
        let session = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..Default::default()
            },
        )
        .unwrap()
        .0;
        session.identity(std::time::Duration::from_secs(1)).unwrap();
        (deck, session)
    }

    fn cover(seed: u32) -> Arc<Cover> {
        CoverCache::default()
            .prepare(&format!("source{seed}"), &noise_png(50, 50, seed), 64)
            .unwrap()
    }

    fn puts(deck: &MockDeck) -> usize {
        deck.received()
            .iter()
            .filter(|r| r["m"] == "v2.asset.put")
            .count()
    }

    #[test]
    fn the_sweep_deletes_leftover_jpegs() {
        let (deck, session) = deck();
        for n in 90..96 {
            asset::put_asset(&session, &cover(n).bytes, AssetFormat::Jpeg).unwrap();
        }
        assert_eq!(deck.assets().len(), 6);
        CoverStore::default().sweep(&session).unwrap();
        assert!(deck.assets().is_empty());
    }

    #[test]
    fn a_device_keeps_at_most_four_covers_and_a_known_cover_is_not_sent_again() {
        let (deck, session) = deck();
        let mut store = CoverStore::default();
        for n in 0..10 {
            store.upload(&session, &cover(n)).unwrap();
            assert!(deck.assets().len() <= KEPT);
        }
        let last = cover(9);
        assert!(deck.assets().contains_key(&last.id));
        let sent = puts(&deck);
        store.upload(&session, &last).unwrap();
        assert_eq!(puts(&deck), sent);
        assert!(!deck.assets().contains_key(&cover(5).id));
        assert!(deck.assets().contains_key(&cover(6).id));
    }

    #[test]
    fn a_full_device_loses_its_older_covers_before_the_new_one_goes() {
        let (deck, session) = deck();
        let mut store = CoverStore::default();
        store.upload(&session, &cover(1)).unwrap();
        store.upload(&session, &cover(2)).unwrap();
        deck.set_asset_capacity(0);
        assert_eq!(
            store.upload(&session, &cover(3)).unwrap_err().reason(),
            Some(&AssetReason::Full)
        );
        assert!(deck.assets().is_empty());
    }

    #[test]
    fn covers_are_square_bounded_jpegs_made_once_per_source_and_side() {
        let mut cache = CoverCache::default();
        let source = noise_png(350, 200, 17);
        let cover = cache.prepare("a", &source, 174).unwrap();
        assert!(cover.bytes.len() <= COVER_BYTES_MAX);
        assert_eq!(AssetFormat::sniff(&cover.bytes), Some(AssetFormat::Jpeg));
        let decoded = image::load_from_memory(&cover.bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (174, 174));
        assert_eq!(cover.id, asset::asset_id(&cover.bytes));
        assert!(Arc::ptr_eq(&cover, &cache.prepare("a", b"", 174).unwrap()));
        assert_ne!(cover.id, cache.prepare("a", &source, 96).unwrap().id);
        let noisy = noise_png(1024, 1024, 3);
        assert!(cache.prepare("noisy", &noisy, 256).unwrap().bytes.len() <= COVER_BYTES_MAX);
    }

    #[test]
    fn hostile_pictures_are_refused_without_decoding_them() {
        let mut cache = CoverCache::default();
        assert!(
            cache
                .prepare("big", &vec![0; SOURCE_BYTES_MAX + 1], 48)
                .is_err()
        );
        assert!(
            cache
                .prepare("url", b"https://invalid.example/cover.jpg", 48)
                .is_err()
        );
        assert!(cache.prepare("wide", &noise_png(4097, 1, 1), 48).is_err());
        assert!(cache.prepare("side", &noise_png(10, 10, 1), 47).is_err());
        assert!(cache.prepare("side", &noise_png(10, 10, 1), 257).is_err());
    }
}
