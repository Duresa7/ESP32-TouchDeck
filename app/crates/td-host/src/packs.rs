//! Icon packs: licensed Iconify sets a person installs, searched and drawn offline.
//!
//! The curated list in `packs.json` pins each `@iconify-json/<prefix>` npm package by version
//! and `dist.integrity`; `app/scripts/generate_packs.py` writes it. Installing one downloads that
//! tarball from the npm registry, checks its SHA-512 and keeps only `icons.json`, `info.json` and
//! any license file in `<dir>/<prefix>/`. A person can also import an Iconify JSON file they
//! have. Nothing is installed by default.

use crate::{
    catalog::{self, Fetcher},
    iconify::{self, IconSet},
};
use base64::Engine;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use ts_rs::TS;

/// The only host packs download from.
pub const REGISTRY: &str = "https://registry.npmjs.org/";
/// The largest tarball or imported file accepted.
pub const MAX_DOWNLOAD: u64 = 32 << 20;
/// The most bytes a tarball may unpack to, and the largest single file kept from it.
const MAX_UNPACKED: u64 = 256 << 20;
const MAX_FILE: u64 = 64 << 20;
/// The license title of an imported pack.
pub const CUSTOM_LICENSE: &str = "Custom (you provided it)";
const META: &str = "pack.json";
const ICONS: &str = "icons.json";
/// The one license file a pack keeps, whatever the package called it.
const LICENSE: &str = "LICENSE";
/// The longest pax header or GNU long name the reader takes.
const MAX_HEADER: u64 = 64 << 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PackLicense {
    pub title: String,
    pub spdx: Option<String>,
    pub url: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PackAuthor {
    pub name: String,
    pub url: Option<String>,
}
/// Where an Icon pack comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PackOrigin {
    Curated,
    Imported,
}

/// One entry of `packs.json`.
#[derive(Clone, Debug, Deserialize)]
pub struct Curated {
    pub prefix: String,
    pub name: String,
    pub package: String,
    pub version: String,
    pub tarball: String,
    pub integrity: String,
    pub unpacked_size: u64,
    pub icons: u32,
    pub license: PackLicense,
    pub author: PackAuthor,
    pub attribution: bool,
    pub brands: bool,
    pub palette: bool,
    pub samples: Vec<String>,
}
#[derive(Deserialize)]
struct CuratedList {
    packs: Vec<Curated>,
}
/// The curated packs, in the order the UI offers them.
pub fn curated() -> &'static [Curated] {
    static LIST: OnceLock<Vec<Curated>> = OnceLock::new();
    LIST.get_or_init(|| {
        serde_json::from_str::<CuratedList>(include_str!("packs.json"))
            .expect("bundled packs.json")
            .packs
    })
}

/// An Icon pack as `GET /icon-packs` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PackInfo {
    pub prefix: String,
    pub name: String,
    pub origin: PackOrigin,
    pub installed: bool,
    /// The pinned package version, for curated packs.
    pub version: Option<String>,
    /// What a curated pack unpacks to before the Companion trims it, in bytes.
    pub download_bytes: Option<u64>,
    /// What the installed pack takes on disk, in bytes.
    pub size_bytes: Option<u64>,
    pub icons: u32,
    pub license: PackLicense,
    pub author: PackAuthor,
    /// Whether the license asks for credit wherever the icons show.
    pub attribution: bool,
    /// Whether the pack holds brand logos.
    pub brands: bool,
    /// Whether its icons carry their own colours, rather than `currentColor`.
    pub palette: bool,
    /// A few icon names to preview it with.
    pub samples: Vec<String>,
}

/// What `pack.json` keeps next to an installed pack.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Meta {
    prefix: String,
    name: String,
    origin: PackOrigin,
    version: Option<String>,
    icons: u32,
    license: PackLicense,
    author: PackAuthor,
    attribution: bool,
    brands: bool,
    palette: bool,
    samples: Vec<String>,
}

#[derive(Debug)]
pub enum PackError {
    /// No curated or installed pack has that prefix, or it has no such icon.
    NotFound,
    /// An installed pack already uses that prefix, or a curated pack reserves it.
    Exists(String),
    /// The file isn't a usable Iconify JSON pack.
    Invalid(String),
    /// The download failed, was too large, or didn't match its pinned integrity.
    Download(String),
    /// The pinned SHA-512 didn't match.
    Integrity,
    /// Another install of this pack is running.
    Busy,
    Cancelled,
    Io(std::io::Error),
}
impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "No Icon pack or icon with that name."),
            Self::Exists(prefix) => write!(
                f,
                "An Icon pack named \"{prefix}\" is already installed or reserved."
            ),
            Self::Invalid(reason) => write!(f, "This isn't a usable Iconify JSON pack: {reason}."),
            Self::Download(reason) => write!(f, "The Icon pack didn't download: {reason}."),
            Self::Integrity => write!(
                f,
                "The downloaded Icon pack doesn't match its pinned checksum, so it wasn't installed."
            ),
            Self::Busy => write!(f, "This Icon pack is already being installed."),
            Self::Cancelled => write!(f, "The install was cancelled."),
            Self::Io(_) => write!(f, "The Icon pack folder couldn't be written."),
        }
    }
}
impl std::error::Error for PackError {}
impl From<std::io::Error> for PackError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// An install step, for Job progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PackStage {
    Download,
    Verify,
    Extract,
    Done,
}

/// An installed pack, parsed once on first use.
struct Loaded {
    meta: Meta,
    set: IconSet,
    names: Vec<iconify::Named>,
}

/// A search hit in an installed pack.
#[derive(Clone, Debug)]
pub struct PackHit {
    pub prefix: String,
    pub pack: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub license: PackLicense,
    pub author: PackAuthor,
    pub attribution: bool,
    pub brands: bool,
    pub palette: bool,
}

/// The installed packs under one folder. One per Companion.
pub struct Packs {
    dir: PathBuf,
    fetcher: Box<dyn Fetcher>,
    curated: Vec<Curated>,
    loaded: Mutex<HashMap<String, Arc<Loaded>>>,
    installing: Mutex<HashSet<String>>,
}

/// SRI `sha512-<base64>` of `bytes`.
pub fn integrity_of(bytes: &[u8]) -> String {
    format!(
        "sha512-{}",
        base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes))
    )
}

impl Packs {
    pub fn new(dir: PathBuf, fetcher: Box<dyn Fetcher>) -> Self {
        Self::with_list(dir, fetcher, curated().to_vec())
    }

    /// A store that offers `curated` instead of the bundled list, for tests that serve their
    /// own tarballs.
    pub fn with_list(dir: PathBuf, fetcher: Box<dyn Fetcher>, curated: Vec<Curated>) -> Self {
        Self {
            dir,
            fetcher,
            curated,
            loaded: Mutex::new(HashMap::new()),
            installing: Mutex::new(HashSet::new()),
        }
    }

    fn folder(&self, prefix: &str) -> Option<PathBuf> {
        iconify::valid_name(prefix).then(|| self.dir.join(prefix))
    }

    fn meta(&self, prefix: &str) -> Option<Meta> {
        let dir = self.folder(prefix)?;
        let meta: Meta = serde_json::from_slice(&fs::read(dir.join(META)).ok()?).ok()?;
        (meta.prefix == prefix && dir.join(ICONS).is_file()).then_some(meta)
    }

    fn installed(&self) -> BTreeMap<String, Meta> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return BTreeMap::new();
        };
        entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|prefix| Some((prefix.clone(), self.meta(&prefix)?)))
            .collect()
    }

    fn size(&self, prefix: &str) -> Option<u64> {
        let dir = self.folder(prefix)?;
        Some(
            fs::read_dir(dir)
                .ok()?
                .filter_map(|e| e.ok()?.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum(),
        )
    }

    /// The curated packs, then imported ones by prefix, each with its installed state.
    pub fn list(&self) -> Vec<PackInfo> {
        let mut installed = self.installed();
        let mut out: Vec<PackInfo> = self
            .curated
            .iter()
            .map(|c| {
                let here = installed
                    .remove(&c.prefix)
                    .filter(|m| m.origin == PackOrigin::Curated);
                PackInfo {
                    prefix: c.prefix.clone(),
                    name: c.name.clone(),
                    origin: PackOrigin::Curated,
                    installed: here.is_some(),
                    version: Some(c.version.clone()),
                    download_bytes: Some(c.unpacked_size),
                    size_bytes: here.as_ref().and_then(|_| self.size(&c.prefix)),
                    icons: here.as_ref().map_or(c.icons, |m| m.icons),
                    license: c.license.clone(),
                    author: c.author.clone(),
                    attribution: c.attribution,
                    brands: c.brands,
                    palette: c.palette,
                    samples: c.samples.clone(),
                }
            })
            .collect();
        out.extend(
            installed
                .into_values()
                .filter(|m| m.origin == PackOrigin::Imported)
                .map(|m| self.info(m)),
        );
        out
    }

    fn info(&self, m: Meta) -> PackInfo {
        PackInfo {
            size_bytes: self.size(&m.prefix),
            download_bytes: self
                .curated
                .iter()
                .find(|c| c.prefix == m.prefix && m.origin == PackOrigin::Curated)
                .map(|c| c.unpacked_size),
            prefix: m.prefix,
            name: m.name,
            origin: m.origin,
            installed: true,
            version: m.version,
            icons: m.icons,
            license: m.license,
            author: m.author,
            attribution: m.attribution,
            brands: m.brands,
            palette: m.palette,
            samples: m.samples,
        }
    }

    /// Downloads, verifies and unpacks a curated pack. Installing one that is installed
    /// returns it as it is.
    pub fn install(
        &self,
        prefix: &str,
        stage: &dyn Fn(PackStage),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<PackInfo, PackError> {
        let pack = self
            .curated
            .iter()
            .find(|c| c.prefix == prefix)
            .ok_or(PackError::NotFound)?;
        if let Some(meta) = self.meta(prefix) {
            return if meta.origin == PackOrigin::Curated {
                Ok(self.info(meta))
            } else {
                Err(PackError::Exists(prefix.into()))
            };
        }
        let _guard = Installing::start(&self.installing, prefix)?;
        if !pack.tarball.starts_with(REGISTRY) {
            return Err(PackError::Download("it isn't on the npm registry".into()));
        }
        stage(PackStage::Download);
        let bytes = self
            .fetcher
            .fetch(&pack.tarball)
            .map_err(|e| PackError::Download(e.to_string()))?;
        if bytes.len() as u64 > MAX_DOWNLOAD {
            return Err(PackError::Download("it is larger than 32 MiB".into()));
        }
        if cancelled() {
            return Err(PackError::Cancelled);
        }
        stage(PackStage::Verify);
        if integrity_of(&bytes) != pack.integrity {
            return Err(PackError::Integrity);
        }
        stage(PackStage::Extract);
        let files = untar(&bytes, |path| {
            let Some(name) = path.strip_prefix("package/") else {
                return false;
            };
            name == ICONS || name == "info.json" || is_license(name)
        })
        .map_err(PackError::Invalid)?;
        let icons = files
            .iter()
            .find(|(name, _)| name == "package/icons.json")
            .ok_or_else(|| PackError::Invalid("the package holds no icons.json".into()))?;
        let set = IconSet::parse(&icons.1).map_err(PackError::Invalid)?;
        if set.prefix != pack.prefix {
            return Err(PackError::Invalid(format!("its prefix is {}", set.prefix)));
        }
        if cancelled() {
            return Err(PackError::Cancelled);
        }
        let names = set.names();
        let meta = Meta {
            prefix: pack.prefix.clone(),
            name: pack.name.clone(),
            origin: PackOrigin::Curated,
            version: Some(pack.version.clone()),
            icons: names.len() as u32,
            license: pack.license.clone(),
            author: pack.author.clone(),
            attribution: pack.attribution,
            brands: pack.brands,
            palette: pack.palette,
            samples: pack.samples.clone(),
        };
        // Files keep fixed names, so a license spelled two ways can't collide on a
        // case-insensitive file system.
        let mut license = false;
        let files = files
            .iter()
            .filter_map(|(path, data)| {
                let name = path.trim_start_matches("package/");
                if !is_license(name) {
                    return Some((name, data.as_slice()));
                }
                (!std::mem::replace(&mut license, true)).then_some((LICENSE, data.as_slice()))
            })
            .collect::<Vec<_>>();
        self.write(&meta, &files)?;
        self.keep(meta.clone(), set, names);
        stage(PackStage::Done);
        Ok(self.info(meta))
    }

    /// Validates and installs an Iconify JSON file someone provided.
    pub fn import(&self, json: &[u8]) -> Result<PackInfo, PackError> {
        if json.len() as u64 > MAX_DOWNLOAD {
            return Err(PackError::Invalid("the file is larger than 32 MiB".into()));
        }
        let set = IconSet::parse(json).map_err(PackError::Invalid)?;
        let prefix = set.prefix.clone();
        if self.curated.iter().any(|c| c.prefix == prefix) || self.meta(&prefix).is_some() {
            return Err(PackError::Exists(prefix));
        }
        let _guard = Installing::start(&self.installing, &prefix)?;
        let info = set.info.clone().unwrap_or_default();
        let names = set.names();
        let meta = Meta {
            name: if info.name.trim().is_empty() {
                prefix.clone()
            } else {
                info.name.trim().chars().take(80).collect()
            },
            prefix,
            origin: PackOrigin::Imported,
            version: None,
            icons: names.len() as u32,
            license: PackLicense {
                title: CUSTOM_LICENSE.into(),
                spdx: None,
                url: None,
            },
            author: PackAuthor {
                name: info.author.name.chars().take(80).collect(),
                url: None,
            },
            attribution: false,
            brands: false,
            palette: info.palette,
            samples: names.iter().take(6).map(|n| n.name.clone()).collect(),
        };
        self.write(&meta, &[(ICONS, json)])?;
        self.keep(meta.clone(), set, names);
        Ok(self.info(meta))
    }

    /// Writes a pack into a staging folder, then moves it into place.
    fn write(&self, meta: &Meta, files: &[(&str, &[u8])]) -> Result<(), PackError> {
        let target = self.folder(&meta.prefix).ok_or(PackError::NotFound)?;
        fs::create_dir_all(&self.dir)?;
        let staging = self.dir.join(format!(".staging-{}", meta.prefix));
        let _ = fs::remove_dir_all(&staging);
        let result = (|| -> std::io::Result<()> {
            fs::create_dir_all(&staging)?;
            for (name, data) in files {
                fs::write(staging.join(name), data)?;
            }
            fs::write(staging.join(META), serde_json::to_vec_pretty(meta)?)?;
            fs::rename(&staging, &target)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        Ok(result?)
    }

    fn keep(&self, meta: Meta, set: IconSet, names: Vec<iconify::Named>) {
        self.loaded
            .lock()
            .unwrap()
            .insert(meta.prefix.clone(), Arc::new(Loaded { meta, set, names }));
    }

    /// Deletes an installed pack's folder.
    pub fn remove(&self, prefix: &str) -> Result<(), PackError> {
        let dir = self.folder(prefix).ok_or(PackError::NotFound)?;
        if self.meta(prefix).is_none() {
            return Err(PackError::NotFound);
        }
        if self.installing.lock().unwrap().contains(prefix) {
            return Err(PackError::Busy);
        }
        self.loaded.lock().unwrap().remove(prefix);
        fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// The installed pack, parsed on first use.
    fn load(&self, prefix: &str) -> Option<Arc<Loaded>> {
        let mut loaded = self.loaded.lock().unwrap();
        if let Some(pack) = loaded.get(prefix) {
            return Some(pack.clone());
        }
        let meta = self.meta(prefix)?;
        let bytes = fs::read(self.folder(prefix)?.join(ICONS)).ok()?;
        let set = IconSet::parse(&bytes).ok()?;
        let names = set.names();
        let pack = Arc::new(Loaded { meta, set, names });
        loaded.insert(prefix.into(), pack.clone());
        Some(pack)
    }

    /// Ranked hits across the installed packs, or just `only`. Ranks match
    /// [`catalog::rank`]; ties keep pack, then name order. At most `limit` hits.
    pub fn search(&self, query: &str, only: Option<&str>, limit: usize) -> Vec<(u8, PackHit)> {
        let query = query.trim().to_lowercase();
        let packs: Vec<Arc<Loaded>> = self
            .installed()
            .into_keys()
            .filter(|prefix| only.is_none_or(|o| o == prefix))
            .filter_map(|prefix| self.load(&prefix))
            .collect();
        let mut ranked: Vec<(u8, usize, usize)> = Vec::new();
        for (p, pack) in packs.iter().enumerate() {
            for (i, named) in pack.names.iter().enumerate() {
                if let Some(rank) = named
                    .terms
                    .iter()
                    .filter_map(|t| catalog::rank_one(&query, t))
                    .min()
                {
                    ranked.push((rank, p, i));
                }
            }
        }
        ranked.sort_by_key(|(rank, _, _)| *rank);
        ranked.truncate(limit);
        ranked
            .into_iter()
            .map(|(rank, p, i)| {
                let pack = &packs[p];
                let named = &pack.names[i];
                (
                    rank,
                    PackHit {
                        prefix: pack.meta.prefix.clone(),
                        pack: pack.meta.name.clone(),
                        name: named.name.clone(),
                        aliases: named.aliases.clone(),
                        license: pack.meta.license.clone(),
                        author: pack.meta.author.clone(),
                        attribution: pack.meta.attribution,
                        brands: pack.meta.brands,
                        palette: pack.meta.palette,
                    },
                )
            })
            .collect()
    }

    /// A standalone SVG of one icon, monochrome icons inked in [`crate::icon_pack::INK`].
    pub fn svg(&self, prefix: &str, name: &str) -> Result<String, PackError> {
        let pack = self.load(prefix).ok_or(PackError::NotFound)?;
        pack.set
            .svg(name, crate::icon_pack::INK)
            .ok_or(PackError::NotFound)
    }

    /// Draws one icon into a Tile the way a catalog SVG is drawn.
    pub fn render(
        &self,
        prefix: &str,
        name: &str,
        options: catalog::Options,
    ) -> Result<RgbaImage, catalog::Error> {
        let svg = self
            .svg(prefix, name)
            .map_err(|_| catalog::Error::NotFound)?;
        catalog::fit(svg.as_bytes(), true, options)
    }
}

/// Marks a prefix as being installed until dropped.
struct Installing<'a> {
    set: &'a Mutex<HashSet<String>>,
    prefix: String,
}
impl<'a> Installing<'a> {
    fn start(set: &'a Mutex<HashSet<String>>, prefix: &str) -> Result<Self, PackError> {
        if !set.lock().unwrap().insert(prefix.into()) {
            return Err(PackError::Busy);
        }
        Ok(Self {
            set,
            prefix: prefix.into(),
        })
    }
}
impl Drop for Installing<'_> {
    fn drop(&mut self) {
        self.set.lock().unwrap().remove(&self.prefix);
    }
}

fn is_license(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.contains('/')
        && (lower.starts_with("license") || lower.starts_with("licence"))
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// Whether an archive path stays inside the folder it would unpack into, on Linux and
/// Windows alike: relative, no `..`, no drive or stream (`:`), no NUL.
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains([':', '\0'])
        && path.split(['/', '\\']).all(|part| part != "..")
}

/// The `path` record of a pax extended header, if it has one.
fn pax_path(data: &[u8]) -> Option<String> {
    let mut rest = data;
    let mut found = None;
    while !rest.is_empty() {
        let space = rest.iter().position(|b| *b == b' ')?;
        let len: usize = std::str::from_utf8(&rest[..space]).ok()?.parse().ok()?;
        if len <= space || len > rest.len() {
            return None;
        }
        let record = &rest[space + 1..len];
        let record = record.strip_suffix(b"\n").unwrap_or(record);
        if let Some(value) = record.strip_prefix(b"path=") {
            found = Some(String::from_utf8_lossy(value).into_owned());
        }
        rest = &rest[len..];
    }
    found
}

/// Reads the regular files `want` picks from a gzipped ustar archive, such as an npm tarball.
/// Refuses the whole archive if any entry is a link or has a path that could leave its folder.
fn untar(gz: &[u8], want: impl Fn(&str) -> bool) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut reader = flate2::read::GzDecoder::new(gz).take(MAX_UNPACKED);
    let mut out = Vec::new();
    let mut header = [0u8; 512];
    // A pax header or GNU long name names the entry after it.
    let mut long_path: Option<String> = None;
    loop {
        let mut filled = 0;
        while filled < header.len() {
            let n = reader
                .read(&mut header[filled..])
                .map_err(|e| format!("the archive is damaged: {e}"))?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 || header.iter().all(|b| *b == 0) {
            return Ok(out);
        }
        if filled < header.len() {
            return Err("the archive ends inside a header".into());
        }
        let text = |range: std::ops::Range<usize>| {
            let field = &header[range];
            let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
            String::from_utf8_lossy(&field[..end]).into_owned()
        };
        let size = u64::from_str_radix(text(124..136).trim_matches([' ', '\0']), 8)
            .map_err(|_| "the archive has a bad size field".to_owned())?;
        let name = text(0..100);
        let prefix = if &header[257..262] == b"ustar" {
            text(345..500)
        } else {
            String::new()
        };
        let flag = header[156];
        let padded = size.div_ceil(512) * 512;
        if matches!(flag, b'x' | b'L') {
            if size > MAX_HEADER {
                return Err("the archive has an oversized header".into());
            }
            let mut data = vec![0; padded as usize];
            reader
                .read_exact(&mut data)
                .map_err(|_| "the archive is cut short".to_owned())?;
            data.truncate(size as usize);
            if flag == b'L' {
                let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
                long_path = Some(String::from_utf8_lossy(&data[..end]).into_owned());
            } else if let Some(path) = pax_path(&data) {
                long_path = Some(path);
            }
            continue;
        }
        let path = match long_path.take() {
            Some(path) => path,
            None if prefix.is_empty() => name,
            None => format!("{prefix}/{name}"),
        };
        if !safe_path(&path) {
            return Err(format!("the archive has an unsafe path: {path}"));
        }
        if matches!(flag, b'1' | b'2' | b'K') {
            return Err(format!("the archive has a link: {path}"));
        }
        let regular = matches!(flag, b'0' | 0);
        if regular && want(&path) {
            if size > MAX_FILE {
                return Err(format!("{path} is too large"));
            }
            let mut data = vec![0; size as usize];
            reader
                .read_exact(&mut data)
                .map_err(|_| format!("{path} is cut short"))?;
            std::io::copy(&mut (&mut reader).take(padded - size), &mut std::io::sink())
                .map_err(|e| e.to_string())?;
            out.retain(|(p, _)| *p != path);
            out.push((path, data));
        } else {
            let skipped = std::io::copy(&mut (&mut reader).take(padded), &mut std::io::sink())
                .map_err(|e| e.to_string())?;
            if skipped < padded {
                return Err("the archive is cut short".into());
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    /// A gzipped ustar archive holding `files`.
    pub fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
        let entries: Vec<_> = files
            .iter()
            .map(|(name, data)| (*name, b'0', *data))
            .collect();
        archive(&entries)
    }

    /// A gzipped ustar archive of `(name, type flag, data)` entries.
    fn archive(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for (name, flag, data) in entries {
            let mut header = [0u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            header[100..108].copy_from_slice(b"0000644\0");
            header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
            header[156] = *flag;
            if *flag == b'2' {
                header[157..169].copy_from_slice(b"/etc/passwd\0");
            }
            header[257..263].copy_from_slice(b"ustar\0");
            header[148..156].copy_from_slice(b"        ");
            let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
            header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            tar.extend_from_slice(&header);
            tar.extend_from_slice(data);
            tar.resize(tar.len().div_ceil(512) * 512, 0);
        }
        tar.extend_from_slice(&[0; 1024]);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn curated_list_is_pinned_to_the_registry() {
        let list = curated();
        let prefixes: Vec<_> = list.iter().map(|c| c.prefix.as_str()).collect();
        for expected in [
            "logos",
            "devicon",
            "skill-icons",
            "mdi",
            "tabler",
            "lucide",
            "fa6-brands",
        ] {
            assert!(prefixes.contains(&expected), "{expected}");
        }
        assert!(!prefixes.contains(&"simple-icons"));
        for c in list {
            assert!(c.tarball.starts_with(REGISTRY), "{}", c.prefix);
            assert!(c.integrity.starts_with("sha512-"), "{}", c.prefix);
            assert_eq!(c.package, format!("@iconify-json/{}", c.prefix));
            assert!(c.unpacked_size < MAX_UNPACKED);
        }
        assert!(
            list.iter()
                .find(|c| c.prefix == "fa6-brands")
                .unwrap()
                .attribution
        );
    }

    pub const MONO: &str = r#"{"prefix":"mono","width":24,"height":24,"icons":{"spotify":{"body":"<circle cx=\"12\" cy=\"12\" r=\"10\" fill=\"currentColor\"/>"},"arrow":{"body":"<path fill=\"currentColor\" d=\"M0 0h12v24H0z\"/>"}},"aliases":{"music":{"parent":"spotify"},"arrow-right":{"parent":"arrow","hFlip":true}}}"#;

    struct Serve(Vec<u8>);
    impl Fetcher for Serve {
        fn fetch(&self, url: &str) -> Result<Vec<u8>, catalog::Error> {
            assert!(url.starts_with(REGISTRY), "{url}");
            Ok(self.0.clone())
        }
    }

    pub fn listed(prefix: &str, tarball: &[u8]) -> Curated {
        Curated {
            prefix: prefix.into(),
            name: "Mono".into(),
            package: format!("@iconify-json/{prefix}"),
            version: "1.0.0".into(),
            tarball: format!("{REGISTRY}@iconify-json/{prefix}/-/{prefix}-1.0.0.tgz"),
            integrity: integrity_of(tarball),
            unpacked_size: 1000,
            icons: 2,
            license: PackLicense {
                title: "MIT".into(),
                spdx: Some("MIT".into()),
                url: None,
            },
            author: PackAuthor {
                name: "Someone".into(),
                url: None,
            },
            attribution: false,
            brands: false,
            palette: false,
            samples: vec!["spotify".into()],
        }
    }

    #[test]
    fn install_search_render_and_remove_offline() {
        let dir = tempfile::tempdir().unwrap();
        let gz = tarball(&[
            ("package/icons.json", MONO.as_bytes()),
            ("package/info.json", b"{}"),
            ("package/index.js", b"x"),
            ("package/LICENSE.md", b"MIT"),
            ("package/license", b"also MIT"),
        ]);
        let packs = Packs::with_list(
            dir.path().join("packs"),
            Box::new(Serve(gz.clone())),
            vec![listed("mono", &gz)],
        );
        assert!(packs.search("spotify", None, 10).is_empty());
        let stages = Mutex::new(Vec::new());
        let info = packs
            .install("mono", &|s| stages.lock().unwrap().push(s), &|| false)
            .unwrap();
        assert_eq!(
            *stages.lock().unwrap(),
            [
                PackStage::Download,
                PackStage::Verify,
                PackStage::Extract,
                PackStage::Done
            ]
        );
        assert!(info.installed && info.icons == 3 && info.size_bytes.unwrap() > 0);
        let mut kept: Vec<_> = fs::read_dir(dir.path().join("packs/mono"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, ["LICENSE", "icons.json", "info.json", "pack.json"]);
        assert_eq!(
            fs::read(dir.path().join("packs").join("mono").join(LICENSE)).unwrap(),
            b"MIT"
        );

        // A fresh store finds the pack on disk and loads it on first search.
        let packs = Packs::with_list(
            dir.path().join("packs"),
            Box::new(Serve(gz.clone())),
            vec![listed("mono", &gz)],
        );
        let hits = packs.search("music", None, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (
                hits[0].0,
                hits[0].1.name.as_str(),
                hits[0].1.aliases.as_slice()
            ),
            (0, "spotify", &["music".to_owned()][..])
        );
        assert_eq!(
            packs
                .search("arrow", None, 10)
                .iter()
                .map(|h| h.0)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert_eq!(packs.search("arrow", None, 1).len(), 1);

        // currentColor draws in the built-in ink, visibly, on a transparent Tile.
        let image = packs
            .render("mono", "spotify", catalog::Options::default())
            .unwrap();
        assert_eq!(image.dimensions(), (64, 64));
        assert_eq!(image.get_pixel(32, 32).0, [0xf2, 0xf5, 0xfa, 255]);
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        // The flipped alias draws the bar on the right half.
        let flipped = packs
            .render("mono", "arrow-right", catalog::Options::default())
            .unwrap();
        assert_eq!(
            (
                flipped.get_pixel(48, 32).0[3],
                flipped.get_pixel(16, 32).0[3]
            ),
            (255, 0)
        );
        assert!(
            packs
                .svg("mono", "spotify")
                .unwrap()
                .contains("color=\"#f2f5fa\"")
        );
        assert!(matches!(
            packs.svg("mono", "missing"),
            Err(PackError::NotFound)
        ));

        packs.remove("mono").unwrap();
        assert!(!dir.path().join("packs/mono").exists());
        assert!(packs.search("spotify", None, 10).is_empty());
        assert!(!packs.list()[0].installed);
        assert!(matches!(packs.remove("mono"), Err(PackError::NotFound)));
    }

    #[test]
    fn a_tarball_that_fails_its_integrity_is_never_unpacked() {
        let dir = tempfile::tempdir().unwrap();
        let good = tarball(&[("package/icons.json", MONO.as_bytes())]);
        let evil = tarball(&[(
            "package/icons.json",
            MONO.replace("spotify", "evil").as_bytes(),
        )]);
        let packs = Packs::with_list(
            dir.path().to_owned(),
            Box::new(Serve(evil)),
            vec![listed("mono", &good)],
        );
        assert!(matches!(
            packs.install("mono", &|_| {}, &|| false),
            Err(PackError::Integrity)
        ));
        assert_eq!(fs::read_dir(dir.path()).map(|d| d.count()).unwrap_or(0), 0);
        assert!(matches!(
            packs.install("nope", &|_| {}, &|| false),
            Err(PackError::NotFound)
        ));
    }

    #[test]
    fn imports_validate_and_refuse_prefix_clashes() {
        let dir = tempfile::tempdir().unwrap();
        let gz = tarball(&[("package/icons.json", MONO.as_bytes())]);
        let packs = Packs::with_list(
            dir.path().to_owned(),
            Box::new(Serve(gz.clone())),
            vec![listed("mono", &gz)],
        );
        let mine = MONO.replace("\"mono\"", "\"mine\"");
        let info = packs.import(mine.as_bytes()).unwrap();
        assert_eq!(
            (info.origin, info.license.title.as_str(), info.attribution),
            (PackOrigin::Imported, CUSTOM_LICENSE, false)
        );
        assert!(matches!(packs.import(mine.as_bytes()), Err(PackError::Exists(p)) if p == "mine"));
        assert!(matches!(packs.import(MONO.as_bytes()), Err(PackError::Exists(p)) if p == "mono"));
        assert!(matches!(
            packs.import(b"{\"prefix\":\"x\"}"),
            Err(PackError::Invalid(_))
        ));
        // Prefixes are lowercase, so `Mine` can't share `mine`'s folder on Windows.
        assert!(matches!(
            packs.import(MONO.replace("\"mono\"", "\"Mine\"").as_bytes()),
            Err(PackError::Invalid(_))
        ));
        assert_eq!(
            packs
                .list()
                .iter()
                .map(|p| (p.prefix.as_str(), p.installed))
                .collect::<Vec<_>>(),
            [("mono", false), ("mine", true)]
        );
        assert_eq!(packs.search("spotify", None, 10)[0].1.prefix, "mine");
    }

    #[test]
    fn untar_keeps_only_wanted_files_and_refuses_damage() {
        let gz = tarball(&[
            ("package/icons.json", b"{}"),
            ("package/index.js", b"alert(1)"),
            ("package/LICENSE", b"MIT"),
        ]);
        let files = untar(&gz, |p| p != "package/index.js").unwrap();
        let names: Vec<_> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["package/icons.json", "package/LICENSE"]);
        assert_eq!(files[1].1, b"MIT");
        assert!(untar(&gz[..gz.len() / 2], |_| true).is_err());
        assert!(untar(b"not gzip", |_| true).is_err());
        assert!(is_license("LICENSE.txt") && is_license("license") && !is_license("../LICENSE"));
    }

    #[test]
    fn untar_refuses_entries_that_could_leave_the_pack_folder() {
        let icons: &[u8] = b"{}";
        let refused = |entries: &[(&str, u8, &[u8])]| {
            let mut all = vec![("package/icons.json", b'0', icons)];
            all.extend_from_slice(entries);
            untar(&archive(&all), |_| true).unwrap_err()
        };
        for path in [
            "../icons.json",
            "package/../../icons.json",
            "package\\..\\..\\icons.json",
            "/etc/icons.json",
            "\\\\server\\share\\icons.json",
            "C:/icons.json",
            "C:icons.json",
            "package/icons.json:stream",
        ] {
            let error = refused(&[(path, b'0', icons)]);
            assert!(error.contains("unsafe path"), "{path}: {error}");
        }
        // Links are refused even when the reader would skip them.
        for flag in [b'1', b'2'] {
            let error = refused(&[("package/LICENSE", flag, b"")]);
            assert!(error.contains("link"), "{error}");
        }
        // A GNU long name or a pax path names the next entry, and is checked too.
        let long: &[u8] = b"package/../../evil.json\0";
        let error = refused(&[
            ("././@LongLink", b'L', long),
            ("package/ok.json", b'0', icons),
        ]);
        assert!(error.contains("../../evil.json"), "{error}");
        let pax: &[u8] = b"32 path=package/../../evil.json\n";
        let error = refused(&[("PaxHeader", b'x', pax), ("package/ok.json", b'0', icons)]);
        assert!(error.contains("../../evil.json"), "{error}");
        // A safe pax path is used as the entry's name.
        let pax: &[u8] = b"27 path=package/icons.json\n";
        let files = untar(
            &archive(&[("PaxHeader", b'x', pax), ("short", b'0', icons)]),
            |_| true,
        )
        .unwrap();
        assert_eq!(files[0].0, "package/icons.json");
    }

    #[test]
    fn a_pack_with_a_slipping_entry_is_never_installed() {
        let dir = tempfile::tempdir().unwrap();
        let gz = archive(&[
            ("package/icons.json", b'0', MONO.as_bytes()),
            ("package/../../escaped.json", b'0', b"{}"),
        ]);
        let packs = Packs::with_list(
            dir.path().join("packs"),
            Box::new(Serve(gz.clone())),
            vec![listed("mono", &gz)],
        );
        assert!(matches!(
            packs.install("mono", &|_| {}, &|| false),
            Err(PackError::Invalid(e)) if e.contains("unsafe path")
        ));
        assert!(!dir.path().join("packs").exists());
        assert!(!dir.path().join("escaped.json").exists());
        assert!(!packs.list()[0].installed);
    }
}
