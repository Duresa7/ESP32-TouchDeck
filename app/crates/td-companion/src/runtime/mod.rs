//! The runtime folder: readable only by the user, holding the single-instance lock and
//! `companion.json`.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

use parking_lot::Mutex;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

/// Waits between attempts when a check or rewrite fails, such as while an antivirus scanner
/// holds the file.
const RETRIES: [Duration; 3] = [
    Duration::from_millis(200),
    Duration::from_secs(1),
    Duration::from_secs(3),
];

pub(crate) struct Runtime {
    file: PathBuf,
    /// Held for the Companion's lifetime. A second Companion for this folder can't take it.
    _lock: File,
    /// Serializes the start, the 60 s check and Reset pairing.
    write: Mutex<()>,
    #[cfg(test)]
    writes: std::sync::atomic::AtomicUsize,
}
impl Runtime {
    /// Secures `dir`, then takes the single-instance lock inside it. The folder is the user's
    /// alone, so another account can't create the lock first.
    pub(crate) fn open(dir: &Path) -> Result<Self, String> {
        platform::secure_dir(dir).map_err(|e| format!("The runtime folder can't be used: {e}"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("companion.lock"))
            .map_err(|e| format!("The runtime folder can't be used: {e}"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| "The Companion is already running.".to_owned())?;
        Ok(Self {
            file: dir.join("companion.json"),
            _lock: lock,
            write: Mutex::new(()),
            #[cfg(test)]
            writes: Default::default(),
        })
    }
    /// Checks that the file holds `value` with owner-only security, and rewrites it only when
    /// it has drifted. Retries before giving up. Blocks, so call it off the async runtime.
    pub(crate) fn repair(&self, value: &serde_json::Value) -> io::Result<()> {
        let _held = self.write.lock();
        let bytes = serde_json::to_vec(value)?;
        let mut result = Ok(());
        for wait in std::iter::once(Duration::ZERO).chain(RETRIES) {
            std::thread::sleep(wait);
            let intact = std::fs::read(&self.file).is_ok_and(|b| b == bytes)
                && platform::verify_file(&self.file).is_ok();
            if intact {
                return Ok(());
            }
            result = platform::write_file(&self.file, &bytes);
            if result.is_ok() {
                #[cfg(test)]
                self.writes
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                return result;
            }
        }
        result
    }
    /// Deletes the file on clean shutdown.
    pub(crate) fn remove(&self) {
        let _held = self.write.lock();
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    fn open() -> (tempfile::TempDir, Runtime) {
        let root = tempfile::tempdir().unwrap();
        let runtime = Runtime::open(&root.path().join("runtime")).unwrap();
        (root, runtime)
    }

    #[test]
    fn an_intact_file_is_left_alone_and_drift_is_repaired() {
        let (_root, runtime) = open();
        let value = json!({ "token": "tdp_a" });
        runtime.repair(&value).unwrap();
        runtime.repair(&value).unwrap();
        assert_eq!(runtime.writes.load(Ordering::SeqCst), 1);
        std::fs::remove_file(&runtime.file).unwrap();
        runtime.repair(&value).unwrap();
        assert_eq!(
            std::fs::read(&runtime.file).unwrap(),
            b"{\"token\":\"tdp_a\"}"
        );
        std::fs::write(&runtime.file, b"{}").unwrap();
        runtime.repair(&value).unwrap();
        assert_eq!(
            std::fs::read(&runtime.file).unwrap(),
            b"{\"token\":\"tdp_a\"}"
        );
        assert_eq!(runtime.writes.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn a_second_lock_on_the_same_folder_fails() {
        let (root, _runtime) = open();
        let second = Runtime::open(&root.path().join("runtime"));
        assert_eq!(second.err().unwrap(), "The Companion is already running.");
        let (_other, _elsewhere) = open();
    }

    /// A scanner holding the file makes the rewrite fail for a moment. The old file stays,
    /// and a retry succeeds once the handle is gone.
    #[cfg(windows)]
    #[test]
    fn a_held_file_is_kept_and_retried() {
        use std::os::windows::fs::OpenOptionsExt;
        let (_root, runtime) = open();
        runtime.repair(&json!({ "token": "tdp_old" })).unwrap();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&runtime.file)
            .unwrap();
        let path = runtime.file.clone();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            assert!(
                std::fs::metadata(&path).is_ok(),
                "the old file must survive a failed rewrite"
            );
            drop(held);
        });
        runtime.repair(&json!({ "token": "tdp_new" })).unwrap();
        release.join().unwrap();
        assert_eq!(
            std::fs::read(&runtime.file).unwrap(),
            b"{\"token\":\"tdp_new\"}"
        );
    }
}
