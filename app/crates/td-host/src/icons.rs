//! Icons for tiles: the built-in set, icons taken from apps, and the app's icon cache.
//!
//! An icon is a `TDI1` file from [`icon_pack`](crate::icon_pack), and its id is the first 16 hex
//! digits of the file's SHA-256. A tile names it as `file:<id>`, and the cache keeps the file
//! as `<id>.bin`, the name the deck stores it under.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use td_core::Layout;
use td_core::layout::Page;

use crate::icon_pack::{self, SIDE};

const BUILTIN_FILES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/builtin_icons.rs"));

/// One entry of `app/icons/builtin/index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuiltinEntry {
    pub name: String,
    pub file: String,
    pub label: String,
    pub tags: Vec<String>,
    pub source: String,
    pub license: String,
}

#[derive(Debug, Deserialize)]
struct Index {
    icons: Vec<BuiltinEntry>,
}

/// A built-in icon rendered into the deck's format.
#[derive(Debug, Clone, Serialize)]
pub struct Builtin {
    pub name: String,
    pub label: String,
    pub tags: Vec<String>,
    pub id: String,
    #[serde(skip)]
    pub file: Vec<u8>,
}

/// The SVG source of a built-in file, by its file name.
pub fn builtin_source(file: &str) -> Option<&'static [u8]> {
    BUILTIN_FILES
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
}

/// The SVG file names the build embedded.
pub fn builtin_svg_names() -> Vec<&'static str> {
    BUILTIN_FILES
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| name.ends_with(".svg"))
        .collect()
}

pub fn builtin_index() -> Result<Vec<BuiltinEntry>, String> {
    let index = builtin_source("index.json").ok_or("index.json is missing")?;
    serde_json::from_slice::<Index>(index)
        .map(|index| index.icons)
        .map_err(|error| format!("index.json: {error}"))
}

fn render_builtin(entry: &BuiltinEntry) -> Result<Builtin, String> {
    let svg = builtin_source(&entry.file).ok_or_else(|| format!("{} is missing", entry.file))?;
    let image =
        icon_pack::render_svg(svg, SIDE).map_err(|error| format!("{}: {error}", entry.file))?;
    let file = icon_pack::pack(&image);
    Ok(Builtin {
        name: entry.name.clone(),
        label: entry.label.clone(),
        tags: entry.tags.clone(),
        id: icon_pack::icon_id(&file),
        file,
    })
}

/// The built-in set, rendered once on first use.
pub fn builtin() -> &'static [Builtin] {
    static SET: OnceLock<Vec<Builtin>> = OnceLock::new();
    SET.get_or_init(|| {
        builtin_index()
            .expect("the embedded index.json parses")
            .iter()
            .map(|entry| render_builtin(entry).expect("every built-in SVG renders"))
            .collect()
    })
}

/// The built-in icon a tile's action suggests: a browser or folder for `open` by its
/// target, a terminal for `command`, and a widget's source glyph.
pub fn default_for(action: &serde_json::Value) -> Option<&'static str> {
    let field = |key: &str| action.get(key).and_then(serde_json::Value::as_str);
    match field("type")? {
        "open" => {
            let target = field("target").unwrap_or_default();
            let web = ["http://", "https://"]
                .iter()
                .any(|scheme| target.to_ascii_lowercase().starts_with(scheme));
            Some(if web { "browser" } else { "folder" })
        }
        "command" => Some("terminal"),
        "widget" => match field("source")? {
            "cliproxyapi" => Some("server"),
            _ => Some("gauge"),
        },
        _ => None,
    }
}

/// Every `file:` id the layout names.
pub fn file_ids(layout: &Layout) -> BTreeSet<String> {
    layout
        .pages
        .iter()
        .filter_map(|page| match page {
            Page::Grid { tiles, .. } => Some(tiles),
            _ => None,
        })
        .flatten()
        .filter_map(|tile| tile.icon.as_deref()?.strip_prefix("file:"))
        .map(str::to_owned)
        .collect()
}

/// The app's icon files, kept as `<dir>/<id>.bin`. Built-in icons need no file there.
#[derive(Debug, Clone)]
pub struct IconCache {
    dir: PathBuf,
}

impl IconCache {
    pub fn new(dir: PathBuf) -> IconCache {
        IconCache { dir }
    }

    /// Stores an icon file and returns its id.
    pub fn put(&self, file: &[u8]) -> Result<String, String> {
        if !icon_pack::is_icon(file) {
            return Err("not a TDI1 icon file".into());
        }
        let id = icon_pack::icon_id(file);
        std::fs::create_dir_all(&self.dir)
            .map_err(|error| format!("{}: {error}", self.dir.display()))?;
        let path = self.dir.join(format!("{id}.bin"));
        std::fs::write(&path, file).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(id)
    }

    /// The file for an id, from the built-in set or the cache. A cached file whose hash is
    /// not its id is ignored.
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        if let Some(icon) = builtin().iter().find(|icon| icon.id == id) {
            return Some(icon.file.clone());
        }
        if id.len() != 16
            || !id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        let file = std::fs::read(self.dir.join(format!("{id}.bin"))).ok()?;
        (icon_pack::is_icon(&file) && icon_pack::icon_id(&file) == id).then_some(file)
    }

    /// Takes the icon of the app at `path`, packs it, stores it, and returns its id.
    pub fn take_app_icon(&self, path: &Path) -> Result<String, String> {
        let image = app_icon(path)?;
        self.put(&icon_pack::pack_any(&image))
    }
}

/// The icon of the app at `path` as the desktop shows it, as an RGBA image of any size.
pub fn app_icon(path: &Path) -> Result<image::RgbaImage, String> {
    #[cfg(windows)]
    return crate::icons_windows::app_icon(path);
    #[cfg(target_os = "linux")]
    return crate::icons_linux::app_icon(path);
    #[cfg(not(any(windows, target_os = "linux")))]
    Err(format!(
        "{}: app icons need Windows or Linux",
        path.display()
    ))
}
