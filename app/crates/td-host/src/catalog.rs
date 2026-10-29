//! Offline catalogs and fetch-on-pick images. Raw downloads are independent of Device Assets.
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub source: String,
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub hex: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub svg: bool,
    #[serde(default)]
    pub light: bool,
    #[serde(default)]
    pub dark: bool,
    pub license: String,
    pub license_url: Option<String>,
    pub guidelines: Option<String>,
    pub attribution_url: String,
}
#[derive(Deserialize)]
pub struct Index {
    pub simple_icons_version: String,
    pub selfhst_commit: String,
    pub entries: Vec<Entry>,
}
pub fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        serde_json::from_str(include_str!("catalog/index.json")).expect("bundled catalog")
    })
}
/// Exact, prefix, then substring matches; ties retain the generated name/source/id order.
pub fn search(query: &str, source: Option<&str>) -> Vec<&'static Entry> {
    let query = query.trim().to_lowercase();
    let mut hits: Vec<_> = index()
        .entries
        .iter()
        .filter(|e| {
            source.is_none_or(|s| s == e.source)
                && !(e.source == "simple-icons"
                    && (e.license.contains("NC") || e.license.contains("ND")))
        })
        .filter_map(|e| {
            let rank = std::iter::once(&e.name)
                .chain(&e.aliases)
                .chain(&e.tags)
                .filter_map(|s| {
                    let s = s.to_lowercase();
                    if s == query {
                        Some(0)
                    } else if s.starts_with(&query) {
                        Some(1)
                    } else if s.contains(&query) {
                        Some(2)
                    } else {
                        None
                    }
                })
                .min()?;
            Some((rank, e))
        })
        .collect();
    hits.sort_by_key(|(rank, _)| *rank);
    hits.into_iter().map(|(_, e)| e).collect()
}
#[derive(Debug)]
pub enum Error {
    NotFound,
    Fetch(String),
    Cache(std::io::Error),
    Render(String),
    InvalidSize,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Catalog icon or variant not found"),
            Self::Fetch(e) => write!(f, "Icon download failed: {e}"),
            Self::Cache(_) => write!(f, "Icon cache unavailable"),
            Self::Render(e) => write!(f, "Icon render failed: {e}"),
            Self::InvalidSize => write!(f, "Invalid Tile dimensions or padding"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Cache(e)
    }
}
/// Implementations must bound request duration and downloaded bytes.
pub trait Fetcher: Send + Sync {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error>;
}
pub struct HttpFetcher;
impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
        ureq::get(url)
            .config()
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .call()
            .map_err(|e| Error::Fetch(e.to_string()))?
            .body_mut()
            .with_config()
            .limit(4 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| Error::Fetch(e.to_string()))
    }
}
pub const CACHE_CAP: u64 = 20 * 1024 * 1024;
/// Serialized atomic writes and persistent last-use times. One cache per Companion.
pub struct Cache {
    dir: PathBuf,
    cap: u64,
    clock: Mutex<SystemTime>,
}
impl Cache {
    pub fn new(dir: PathBuf, cap: u64) -> Self {
        Self {
            dir,
            cap,
            clock: Mutex::new(SystemTime::UNIX_EPOCH),
        }
    }
    pub fn get_or_fetch(
        &self,
        key: &str,
        url: &str,
        fetcher: &dyn Fetcher,
    ) -> Result<Vec<u8>, Error> {
        let mut clock = self.clock.lock().unwrap();
        fs::create_dir_all(&self.dir)?;
        let name = format!("{:x}", Sha256::digest(key.as_bytes()));
        let path = self.dir.join(format!("{name}.bin"));
        *clock = SystemTime::now().max(*clock + Duration::from_millis(1));
        if let Ok(bytes) = fs::read(&path) {
            fs::OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_times(fs::FileTimes::new().set_modified(*clock))?;
            return Ok(bytes);
        }
        let bytes = fetcher.fetch(url)?;
        if bytes.len() as u64 > self.cap {
            return Err(Error::Fetch("Icon exceeds cache size limit".into()));
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|e| e == "bin") {
                let meta = entry.metadata()?;
                files.push((meta.modified()?, entry.path(), meta.len()));
            }
        }
        files.sort();
        let mut total: u64 = files.iter().map(|f| f.2).sum();
        for (_, old, len) in files {
            if total + bytes.len() as u64 <= self.cap {
                break;
            }
            fs::remove_file(old)?;
            total -= len;
        }
        let temp = self.dir.join(format!("{name}.tmp"));
        let result = (|| -> Result<(), Error> {
            use std::io::Write;
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.set_times(fs::FileTimes::new().set_modified(*clock))?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        Ok(bytes)
    }
}
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    #[default]
    Base,
    Light,
    Dark,
}
/// Pixel dimensions come from the caller's Tile geometry; colors are straight RGBA/RGB.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub cover: bool,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub padding: u32,
    pub background: Option<[u8; 4]>,
    pub tint: Option<[u8; 3]>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            cover: false,
            width: 64,
            height: 64,
            padding: 0,
            background: None,
            tint: None,
        }
    }
}
pub struct Catalog {
    pub cache: Cache,
    pub fetcher: Box<dyn Fetcher>,
}
impl Catalog {
    pub fn new(dir: PathBuf, fetcher: Box<dyn Fetcher>) -> Self {
        Self {
            cache: Cache::new(dir, CACHE_CAP),
            fetcher,
        }
    }
    pub fn render(
        &self,
        source: &str,
        id: &str,
        variant: Variant,
        options: Options,
    ) -> Result<RgbaImage, Error> {
        validate(options)?;
        let entry = index()
            .entries
            .iter()
            .find(|e| e.source == source && e.id == id)
            .ok_or(Error::NotFound)?;
        let suffix = match variant {
            Variant::Base => "",
            Variant::Light if entry.light => "-light",
            Variant::Dark if entry.dark => "-dark",
            _ => return Err(Error::NotFound),
        };
        let ext = if entry.svg { "svg" } else { "png" };
        let url = if source == "simple-icons" {
            format!(
                "https://cdn.jsdelivr.net/npm/simple-icons@{}/icons/{id}.svg",
                index().simple_icons_version
            )
        } else {
            format!(
                "https://cdn.jsdelivr.net/gh/selfhst/icons@{}/{ext}/{id}{suffix}.{ext}",
                index().selfhst_commit
            )
        };
        // The immutable URL includes source, version, slug, variant and format.
        let bytes = self.cache.get_or_fetch(&url, &url, self.fetcher.as_ref())?;
        fit(&bytes, entry.svg, options)
    }
}
fn validate(o: Options) -> Result<(), Error> {
    if o.width == 0
        || o.height == 0
        || o.width > 1024
        || o.height > 1024
        || o.padding >= o.width.div_ceil(2)
        || o.padding >= o.height.div_ceil(2)
    {
        return Err(Error::InvalidSize);
    }
    Ok(())
}
pub fn fit(bytes: &[u8], svg: bool, o: Options) -> Result<RgbaImage, Error> {
    validate(o)?;
    let image = if svg {
        crate::icon_pack::render_svg_fit(
            bytes,
            o.width - 2 * o.padding,
            o.height - 2 * o.padding,
            o.cover,
        )
        .map_err(Error::Render)?
    } else {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        reader.limits(limits);
        reader
            .decode()
            .map_err(|e| Error::Render(e.to_string()))?
            .into_rgba8()
    };
    let image = if o.cover && !svg {
        let ratio = (o.width - 2 * o.padding) as f64 / (o.height - 2 * o.padding) as f64;
        let w = image
            .width()
            .min((image.height() as f64 * ratio).round().max(1.0) as u32);
        let h = image
            .height()
            .min((image.width() as f64 / ratio).round().max(1.0) as u32);
        image::imageops::crop_imm(
            &image,
            (image.width() - w) / 2,
            (image.height() - h) / 2,
            w,
            h,
        )
        .to_image()
    } else {
        image
    };
    Ok(crate::icon_pack::fit_tile(
        &image,
        o.width,
        o.height,
        o.padding,
        o.background,
        o.tint,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path fill="red" d="M0 0h10v10H0z"/></svg>"#;
    struct Fake(AtomicUsize);
    impl Fetcher for Fake {
        fn fetch(&self, _: &str) -> Result<Vec<u8>, Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(vec![1; 4])
        }
    }
    #[test]
    fn offline_search_ranking_licenses_and_metadata() {
        let all = search("", None);
        assert!(all.iter().any(|e| e.source == "selfhst"));
        assert!(all.iter().any(|e| e.source == "simple-icons"));
        assert!(
            all.iter()
                .all(|e| !e.license.contains("NC") && !e.license.contains("ND"))
        );
        assert!(index().entries.iter().any(|e| e.license.contains("NC")));
        for query in ["git", "home", "pro", "cloud"] {
            let hits = search(query, None);
            let ranks: Vec<_> = hits
                .iter()
                .map(|e| {
                    std::iter::once(&e.name)
                        .chain(&e.aliases)
                        .chain(&e.tags)
                        .filter_map(|s| {
                            let s = s.to_lowercase();
                            if s == query {
                                Some(0)
                            } else if s.starts_with(query) {
                                Some(1)
                            } else if s.contains(query) {
                                Some(2)
                            } else {
                                None
                            }
                        })
                        .min()
                        .unwrap()
                })
                .collect();
            assert!(ranks.windows(2).all(|r| r[0] <= r[1]));
        }
        assert_eq!(search("github", Some("simple-icons"))[0].id, "github");
        assert!(
            search("", Some("selfhst"))
                .iter()
                .all(|e| e.source == "selfhst"
                    && e.license == "CC-BY-4.0"
                    && e.license_url.is_some())
        );
    }
    #[test]
    fn cache_hits_persist_and_evict_lru() {
        let dir = std::env::temp_dir().join(format!("td-catalog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let fake = Fake(AtomicUsize::new(0));
        let cache = Cache::new(dir.clone(), 8);
        for key in ["a", "b", "a", "c", "a"] {
            cache.get_or_fetch(key, key, &fake).unwrap();
        }
        assert_eq!(fake.0.load(Ordering::SeqCst), 3);
        let cache = Cache::new(dir.clone(), 8);
        cache.get_or_fetch("a", "a", &fake).unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 3);
        cache.get_or_fetch("b", "b", &fake).unwrap();
        assert_eq!(fake.0.load(Ordering::SeqCst), 4);
        for n in 0..30 {
            cache.get_or_fetch(&n.to_string(), "", &fake).unwrap();
            let total: u64 = fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().metadata().unwrap().len())
                .sum();
            assert!(total <= 8);
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn picks_use_pinned_urls_variants_png_fallback_and_cache() {
        struct ImageFetch(AtomicUsize);
        impl Fetcher for ImageFetch {
            fn fetch(&self, url: &str) -> Result<Vec<u8>, Error> {
                self.0.fetch_add(1, Ordering::SeqCst);
                assert!(url.starts_with("https://cdn.jsdelivr.net/"));
                if url.ends_with(".svg") {
                    return Ok(SVG.to_vec());
                }
                assert!(url.contains(&format!("selfhst/icons@{}/png/", index().selfhst_commit)));
                let mut out = std::io::Cursor::new(Vec::new());
                RgbaImage::new(2, 2)
                    .write_to(&mut out, image::ImageFormat::Png)
                    .unwrap();
                Ok(out.into_inner())
            }
        }
        let dir = std::env::temp_dir().join(format!("td-catalog-picks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let catalog = Catalog::new(dir.clone(), Box::new(ImageFetch(AtomicUsize::new(0))));
        for entry in [
            index()
                .entries
                .iter()
                .find(|e| e.source == "simple-icons")
                .unwrap(),
            index()
                .entries
                .iter()
                .find(|e| e.source == "selfhst" && !e.svg)
                .unwrap(),
        ] {
            assert_eq!(
                catalog
                    .render(&entry.source, &entry.id, Variant::Base, Options::default())
                    .unwrap()
                    .dimensions(),
                (64, 64)
            );
        }
        let entry = index()
            .entries
            .iter()
            .find(|e| e.source == "selfhst" && e.light)
            .unwrap();
        catalog
            .render(&entry.source, &entry.id, Variant::Light, Options::default())
            .unwrap();
        assert!(matches!(
            catalog.render("simple-icons", "github", Variant::Dark, Options::default()),
            Err(Error::NotFound)
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fit_dimensions_pixels_png_and_errors() {
        let o = Options {
            cover: false,
            width: 40,
            height: 20,
            padding: 3,
            background: Some([30, 40, 50, 255]),
            tint: Some([70, 80, 90]),
        };
        let image = fit(SVG, true, o).unwrap();
        assert_eq!(image.dimensions(), (40, 20));
        assert_eq!(image.get_pixel(0, 0).0, [30, 40, 50, 255]);
        assert_eq!(image.get_pixel(20, 10).0, [70, 80, 90, 255]);
        let covered = fit(
            SVG,
            true,
            Options {
                cover: true,
                width: 40,
                height: 20,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(covered.get_pixel(0, 0).0, [255, 0, 0, 255]);
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        assert_eq!(fit(png.get_ref(), false, o).unwrap().dimensions(), (40, 20));
        assert!(matches!(fit(b"broken", true, o), Err(Error::Render(_))));
        assert!(matches!(
            fit(
                SVG,
                true,
                Options {
                    padding: 32,
                    ..Default::default()
                }
            ),
            Err(Error::InvalidSize)
        ));
        assert_eq!(
            fit(
                SVG,
                true,
                Options {
                    width: 40,
                    height: 20,
                    ..Default::default()
                }
            )
            .unwrap()
            .get_pixel(0, 0)
            .0,
            [0; 4]
        );
    }
    #[test]
    fn fits_both_orientations_and_all_grid_spans() {
        // Geometry uses integer edges, a 16px gap and 28/76/60px margins.
        for (screen_w, screen_h) in [(1024, 600), (600, 1024)] {
            for fullscreen in [false, true] {
                let (w, h) = if fullscreen {
                    (screen_w, screen_h)
                } else {
                    (screen_w - 56, screen_h - 136)
                };
                for cells in 1..=8 {
                    for span in 1..=cells {
                        let width = span * (w + 16) / cells - 16;
                        let height = span * (h + 16) / cells - 16;
                        let image = fit(
                            SVG,
                            true,
                            Options {
                                width,
                                height,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        assert_eq!(image.dimensions(), (width, height));
                    }
                }
            }
        }
    }
}
