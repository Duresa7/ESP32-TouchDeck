//! Command approval. A `command` tile runs only after the operator approves its exact program,
//! arguments, directory, and shell flag. Any change to one of them makes a new hash, which asks again.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How many `once` records the file keeps, newest last. `always` records are never trimmed.
const ONCE_KEPT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub shell: bool,
}

impl CommandSpec {
    /// The SHA-256 of the spec's compact JSON, `{program, args, cwd, shell}` in that order.
    pub fn hash(&self) -> String {
        let digest = Sha256::digest(serde_json::to_vec(self).expect("specs serialize"));
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grant {
    Once,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub hash: String,
    pub grant: Grant,
    /// Seconds since the Unix epoch.
    pub at: u64,
}

/// The approvals in `approvals.json`, or in memory only when there is no path.
#[derive(Debug, Default)]
pub struct Approvals {
    path: Option<PathBuf>,
    records: Vec<Record>,
}

impl Approvals {
    pub fn in_memory() -> Approvals {
        Approvals::default()
    }

    /// Reads `path`. A missing file is an empty list. An unreadable one is an error, so a damaged
    /// file never silently drops an approval or grants one.
    pub fn load(path: PathBuf) -> io::Result<Approvals> {
        let records = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Approvals {
            path: Some(path),
            records,
        })
    }

    pub fn is_always(&self, hash: &str) -> bool {
        self.records
            .iter()
            .any(|record| record.grant == Grant::Always && record.hash == hash)
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// Records a grant and saves the file through a temporary file and a rename.
    pub fn grant(&mut self, hash: &str, grant: Grant) -> io::Result<()> {
        if grant == Grant::Always && self.is_always(hash) {
            return Ok(());
        }
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        self.records.push(Record {
            hash: hash.to_owned(),
            grant,
            at,
        });
        let once = self
            .records
            .iter()
            .filter(|record| record.grant == Grant::Once)
            .count();
        if once > ONCE_KEPT {
            let mut extra = once - ONCE_KEPT;
            self.records.retain(|record| {
                let drop = extra > 0 && record.grant == Grant::Once;
                extra -= usize::from(drop);
                !drop
            });
        }
        self.save()
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(&self.records).expect("records serialize");
        bytes.push(b'\n');
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo() -> CommandSpec {
        CommandSpec {
            program: "cmd".into(),
            args: vec!["/c".into(), "echo".into(), "TouchDeck".into()],
            cwd: String::new(),
            shell: false,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "td-host-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("approvals.json")
    }

    #[test]
    fn once_runs_without_remembering() {
        let mut approvals = Approvals::in_memory();
        approvals.grant(&echo().hash(), Grant::Once).unwrap();
        assert!(!approvals.is_always(&echo().hash()));
        assert_eq!(approvals.records()[0].grant, Grant::Once);
    }

    #[test]
    fn always_is_remembered_across_a_reload() {
        let path = scratch("always");
        let mut approvals = Approvals::load(path.clone()).unwrap();
        approvals.grant(&echo().hash(), Grant::Always).unwrap();
        let reloaded = Approvals::load(path.clone()).unwrap();
        assert!(reloaded.is_always(&echo().hash()));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_changed_argument_asks_again() {
        let mut approvals = Approvals::in_memory();
        approvals.grant(&echo().hash(), Grant::Always).unwrap();
        let mut changed = echo();
        changed.args[2] = "TouchDeck!".into();
        assert_ne!(changed.hash(), echo().hash());
        assert!(!approvals.is_always(&changed.hash()));
    }

    #[test]
    fn a_changed_cwd_or_shell_flag_asks_again() {
        let mut approvals = Approvals::in_memory();
        approvals.grant(&echo().hash(), Grant::Always).unwrap();
        let mut moved = echo();
        moved.cwd = "C:\\".into();
        let mut shelled = echo();
        shelled.shell = true;
        assert!(!approvals.is_always(&moved.hash()));
        assert!(!approvals.is_always(&shelled.hash()));
    }

    #[test]
    fn arguments_cannot_be_regrouped_into_the_same_hash() {
        let split = echo();
        let mut joined = echo();
        joined.args = vec!["/c".into(), "echo TouchDeck".into()];
        assert_ne!(split.hash(), joined.hash());
    }

    #[test]
    fn a_damaged_file_is_an_error_not_an_empty_list() {
        let path = scratch("damaged");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{not json").unwrap();
        assert!(Approvals::load(path).is_err());
    }

    #[test]
    fn once_records_are_trimmed_and_always_records_kept() {
        let mut approvals = Approvals::in_memory();
        approvals.grant("keep", Grant::Always).unwrap();
        for i in 0..ONCE_KEPT + 5 {
            approvals.grant(&format!("{i}"), Grant::Once).unwrap();
        }
        assert_eq!(approvals.records().len(), ONCE_KEPT + 1);
        assert!(approvals.is_always("keep"));
        assert_eq!(approvals.records()[1].hash, "5");
    }

    #[test]
    fn the_hash_is_the_sha256_of_the_compact_spec() {
        let spec = CommandSpec {
            program: "a".into(),
            args: vec![],
            cwd: String::new(),
            shell: false,
        };
        // sha256 of {"program":"a","args":[],"cwd":"","shell":false}
        let expected: String =
            Sha256::digest(br#"{"program":"a","args":[],"cwd":"","shell":false}"#)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
        assert_eq!(spec.hash(), expected);
    }
}
