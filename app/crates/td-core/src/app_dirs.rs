//! The folders the app and `td-statusline` keep source config and status line state in.
//!
//! They are Tauri's identifier folders, resolved the way Tauri resolves them, so the
//! installer's "Delete app data" removes them and nothing lands in the install folder:
//! `<config dir>/io.github.duresa7.touchdeck` for `sources.json` and
//! `<local data dir>/io.github.duresa7.touchdeck/claude-status` for the session files.
//! Releases before this kept them in `<config dir>/touchdeck` and `<state dir>/touchdeck`,
//! which on Windows is the per-user install folder; [`Dirs::migrate`] moves them over.

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
    /// Holds `claude-status/`.
    pub local_data: PathBuf,
    /// The old `sources.json`, when a migration may read it.
    pub legacy_config: Option<PathBuf>,
    /// The old `claude-status` folder, when a migration may read it.
    pub legacy_status: Option<PathBuf>,
}

impl Dirs {
    /// The folders for this process: `TOUCHDECK_CONFIG_DIR` and `TOUCHDECK_STATE_DIR` stand
    /// in for the user's config and local data folders, and `TOUCHDECK_APP_DATA_DIR`, the
    /// app's scratch override, holds both as `config` and `state` when they are unset.
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
            dirs::data_local_dir(),
            legacy_home("APPDATA", "XDG_CONFIG_HOME", ".config"),
            legacy_home("LOCALAPPDATA", "XDG_STATE_HOME", ".local/state"),
        )
    }

    /// `config_home` and `local_home` are Tauri's `config_dir` and `local_data_dir`;
    /// `legacy_config_home` and `legacy_state_home` are what releases before used for them.
    pub fn resolve(
        env: impl Fn(&str) -> Option<PathBuf>,
        config_home: Option<PathBuf>,
        local_home: Option<PathBuf>,
        legacy_config_home: Option<PathBuf>,
        legacy_state_home: Option<PathBuf>,
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
        let (local_data, legacy_state) = pick(
            "TOUCHDECK_STATE_DIR",
            "state",
            local_home,
            legacy_state_home,
        );
        Dirs {
            config,
            local_data,
            legacy_config: legacy_config.map(|dir| dir.join("sources.json")),
            legacy_status: legacy_state.map(|dir| dir.join("claude-status")),
        }
    }

    pub fn sources_config(&self) -> PathBuf {
        self.config.join("sources.json")
    }

    pub fn claude_status(&self) -> PathBuf {
        self.local_data.join("claude-status")
    }

    /// Moves the old `sources.json` and status line files into the new folders, once, and
    /// removes the old folders when that leaves them empty. Returns what went wrong.
    pub fn migrate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(old) = &self.legacy_config
            && let Err(error) = migrate_file(old, &self.sources_config())
        {
            problems.push(format!("{}: {error}", old.display()));
        }
        if let Some(old) = &self.legacy_status
            && let Err(error) = migrate_dir(old, &self.claude_status())
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

/// Moves every file of `old` into `new` that `new` lacks, drops the old copies `new` already
/// has a newer version of, and removes `old`, and its parent, if that left them empty.
/// Returns how many files it moved.
pub fn migrate_dir(old: &Path, new: &Path) -> io::Result<usize> {
    if !old.is_dir() {
        return Ok(0);
    }
    fs::create_dir_all(new)?;
    let mut moved = 0;
    for entry in fs::read_dir(old)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let target = new.join(entry.file_name());
        if target.exists() {
            fs::remove_file(entry.path())?;
        } else {
            move_file(&entry.path(), &target)?;
            moved += 1;
        }
    }
    remove_if_empty(Some(old));
    remove_if_empty(old.parent());
    Ok(moved)
}

fn move_file(old: &Path, new: &Path) -> io::Result<()> {
    fs::rename(old, new).or_else(|_| {
        fs::copy(old, new)?;
        fs::remove_file(old)
    })
}

/// Removes an empty folder named `touchdeck` or `claude-status`. It never removes a folder
/// with anything in it, so the install folder, which shares the old name on Windows, stays.
fn remove_if_empty(dir: Option<&Path>) {
    if let Some(dir) = dir.filter(|dir| {
        dir.file_name().is_some_and(|name| {
            name == OsStr::new("touchdeck") || name == OsStr::new("claude-status")
        })
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
            Some("/local".into()),
            Some("/old-roaming".into()),
            Some("/old-local".into()),
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
            dirs.claude_status(),
            Path::new("/local").join(APP_ID).join("claude-status")
        );
        assert_eq!(
            dirs.legacy_config,
            Some(Path::new("/old-roaming/touchdeck/sources.json").into())
        );
        assert_eq!(
            dirs.legacy_status,
            Some(Path::new("/old-local/touchdeck/claude-status").into())
        );
    }

    #[test]
    fn overrides_win_and_a_scratch_run_never_reaches_the_real_folders() {
        let dirs = resolve(&[
            ("TOUCHDECK_CONFIG_DIR", "/c"),
            ("TOUCHDECK_STATE_DIR", "/s"),
            ("TOUCHDECK_APP_DATA_DIR", "/a"),
        ]);
        assert_eq!(
            dirs.sources_config(),
            Path::new("/c").join(APP_ID).join("sources.json")
        );
        assert_eq!(
            dirs.claude_status(),
            Path::new("/s").join(APP_ID).join("claude-status")
        );
        assert_eq!(
            dirs.legacy_config,
            Some(Path::new("/c/touchdeck/sources.json").into())
        );

        let dirs = resolve(&[("TOUCHDECK_APP_DATA_DIR", "/a")]);
        assert_eq!(dirs.sources_config(), Path::new("/a/config/sources.json"));
        assert_eq!(dirs.claude_status(), Path::new("/a/state/claude-status"));
        assert_eq!((dirs.legacy_config, dirs.legacy_status), (None, None));
    }

    #[test]
    fn migration_moves_the_old_config_and_status_files_once() {
        let root = scratch("migrate");
        let (old_roaming, old_local) =
            (root.join("roaming/touchdeck"), root.join("local/touchdeck"));
        fs::create_dir_all(old_local.join("claude-status")).unwrap();
        fs::create_dir_all(&old_roaming).unwrap();
        // The old local folder is also the install folder on Windows, so it keeps a file.
        fs::write(old_local.join("touchdeck.exe"), "app").unwrap();
        fs::write(old_roaming.join("sources.json"), "{\"cliproxy\":true}\n").unwrap();
        fs::write(old_local.join("claude-status/a.json"), "old a").unwrap();
        fs::write(old_local.join("claude-status/b.json"), "old b").unwrap();
        let dirs = Dirs {
            config: root.join("new-roaming").join(APP_ID),
            local_data: root.join("new-local").join(APP_ID),
            legacy_config: Some(old_roaming.join("sources.json")),
            legacy_status: Some(old_local.join("claude-status")),
        };
        // The shim already wrote b again in the new folder before the app started.
        fs::create_dir_all(dirs.claude_status()).unwrap();
        fs::write(dirs.claude_status().join("b.json"), "new b").unwrap();

        assert!(dirs.migrate().is_empty());
        assert_eq!(
            fs::read_to_string(dirs.sources_config()).unwrap(),
            "{\"cliproxy\":true}\n"
        );
        assert_eq!(
            fs::read_to_string(dirs.claude_status().join("a.json")).unwrap(),
            "old a"
        );
        assert_eq!(
            fs::read_to_string(dirs.claude_status().join("b.json")).unwrap(),
            "new b"
        );
        assert!(!old_roaming.exists(), "the emptied old config folder goes");
        assert!(!old_local.join("claude-status").exists());
        assert!(
            old_local.join("touchdeck.exe").is_file(),
            "a folder with files stays"
        );

        // A second start finds nothing to move and keeps the new files.
        fs::create_dir_all(&old_roaming).unwrap();
        fs::write(old_roaming.join("sources.json"), "stale").unwrap();
        assert!(!migrate_file(&old_roaming.join("sources.json"), &dirs.sources_config()).unwrap());
        assert_eq!(
            fs::read_to_string(dirs.sources_config()).unwrap(),
            "{\"cliproxy\":true}\n"
        );
        assert_eq!(
            migrate_dir(&old_local.join("claude-status"), &dirs.claude_status()).unwrap(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }
}
