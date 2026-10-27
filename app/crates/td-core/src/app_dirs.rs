//! Source config in Tauri's identifier folder, including the old config migration.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The bundle identifier in `tauri.conf.json`, which names the app's data folders.
pub const APP_ID: &str = "io.github.duresa7.touchdeck";

pub(crate) fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Writes through a temporary file in the same directory and renames it into place.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })
}

/// Where the sources keep their files, and where releases before kept them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// Holds `sources.json`.
    pub config: PathBuf,
    /// The old `sources.json`, when a migration may read it.
    pub legacy_config: Option<PathBuf>,
}

impl Dirs {
    /// `TOUCHDECK_CONFIG_DIR` overrides config; otherwise a scratch app-data override
    /// holds it under `config` without reading or moving the user's real files.
    pub fn current() -> Dirs {
        let legacy_home = |windows: &str, xdg: &str, under_home: &str| {
            if cfg!(windows) {
                env_dir(windows)
            } else {
                env_dir(xdg).or_else(|| {
                    env_dir("HOME")
                        .or_else(|| env_dir("USERPROFILE"))
                        .map(|home| home.join(under_home))
                })
            }
        };
        Dirs::resolve(
            env_dir,
            dirs::config_dir(),
            legacy_home("APPDATA", "XDG_CONFIG_HOME", ".config"),
        )
    }

    /// Resolves Tauri's config folder and the previous source-config location.
    pub fn resolve(
        env: impl Fn(&str) -> Option<PathBuf>,
        config_home: Option<PathBuf>,
        legacy_config_home: Option<PathBuf>,
    ) -> Dirs {
        let scratch = env("TOUCHDECK_APP_DATA_DIR");
        let pick = |var: &str, sub: &str, home: Option<PathBuf>, legacy_home: Option<PathBuf>| {
            if let Some(base) = env(var) {
                (base.join(APP_ID), Some(base.join("touchdeck")))
            } else if let Some(base) = &scratch {
                // A scratch run never reads, or moves, the user's real files.
                (base.join(sub), None)
            } else {
                (
                    home.unwrap_or_else(std::env::temp_dir).join(APP_ID),
                    legacy_home.map(|home| home.join("touchdeck")),
                )
            }
        };
        let (config, legacy_config) = pick(
            "TOUCHDECK_CONFIG_DIR",
            "config",
            config_home,
            legacy_config_home,
        );
        Dirs {
            config,
            legacy_config: legacy_config.map(|dir| dir.join("sources.json")),
        }
    }

    pub fn sources_config(&self) -> PathBuf {
        self.config.join("sources.json")
    }

    /// Moves the old `sources.json` into the new folder, once, and
    /// removes the old folders when that leaves them empty. Returns what went wrong.
    pub fn migrate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(old) = &self.legacy_config
            && let Err(error) = migrate_file(old, &self.sources_config())
        {
            problems.push(format!("{}: {error}", old.display()));
        }
        problems
    }
}

/// Moves `old` to `new` when `old` exists and `new` does not, then removes `old`'s folder if
/// that left it empty. Returns whether it moved the file.
pub fn migrate_file(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_file() || new.exists() {
        return Ok(false);
    }
    if let Some(dir) = new.parent() {
        fs::create_dir_all(dir)?;
    }
    move_file(old, new)?;
    remove_if_empty(old.parent());
    Ok(true)
}

fn move_file(old: &Path, new: &Path) -> io::Result<()> {
    fs::rename(old, new).or_else(|_| {
        fs::copy(old, new)?;
        fs::remove_file(old)
    })
}

/// Removes an empty folder named `touchdeck`. It never removes a folder
/// with anything in it, so the install folder, which shares the old name on Windows, stays.
fn remove_if_empty(dir: Option<&Path>) {
    if let Some(dir) = dir.filter(|dir| {
        dir.file_name()
            .is_some_and(|name| name == OsStr::new("touchdeck"))
    }) {
        let _ = fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn scratch(name: &str) -> PathBuf {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "td-core-dirs-{name}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn resolve(vars: &[(&str, &str)]) -> Dirs {
        let vars: HashMap<String, PathBuf> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), PathBuf::from(value)))
            .collect();
        Dirs::resolve(
            |name| vars.get(name).cloned(),
            Some("/roaming".into()),
            Some("/old-roaming".into()),
        )
    }

    #[test]
    fn defaults_are_the_tauri_identifier_folders() {
        let dirs = resolve(&[]);
        assert_eq!(
            dirs.sources_config(),
            Path::new("/roaming").join(APP_ID).join("sources.json")
        );
        assert_eq!(
            dirs.legacy_config,
            Some(Path::new("/old-roaming/touchdeck/sources.json").into())
        );
    }

    #[test]
    fn overrides_win_and_a_scratch_run_never_reaches_the_real_folders() {
        let dirs = resolve(&[
            ("TOUCHDECK_CONFIG_DIR", "/c"),
            ("TOUCHDECK_APP_DATA_DIR", "/a"),
        ]);
        assert_eq!(
            dirs.sources_config(),
            Path::new("/c").join(APP_ID).join("sources.json")
        );
        assert_eq!(
            dirs.legacy_config,
            Some(Path::new("/c/touchdeck/sources.json").into())
        );

        let dirs = resolve(&[("TOUCHDECK_APP_DATA_DIR", "/a")]);
        assert_eq!(dirs.sources_config(), Path::new("/a/config/sources.json"));
        assert_eq!(dirs.legacy_config, None);
    }

    #[test]
    fn migration_moves_the_old_config_once_and_keeps_nonempty_folders() {
        let root = scratch("migrate");
        let old = root.join("touchdeck");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("touchdeck.exe"), "app").unwrap();
        fs::write(old.join("sources.json"), "{\"cliproxy\":true}\n").unwrap();
        let dirs = Dirs {
            config: root.join(APP_ID),
            legacy_config: Some(old.join("sources.json")),
        };
        assert!(dirs.migrate().is_empty());
        assert_eq!(
            fs::read_to_string(dirs.sources_config()).unwrap(),
            "{\"cliproxy\":true}\n"
        );
        assert!(old.join("touchdeck.exe").is_file());
        fs::write(old.join("sources.json"), "stale").unwrap();
        assert!(dirs.migrate().is_empty());
        assert_eq!(
            fs::read_to_string(dirs.sources_config()).unwrap(),
            "{\"cliproxy\":true}\n"
        );
        fs::remove_file(old.join("sources.json")).unwrap();
        fs::remove_file(old.join("touchdeck.exe")).unwrap();
        fs::remove_file(dirs.sources_config()).unwrap();
        fs::write(old.join("sources.json"), "{}").unwrap();
        assert!(dirs.migrate().is_empty());
        assert!(!old.exists(), "an emptied old config folder goes");
        fs::remove_dir_all(root).unwrap();
    }
}
