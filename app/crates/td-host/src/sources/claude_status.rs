//! Claude Code's status line, read passively.
//!
//! Claude Code pipes a JSON document to its status line command. `td-statusline` wraps
//! that command and writes the document to `<local data dir>/<app id>/claude-status/<session>.json`.
//! This source reads the newest of those files. The `statusLine` entry of
//! `~/.claude/settings.json` is edited by `td_core::statusline`, which `td-statusline
//! --unwrap` shares.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::Value;
use td_core::app_dirs::Dirs;

pub use td_core::statusline::{
    Edit, edit_settings, quote, runs_shim, unwrap, unwrap_settings, wrap,
};

use super::{CLAUDE_STATUS, FAILS_TO_STALE, Info, Reading, Source, unix_ms};

/// A file this much older than now no longer describes a live session.
pub const STALE_AFTER: Duration = Duration::from_secs(600);

pub fn default_dir() -> PathBuf {
    Dirs::current().claude_status()
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClaudeInfo {
    pub dir: String,
    pub session: Option<String>,
    pub updated: Option<u64>,
    pub stale: bool,
    pub error: Option<String>,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, PartialEq)]
struct Parsed {
    model: Option<String>,
    context: Option<f64>,
    cost: Option<f64>,
    five_hour: Option<f64>,
    seven_day: Option<f64>,
}

impl Parsed {
    fn from_json(bytes: &[u8]) -> Result<Parsed, String> {
        let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if !value.is_object() {
            return Err("the status line document is not an object".into());
        }
        let number = |path: &[&str]| {
            path.iter()
                .try_fold(&value, |at, key| at.get(key))
                .and_then(Value::as_f64)
        };
        Ok(Parsed {
            model: value["model"]["display_name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            context: number(&["context_window", "used_percentage"]),
            cost: number(&["cost", "total_cost_usd"]),
            five_hour: number(&["rate_limits", "five_hour", "used_percentage"]),
            seven_day: number(&["rate_limits", "seven_day", "used_percentage"]),
        })
    }

    fn readings(&self, updated: u64, stale: bool) -> Vec<Reading> {
        let reading = |field: &str, value: String, bar: f64| {
            Reading::new(CLAUDE_STATUS, "", field, value, bar, updated, stale)
        };
        let percent = |field: &str, value: f64| {
            let value = value.clamp(0.0, 100.0);
            reading(field, format!("{}%", value.round() as i64), value / 100.0)
        };
        let mut out = Vec::new();
        if let Some(model) = &self.model {
            out.push(reading("model", model.clone(), -1.0));
        }
        if let Some(value) = self.context {
            out.push(percent("context_pct", value));
        }
        if let Some(cost) = self.cost {
            out.push(reading("cost_usd", format!("${cost:.2}"), -1.0));
        }
        if let Some(value) = self.five_hour {
            out.push(percent("five_hour_pct", value));
        }
        if let Some(value) = self.seven_day {
            out.push(percent("seven_day_pct", value));
        }
        out
    }
}

struct Cached {
    path: PathBuf,
    modified: SystemTime,
    len: u64,
    parsed: Parsed,
}

/// Reads the newest status line file in a directory.
pub struct ClaudeStatus {
    dir: PathBuf,
    cached: Option<Cached>,
    fails: u32,
    info: ClaudeInfo,
}

impl ClaudeStatus {
    pub fn new(dir: impl Into<PathBuf>) -> ClaudeStatus {
        let dir = dir.into();
        let info = ClaudeInfo {
            dir: dir.display().to_string(),
            ..ClaudeInfo::default()
        };
        ClaudeStatus {
            dir,
            cached: None,
            fails: 0,
            info,
        }
    }

    fn newest(&self) -> Option<(PathBuf, SystemTime, u64)> {
        fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let meta = entry.metadata().ok().filter(|meta| meta.is_file())?;
                Some((entry.path(), meta.modified().ok()?, meta.len()))
            })
            .max_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)))
    }

    pub fn poll_at(&mut self, now: SystemTime) -> Vec<Reading> {
        self.info.error = None;
        if let Some((path, modified, len)) = self.newest() {
            let fresh = self
                .cached
                .as_ref()
                .is_some_and(|c| c.path == path && c.modified == modified && c.len == len);
            if !fresh {
                match fs::read(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| Parsed::from_json(&bytes))
                {
                    Ok(parsed) => {
                        self.cached = Some(Cached {
                            path,
                            modified,
                            len,
                            parsed,
                        });
                    }
                    Err(error) => {
                        self.info.error = Some(format!("{}: {error}", path.display()));
                    }
                }
            }
        } else {
            self.info.error = Some("no status line file found".to_owned());
        }
        self.fails = if self.info.error.is_some() {
            self.fails.saturating_add(1)
        } else {
            0
        };
        let Some(cached) = &self.cached else {
            self.info.session = None;
            self.info.updated = None;
            self.info.stale = false;
            self.info.readings.clear();
            return Vec::new();
        };
        let stale = self.fails >= FAILS_TO_STALE
            || now
                .duration_since(cached.modified)
                .is_ok_and(|age| age >= STALE_AFTER);
        let updated = unix_ms(cached.modified);
        let readings = cached.parsed.readings(updated, stale);
        self.info.session = cached
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
        self.info.updated = Some(updated);
        self.info.stale = stale;
        self.info.readings = readings.clone();
        readings
    }
}

impl Source for ClaudeStatus {
    fn id(&self) -> &'static str {
        CLAUDE_STATUS
    }

    fn poll(&mut self) -> Vec<Reading> {
        self.poll_at(SystemTime::now())
    }

    fn info(&self) -> Info {
        Info::Claude(self.info.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    /// `ctx`, `del`, or `add`.
    pub kind: &'static str,
    pub text: String,
}

/// The lines two texts differ in, with one line of context. A settings edit changes one
/// span, so trimming the common head and tail is the whole diff.
pub fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let (old, new): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let line = |kind, text: &str| DiffLine {
        kind,
        text: text.to_owned(),
    };
    let mut out = Vec::new();
    if head > 0 {
        out.push(line("ctx", old[head - 1]));
    }
    out.extend(
        old[head..old.len() - tail]
            .iter()
            .map(|text| line("del", text)),
    );
    out.extend(
        new[head..new.len() - tail]
            .iter()
            .map(|text| line("add", text)),
    );
    if tail > 0 {
        out.push(line("ctx", old[old.len() - tail]));
    }
    out
}

/// The sibling of the app binary named `td-statusline`, or the bare name when none sits there.
pub fn shim_command(dir: Option<&Path>) -> (String, bool) {
    let name = if cfg!(windows) {
        "td-statusline.exe"
    } else {
        "td-statusline"
    };
    match dir.map(|dir| dir.join(name)).filter(|path| path.is_file()) {
        Some(path) => {
            let text = path.display().to_string();
            (
                if text.contains(' ') {
                    format!("\"{text}\"")
                } else {
                    text
                },
                true,
            )
        }
        None => ("td-statusline".to_owned(), false),
    }
}
