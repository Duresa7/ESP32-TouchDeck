//! What the Companion keeps on disk under its data folder: each Device's Layouts and the Asset
//! files it prepared.
use std::path::PathBuf;
use td_core::{Layout, Orientation};

/// Each Device's Layouts by Device id, as `layouts/<device>/<orientation>.json` in canonical
/// form. A Layout lands here once the Companion pushes it to the Device or pulls it from it.
#[derive(Clone)]
pub struct LayoutStore {
    dir: PathBuf,
}
impl LayoutStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    fn path(&self, device: &str, orientation: Orientation) -> Option<PathBuf> {
        // Device ids are lowercase letters, digits and `-`, so they never leave the folder.
        let safe = !device.is_empty()
            && device
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        safe.then(|| {
            self.dir
                .join(device)
                .join(format!("{}.json", name(orientation)))
        })
    }
    /// Blocking.
    pub fn put(&self, device: &str, layout: &Layout) -> std::io::Result<()> {
        let path = self
            .path(device, layout.orientation)
            .ok_or_else(|| std::io::Error::other("Invalid Device id."))?;
        td_core::app_dirs::write_atomic(&path, &layout.to_canonical_json())
    }
    /// Blocking. A missing or unreadable file reads as no Layout.
    pub fn get(&self, device: &str, orientation: Orientation) -> Option<Layout> {
        let bytes = std::fs::read(self.path(device, orientation)?).ok()?;
        Layout::parse(&bytes).ok()
    }
    /// Blocking. Every Layout stored for the Device.
    pub fn all(&self, device: &str) -> Vec<Layout> {
        [Orientation::Landscape, Orientation::Portrait]
            .into_iter()
            .filter_map(|o| self.get(device, o))
            .collect()
    }
}
pub fn name(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::Landscape => "landscape",
        Orientation::Portrait => "portrait",
    }
}

/// Asset files: the built-in icons and the ones taken from programs, kept as
/// `assets/<id>.bin`, the name the Device stores them under.
#[derive(Clone)]
pub struct AssetStore {
    cache: td_host::icons::IconCache,
    dir: PathBuf,
}
impl AssetStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self {
            cache: td_host::icons::IconCache::new(dir.clone()),
            dir,
        }
    }
    /// Blocking. The file for an id, from the built-in set or the store.
    pub fn get(&self, id: &str) -> Option<Vec<u8>> {
        if td_link::asset::valid_asset_id(id) {
            let bytes = std::fs::read(self.dir.join(format!("{id}.bin"))).ok()?;
            return (td_link::asset::asset_id(&bytes) == id).then_some(bytes);
        }
        self.cache.get(id)
    }
    /// Blocking. Stores a `TDI1` file and returns its id.
    pub fn put(&self, file: &[u8]) -> Result<String, String> {
        if !td_host::icon_pack::is_icon(file) {
            return Err("Invalid tdi1 Asset".into());
        }
        let id = td_link::asset::asset_id(file);
        td_core::app_dirs::write_atomic(&self.dir.join(format!("{id}.bin")), file)
            .map_err(|e| e.to_string())?;
        Ok(id)
    }
}
