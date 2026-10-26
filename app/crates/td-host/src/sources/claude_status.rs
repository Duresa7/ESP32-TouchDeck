//! Claude Code's status line, read passively.
//!
//! Claude Code pipes a JSON document to its status line command. `td-statusline` wraps
//! that command and writes the document to `<state dir>/touchdeck/claude-status/<session>.json`.
//! This source reads the newest of those files. It also builds the `statusLine` entry for
//! `~/.claude/settings.json` and edits that file without touching a byte outside the entry.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::{Map, Value};

use super::{CLAUDE_STATUS, FAILS_TO_STALE, Info, Reading, Source, state_base, unix_ms};

/// A file this much older than now no longer describes a live session.
pub const STALE_AFTER: Duration = Duration::from_secs(600);

pub fn default_dir() -> PathBuf {
    state_base().join("touchdeck").join("claude-status")
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

/// Quotes a command for a POSIX shell inside double quotes, so the shell hands it back unchanged.
pub fn quote(command: &str) -> String {
    let mut out = String::from("\"");
    for ch in command.chars() {
        if matches!(ch, '\\' | '"' | '$' | '`') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// The command the `statusLine` entry runs: the shim alone, or the shim around the old command.
pub fn wrap(shim: &str, old: Option<&str>) -> String {
    match old {
        Some(old) => format!("{shim} --then {}", quote(old)),
        None => shim.to_owned(),
    }
}

/// The outcome of editing a settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    /// The command the entry ran before, if it had one.
    pub before: Option<String>,
    /// The command the entry runs after.
    pub command: String,
    /// False when the entry already runs the shim, so the file stays as it is.
    pub changed: bool,
    /// The file's new text.
    pub text: String,
}

fn runs_shim(command: &str) -> bool {
    let command = command.trim_start();
    let program = match command.chars().next() {
        Some(quote @ ('"' | '\'')) => command[1..].split(quote).next().unwrap_or(""),
        _ => command.split_whitespace().next().unwrap_or(""),
    };
    program.contains("td-statusline")
}

struct Members {
    first: Option<usize>,
    last_end: Option<usize>,
    status_line: Option<(usize, usize, usize)>,
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn skip_string(b: &[u8], mut i: usize) -> Result<usize, String> {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Ok(i + 1),
            _ => i += 1,
        }
    }
    Err("unterminated string".into())
}

fn skip_value(b: &[u8], i: usize) -> Result<usize, String> {
    match b.get(i) {
        Some(b'"') => skip_string(b, i),
        Some(b'{' | b'[') => {
            let (mut depth, mut at) = (0usize, i);
            while at < b.len() {
                match b[at] {
                    b'"' => at = skip_string(b, at)?,
                    b'{' | b'[' => {
                        depth += 1;
                        at += 1;
                    }
                    b'}' | b']' => {
                        depth -= 1;
                        at += 1;
                        if depth == 0 {
                            return Ok(at);
                        }
                    }
                    _ => at += 1,
                }
            }
            Err("unterminated value".into())
        }
        Some(_) => {
            let mut at = i;
            while at < b.len()
                && !matches!(b[at], b',' | b'}' | b']')
                && !b[at].is_ascii_whitespace()
            {
                at += 1;
            }
            Ok(at)
        }
        None => Err("missing value".into()),
    }
}

/// Finds the top-level members of a settings file that is already known to be one JSON object.
fn members(text: &str) -> Result<Members, String> {
    let b = text.as_bytes();
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return Err("the settings file is not a JSON object".into());
    }
    i += 1;
    let mut out = Members {
        first: None,
        last_end: None,
        status_line: None,
    };
    loop {
        i = skip_ws(b, i);
        match b.get(i) {
            Some(b'}') => return Ok(out),
            Some(b'"') => {}
            _ => return Err("the settings file is not a JSON object".into()),
        }
        let key_start = i;
        i = skip_string(b, i)?;
        let key: String = serde_json::from_str(&text[key_start..i]).map_err(|e| e.to_string())?;
        i = skip_ws(b, i);
        if b.get(i) != Some(&b':') {
            return Err("expected a colon after a member name".into());
        }
        i = skip_ws(b, i + 1);
        let value_start = i;
        i = skip_value(b, i)?;
        out.first.get_or_insert(key_start);
        out.last_end = Some(i);
        if key == "statusLine" {
            out.status_line = Some((key_start, value_start, i));
        }
        i = skip_ws(b, i);
        if b.get(i) == Some(&b',') {
            i += 1;
        }
    }
}

fn indent_of(text: &str, at: usize) -> String {
    let line = text[..at].rsplit('\n').next().unwrap_or("");
    if line.chars().all(|ch| ch == ' ' || ch == '\t') && !line.is_empty() {
        line.to_owned()
    } else {
        "  ".to_owned()
    }
}

fn render(value: &Value, indent: &str) -> String {
    let pretty = serde_json::to_string_pretty(value).expect("JSON prints");
    pretty.replace('\n', &format!("\n{indent}"))
}

/// Points the `statusLine` entry of a settings file at the shim and keeps the old command
/// as its `--then` target. `None` is a file that does not exist yet.
pub fn edit_settings(text: Option<&str>, shim: &str) -> Result<Edit, String> {
    let source = text.unwrap_or("");
    if source.trim().is_empty() {
        let entry = serde_json::json!({"type": "command", "command": shim});
        return Ok(Edit {
            before: None,
            command: shim.to_owned(),
            changed: true,
            text: format!("{{\n  \"statusLine\": {}\n}}\n", render(&entry, "  ")),
        });
    }
    let parsed: Value = serde_json::from_str(source)
        .map_err(|error| format!("the settings file is not valid JSON: {error}"))?;
    if !parsed.is_object() {
        return Err("the settings file is not a JSON object".into());
    }
    let found = members(source)?;
    let Some((key_start, value_start, value_end)) = found.status_line else {
        let entry = serde_json::json!({"type": "command", "command": shim});
        let indent = found
            .first
            .map_or_else(|| "  ".to_owned(), |first| indent_of(source, first));
        let member = format!("\"statusLine\": {}", render(&entry, &indent));
        let text = match found.last_end {
            Some(end) => format!("{},\n{indent}{member}{}", &source[..end], &source[end..]),
            None => {
                let open = source.find('{').expect("an object has a brace") + 1;
                let close = source.rfind('}').expect("an object has a brace");
                format!(
                    "{}\n{indent}{member}\n{}",
                    &source[..open],
                    &source[close..]
                )
            }
        };
        return Ok(Edit {
            before: None,
            command: shim.to_owned(),
            changed: true,
            text,
        });
    };
    let mut entry: Map<String, Value> = match serde_json::from_str(&source[value_start..value_end])
    {
        Ok(Value::Object(entry)) => entry,
        _ => return Err("the statusLine entry is not an object".into()),
    };
    let before = match entry.get("command") {
        Some(Value::String(command)) => Some(command.clone()),
        Some(_) => return Err("the statusLine command is not a string".into()),
        None => None,
    };
    if before.as_deref().is_some_and(runs_shim) {
        return Ok(Edit {
            command: before.clone().unwrap_or_default(),
            before,
            changed: false,
            text: source.to_owned(),
        });
    }
    let command = wrap(shim, before.as_deref());
    entry.insert("command".into(), Value::String(command.clone()));
    entry
        .entry("type")
        .or_insert_with(|| Value::String("command".into()));
    let indent = indent_of(source, key_start);
    let text = format!(
        "{}{}{}",
        &source[..value_start],
        render(&Value::Object(entry), &indent),
        &source[value_end..]
    );
    Ok(Edit {
        before,
        command,
        changed: true,
        text,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    /// `ctx`, `del`, or `add`.
    pub kind: &'static str,
    pub text: String,
}

/// The lines two texts differ in, with one line of context. The edit above changes one
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
