//! Provenance and the 30-day rule. Brandfetch's terms (§5.1) let content fetched through its
//! API be cached for at most 30 days from retrieval, and ask for a new API call at least every
//! 30 days, or the cached content must go. So each Brandfetch Asset records where it came from
//! and when, the original file stays cached with that time, and a daily check calls the Brand
//! API again from day 25:
//!
//! - same file: the fetch time moves to now;
//! - a changed file: it renders again and the PC's stored Layout copies point at the new Asset,
//!   so the next push updates the Device (nothing is pushed here);
//! - a failure: it tries again the next day;
//! - past 30 days with no success: the cached original is deleted and the logo is marked
//!   expired. A Device's Asset is never deleted here.
use super::{
    Brandfetch, BrandfetchLogo,
    client::{FileFormat, Original, draw},
};
use crate::{
    features::assets::Fit,
    store::{AssetStore, LayoutStore},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

const DAY_S: u64 = 24 * 60 * 60;
/// A logo this old gets a new Brand API call.
pub const REFRESH_AFTER_S: u64 = 25 * DAY_S;
/// A logo this old with no successful call loses its cached original.
pub const EXPIRE_AFTER_S: u64 = 30 * DAY_S;
/// A failed refresh waits this long before the next try. The check runs once a day, so the
/// hour of slack keeps a slightly early run from skipping a day.
const RETRY_AFTER_S: u64 = DAY_S - 60 * 60;

pub(super) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    logo: BrandfetchLogo,
    fit: Fit,
    /// The SHA-256 of the cached original, which names its file. Empty once expired.
    original: String,
    format: FileFormat,
    /// Unix seconds of the last successful Brand API call for this logo.
    fetched_at: u64,
    /// Unix seconds of the last refresh attempt, successful or not.
    #[serde(default)]
    tried_at: u64,
    #[serde(default)]
    expired: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Book {
    #[serde(default)]
    logos: BTreeMap<String, Record>,
}

/// What one refresh run did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RefreshReport {
    /// Logos whose file was unchanged, now fetched again.
    pub renewed: u32,
    /// `(old, new)` Asset ids for logos whose file changed.
    pub replaced: Vec<(String, String)>,
    /// Refresh attempts that failed and will be retried.
    pub failed: u32,
    /// Logos that passed 30 days without a successful refresh in this run.
    pub expired: u32,
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl Brandfetch {
    fn book_path(&self) -> PathBuf {
        self.0.dir.join("provenance.json")
    }
    fn originals(&self) -> PathBuf {
        self.0.dir.join("originals")
    }
    fn original_path(&self, sha: &str, format: FileFormat) -> PathBuf {
        self.originals()
            .join(format!("{sha}.{}", format.extension()))
    }
    /// A missing or unreadable file reads as no logos.
    fn read_book(&self) -> Book {
        std::fs::read(self.book_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    fn write_book(&self, book: &Book) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(book).map_err(std::io::Error::other)?;
        td_core::app_dirs::write_atomic(&self.book_path(), &bytes)?;
        // Originals no record names any more go too.
        let keep: std::collections::BTreeSet<_> = book
            .logos
            .values()
            .filter(|r| !r.original.is_empty())
            .map(|r| {
                self.original_path(&r.original, r.format)
                    .file_name()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        for entry in std::fs::read_dir(self.originals())
            .into_iter()
            .flatten()
            .flatten()
        {
            if !keep.contains(&entry.file_name()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }
    fn save_original(&self, original: &Original) -> std::io::Result<String> {
        let sha = sha(&original.bytes);
        td_core::app_dirs::write_atomic(
            &self.original_path(&sha, original.format),
            &original.bytes,
        )?;
        Ok(sha)
    }
    /// Records a freshly picked logo: its options, its original file and the fetch time.
    pub(super) fn record(
        &self,
        asset: &str,
        logo: &BrandfetchLogo,
        fit: Fit,
        original: &Original,
        now: u64,
    ) -> std::io::Result<()> {
        let _book = self.0.book.lock();
        let sha = self.save_original(original)?;
        let mut book = self.read_book();
        book.logos.insert(
            asset.into(),
            Record {
                logo: logo.clone(),
                fit,
                original: sha,
                format: original.format,
                fetched_at: now,
                tried_at: now,
                expired: false,
            },
        );
        self.write_book(&book)
    }
    /// `(logos, need refresh, expired Asset ids)` at `now`.
    pub(super) fn counts(&self, now: u64) -> (u32, u32, Vec<String>) {
        let book = self.read_book();
        let stale = book
            .logos
            .values()
            .filter(|r| r.expired || now.saturating_sub(r.fetched_at) >= REFRESH_AFTER_S)
            .count();
        let expired = book
            .logos
            .iter()
            .filter(|(_, r)| r.expired)
            .map(|(id, _)| id.clone())
            .collect();
        (book.logos.len() as u32, stale as u32, expired)
    }
    /// One refresh run at `now`, in Unix seconds. Blocking: it calls the Brand API once per
    /// due logo.
    pub fn refresh(&self, assets: &AssetStore, layouts: &LayoutStore, now: u64) -> RefreshReport {
        let _running = self.0.refreshing.lock();
        let mut report = RefreshReport::default();
        let due: Vec<(String, BrandfetchLogo)> = self
            .read_book()
            .logos
            .into_iter()
            .filter(|(_, r)| {
                now.saturating_sub(r.fetched_at) >= REFRESH_AFTER_S
                    && (r.tried_at <= r.fetched_at
                        || now.saturating_sub(r.tried_at) >= RETRY_AFTER_S)
            })
            .map(|(id, r)| (id, r.logo))
            .collect();
        for (asset, logo) in due {
            // The network call runs outside the provenance lock, so a pick isn't held up.
            let fetched = self.fetch(&logo);
            let _book = self.0.book.lock();
            let mut book = self.read_book();
            let Some(record) = book.logos.get(&asset).cloned() else {
                continue;
            };
            let outcome = fetched.ok().and_then(|original| {
                let sha = sha(&original.bytes);
                if sha == record.original {
                    return Some((None, original, sha));
                }
                let image = draw(&original, &record.logo, record.fit).ok()?;
                let id = assets.put(&td_host::icon_pack::pack_any(&image)).ok()?;
                Some((Some(id), original, sha))
            });
            match outcome {
                Some((new, original, sha)) => {
                    if self.save_original(&original).is_err() {
                        report.failed += 1;
                        book.logos.get_mut(&asset).unwrap().tried_at = now;
                        let _ = self.write_book(&book);
                        continue;
                    }
                    let renewed = Record {
                        original: sha,
                        format: original.format,
                        fetched_at: now,
                        tried_at: now,
                        expired: false,
                        ..record
                    };
                    match new {
                        Some(new) if new != asset => {
                            if layouts.replace_asset(&asset, &new).is_err() {
                                eprintln!(
                                    "Companion: a refreshed Brandfetch logo couldn't update the stored Layouts."
                                );
                            }
                            book.logos.remove(&asset);
                            book.logos.insert(new.clone(), renewed);
                            report.replaced.push((asset, new));
                        }
                        _ => {
                            book.logos.insert(asset, renewed);
                            report.renewed += 1;
                        }
                    }
                }
                None => {
                    book.logos.get_mut(&asset).unwrap().tried_at = now;
                    report.failed += 1;
                }
            }
            let _ = self.write_book(&book);
        }
        // Expiry also covers logos whose retry waits for tomorrow.
        let _book = self.0.book.lock();
        let mut book = self.read_book();
        let mut changed = false;
        for record in book.logos.values_mut() {
            if !record.expired && now.saturating_sub(record.fetched_at) >= EXPIRE_AFTER_S {
                record.expired = true;
                record.original.clear();
                report.expired += 1;
                changed = true;
            }
        }
        if changed && self.write_book(&book).is_err() {
            eprintln!("Companion: couldn't record expired Brandfetch logos.");
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::brandfetch::{LogoTheme, LogoType, client};
    use parking_lot::Mutex;
    use std::sync::Arc;

    /// Answers the Brand API with one SVG logo whose color the test changes, or fails.
    #[derive(Default)]
    struct Fake {
        color: Mutex<String>,
        down: Mutex<bool>,
        calls: Mutex<u32>,
    }
    impl client::Http for Arc<Fake> {
        fn get(&self, url: &str, _: Option<&client::ApiKey>) -> Result<client::Response, String> {
            if *self.down.lock() {
                return Err("offline".into());
            }
            let ok = |body: String| client::Response {
                status: 200,
                reason: None,
                body: body.into_bytes(),
            };
            if url.contains("/v2/brands/") {
                *self.calls.lock() += 1;
                return Ok(ok(r#"{"logos":[{"type":"icon","theme":"light","formats":[{"src":"https://asset.brandfetch.io/a.svg","format":"svg"}]}]}"#.into()));
            }
            Ok(ok(format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="{}"/></svg>"#,
                self.color.lock()
            )))
        }
    }
    struct Rig {
        _dir: tempfile::TempDir,
        bf: Brandfetch,
        fake: Arc<Fake>,
        assets: AssetStore,
        layouts: LayoutStore,
    }
    const T0: u64 = 1_800_000_000;
    fn rig() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let bf = Brandfetch::new(dir.path().join("brandfetch"), true);
        let fake = Arc::new(Fake::default());
        *fake.color.lock() = "#ff0000".into();
        bf.0.net.write().http = Arc::new(fake.clone());
        bf.0.secrets
            .put(
                "brandfetch",
                "api_key",
                td_host::integrations::secrets::Secret {
                    origin: super::super::API_BASE.into(),
                    value: "test-api-key".into(),
                },
            )
            .unwrap();
        let layouts = LayoutStore::new(dir.path().join("layouts"));
        let assets = AssetStore::new(
            dir.path().join("assets"),
            layouts.clone(),
            crate::api::Clock::default(),
            crate::store::ASSET_STORE_BYTES,
        );
        Rig {
            _dir: dir,
            bf,
            fake,
            assets,
            layouts,
        }
    }
    fn logo() -> BrandfetchLogo {
        BrandfetchLogo {
            brand: "example.com".into(),
            logo_type: LogoType::Icon,
            theme: LogoTheme::Auto,
            padding: 4,
            background: None,
            tint: None,
        }
    }
    /// Picks the logo at `T0` as the render Job does and puts it on a stored Layout.
    fn pick(r: &Rig) -> String {
        let original = r.bf.fetch(&logo()).unwrap();
        let image = draw(&original, &logo(), Fit::Contain).unwrap();
        let id = r.assets.put(&td_host::icon_pack::pack_any(&image)).unwrap();
        r.bf.record(&id, &logo(), Fit::Contain, &original, T0)
            .unwrap();
        let layout = td_core::Layout::parse(
            serde_json::json!({
                "schema":2,"name":"Logos","orientation":"landscape",
                "pages":[{"kind":"grid","id":"main","title":"Main","rows":1,"cols":1,
                "tiles":[{"id":"t0","label":"Logo","row":0,"col":0,"rowSpan":1,"colSpan":1,
                "icon":null,"asset":format!("sha256:{id}"),"action":{"type":"none"}}]}]
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        r.layouts.put("mock0001", &layout).unwrap();
        id
    }
    fn originals(r: &Rig) -> usize {
        std::fs::read_dir(r.bf.originals()).map_or(0, |d| d.count())
    }
    fn tile_asset(r: &Rig) -> Option<String> {
        r.layouts.all("mock0001").into_iter().find_map(|l| {
            l.pages.into_iter().find_map(|p| match p {
                td_core::layout::Page::Grid { tiles, .. } => tiles[0].asset.clone(),
                _ => None,
            })
        })
    }
    const DAY: u64 = DAY_S;

    #[test]
    fn nothing_is_called_before_day_25() {
        let r = rig();
        pick(&r);
        let calls = *r.fake.calls.lock();
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 25 * DAY - 1);
        assert_eq!(report, RefreshReport::default());
        assert_eq!(*r.fake.calls.lock(), calls);
    }

    #[test]
    fn the_same_file_moves_the_fetch_time() {
        let r = rig();
        let id = pick(&r);
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 25 * DAY);
        assert_eq!(report.renewed, 1);
        assert_eq!(tile_asset(&r), Some(format!("sha256:{id}")));
        // Fresh again: another 25 days pass before the next call.
        let calls = *r.fake.calls.lock();
        r.bf.refresh(&r.assets, &r.layouts, T0 + 49 * DAY);
        assert_eq!(*r.fake.calls.lock(), calls);
        assert_eq!(r.bf.counts(T0 + 49 * DAY), (1, 0, vec![]));
    }

    #[test]
    fn a_changed_file_renders_again_and_updates_the_stored_layout() {
        let r = rig();
        let old = pick(&r);
        *r.fake.color.lock() = "#0000ff".into();
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 26 * DAY);
        assert_eq!(report.replaced.len(), 1);
        let (was, new) = &report.replaced[0];
        assert_eq!(was, &old);
        assert_ne!(new, &old);
        assert_eq!(tile_asset(&r), Some(format!("sha256:{new}")));
        assert!(r.assets.get(new).is_some());
        // Only the new original is kept.
        assert_eq!(originals(&r), 1);
        assert_eq!(r.bf.counts(T0 + 26 * DAY), (1, 0, vec![]));
    }

    #[test]
    fn a_failure_retries_the_next_day_then_expires_at_30_days() {
        let r = rig();
        let id = pick(&r);
        *r.fake.down.lock() = true;
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 25 * DAY);
        assert_eq!(report.failed, 1);
        assert_eq!(originals(&r), 1);
        // A second run the same day doesn't call again.
        let again = r.bf.refresh(&r.assets, &r.layouts, T0 + 25 * DAY + 3600);
        assert_eq!(again, RefreshReport::default());
        for day in 26..30 {
            assert_eq!(
                r.bf.refresh(&r.assets, &r.layouts, T0 + day * DAY).failed,
                1
            );
        }
        assert_eq!(r.bf.counts(T0 + 29 * DAY), (1, 1, vec![]));
        let last = r.bf.refresh(&r.assets, &r.layouts, T0 + 30 * DAY);
        assert_eq!((last.failed, last.expired), (1, 1));
        assert_eq!(originals(&r), 0, "the cached original is deleted");
        assert_eq!(r.bf.counts(T0 + 30 * DAY), (1, 1, vec![id.clone()]));
        // The Asset and the Layout stay; the user decides what to do with the Tile.
        assert!(r.assets.get(&id).is_some());
        assert_eq!(tile_asset(&r), Some(format!("sha256:{id}")));
        // A later success brings it back.
        *r.fake.down.lock() = false;
        let back = r.bf.refresh(&r.assets, &r.layouts, T0 + 31 * DAY);
        assert_eq!(back.renewed, 1);
        assert_eq!(r.bf.counts(T0 + 31 * DAY), (1, 0, vec![]));
        assert_eq!(originals(&r), 1);
    }

    #[test]
    fn expiry_happens_even_while_a_retry_waits() {
        let r = rig();
        pick(&r);
        *r.fake.down.lock() = true;
        r.bf.refresh(&r.assets, &r.layouts, T0 + 30 * DAY - 60);
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 30 * DAY);
        assert_eq!((report.failed, report.expired), (0, 1));
        assert_eq!(originals(&r), 0);
    }

    #[test]
    fn without_a_key_a_logo_still_expires() {
        let r = rig();
        let id = pick(&r);
        r.bf.0.secrets.delete("brandfetch", "api_key").unwrap();
        let report = r.bf.refresh(&r.assets, &r.layouts, T0 + 30 * DAY);
        assert_eq!((report.failed, report.expired), (1, 1));
        assert_eq!(r.bf.counts(T0 + 30 * DAY).2, vec![id]);
    }
}
