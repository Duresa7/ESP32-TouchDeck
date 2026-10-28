//! `devices.json`: every known Device with its id, name and setup state.
use super::{DeviceId, SetupState};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub(super) struct Saved {
    pub id: DeviceId,
    pub name: Option<String>,
    pub setup: SetupState,
}
#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    devices: Vec<Saved>,
}

/// The saved Devices. A missing or unreadable file means none.
pub(super) fn load(path: &Path) -> Vec<Saved> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<File>(&bytes).ok())
        .filter(|file| file.version == 1)
        .map(|file| file.devices)
        .unwrap_or_default()
}

/// Writes the file whole through a temporary file, so a crash leaves the old one.
pub(super) fn save(path: &Path, devices: &[Saved]) -> std::io::Result<()> {
    #[derive(Serialize)]
    struct Out<'a> {
        version: u32,
        devices: &'a [Saved],
    }
    let bytes = serde_json::to_vec_pretty(&Out {
        version: 1,
        devices,
    })?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_devices_load_back_and_a_bad_file_loads_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("devices.json");
        assert!(load(&path).is_empty());
        let saved = Saved {
            id: DeviceId::parse("a0b1c2d3e4f5").unwrap(),
            name: Some("Desk".into()),
            setup: SetupState::Ready,
        };
        save(&path, &[saved]).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "a0b1c2d3e4f5");
        assert_eq!(loaded[0].name.as_deref(), Some("Desk"));
        std::fs::write(&path, br#"{"version":1,"devices":[{"id":"BAD!"}]}"#).unwrap();
        assert!(load(&path).is_empty());
    }
}
