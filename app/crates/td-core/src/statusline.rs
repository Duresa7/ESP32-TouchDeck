//! The `statusLine` entry of Claude Code's settings file.
//!
//! Install points the entry at `td-statusline` and keeps the old command as its `--then`
//! target. Unwrap puts the old command back, or removes the entry when there was none. Both
//! edit only the bytes they must, and both are shared by the app and `td-statusline --unwrap`.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::app_dirs::env_dir;

/// Where Claude Code reads its settings: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
pub fn claude_settings_path() -> PathBuf {
    env_dir("CLAUDE_CONFIG_DIR")
        .or_else(|| {
            env_dir("HOME")
                .or_else(|| env_dir("USERPROFILE"))
                .map(|home| home.join(".claude"))
        })
        .unwrap_or_else(|| PathBuf::from(".claude"))
        .join("settings.json")
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

/// Splits a command into words the way a POSIX shell would for the quoting [`quote`] writes:
/// double quotes undo `\\`, `\"`, `\$` and `` \` ``, single quotes keep everything, and a
/// backslash outside quotes escapes a space, a quote, `$`, `` ` `` or another backslash.
fn words(command: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = command.chars().peekable();
    loop {
        while chars.next_if(|ch| ch.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return Ok(out);
        }
        let mut word = String::new();
        while let Some(ch) = chars.next_if(|ch| !ch.is_whitespace()) {
            match ch {
                '"' => loop {
                    match chars.next() {
                        None => return Err("the statusLine command has an unclosed \"".into()),
                        Some('"') => break,
                        Some('\\') => {
                            match chars.next_if(|next| matches!(next, '\\' | '"' | '$' | '`')) {
                                Some(next) => word.push(next),
                                None => word.push('\\'),
                            }
                        }
                        Some(other) => word.push(other),
                    }
                },
                '\'' => loop {
                    match chars.next() {
                        None => return Err("the statusLine command has an unclosed '".into()),
                        Some('\'') => break,
                        Some(other) => word.push(other),
                    }
                },
                '\\' => match chars.next_if(|next| {
                    next.is_whitespace() || matches!(next, '\\' | '"' | '\'' | '$' | '`')
                }) {
                    Some(next) => word.push(next),
                    None => word.push('\\'),
                },
                other => word.push(other),
            }
        }
        out.push(word);
    }
}

/// True when the command's program is `td-statusline`.
pub fn runs_shim(command: &str) -> bool {
    let command = command.trim_start();
    let program = match command.chars().next() {
        Some(quote @ ('"' | '\'')) => command[1..].split(quote).next().unwrap_or(""),
        _ => command.split_whitespace().next().unwrap_or(""),
    };
    program.contains("td-statusline")
}

/// The command a shim command runs after it with `--then`, or `None` when it runs none.
pub fn unwrap(command: &str) -> Result<Option<String>, String> {
    let words = words(command)?;
    let mut args = words.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--then" {
            return match args.next() {
                Some(then) if !then.trim().is_empty() => Ok(Some(then.clone())),
                Some(_) => Ok(None),
                None => Err("the statusLine command ends in --then".into()),
            };
        }
    }
    Ok(None)
}

/// The outcome of editing a settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    /// The command the entry ran before, if it had one.
    pub before: Option<String>,
    /// The command the entry runs after. Empty when an unwrap removed the entry.
    pub command: String,
    /// False when there was nothing to do, so the file stays as it is.
    pub changed: bool,
    /// The file's new text.
    pub text: String,
}

impl Edit {
    fn unchanged(before: Option<String>, command: String, text: &str) -> Edit {
        Edit {
            before,
            command,
            changed: false,
            text: text.to_owned(),
        }
    }
}

struct Member {
    key: String,
    key_start: usize,
    value_start: usize,
    value_end: usize,
}

struct Object {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

impl Object {
    fn get(&self, key: &str) -> Option<(usize, &Member)> {
        self.members
            .iter()
            .enumerate()
            .find(|(_, member)| member.key == key)
    }
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

/// Finds the members of the object that starts at `start`, in text already known to be JSON.
fn object_at(text: &str, start: usize) -> Result<Object, String> {
    let b = text.as_bytes();
    let mut i = skip_ws(b, start);
    if b.get(i) != Some(&b'{') {
        return Err("the settings file is not a JSON object".into());
    }
    let open = i;
    i += 1;
    let mut members = Vec::new();
    loop {
        i = skip_ws(b, i);
        match b.get(i) {
            Some(b'}') => {
                return Ok(Object {
                    open,
                    close: i,
                    members,
                });
            }
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
        members.push(Member {
            key,
            key_start,
            value_start,
            value_end: i,
        });
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

fn splice(text: &str, start: usize, end: usize, with: &str) -> String {
    format!("{}{with}{}", &text[..start], &text[end..])
}

fn parse_object(source: &str) -> Result<Object, String> {
    let parsed: Value = serde_json::from_str(source)
        .map_err(|error| format!("the settings file is not valid JSON: {error}"))?;
    if !parsed.is_object() {
        return Err("the settings file is not a JSON object".into());
    }
    object_at(source, 0)
}

/// The `statusLine` entry: its parsed members, its text span, and its `command` member.
struct Entry<'a> {
    object: Object,
    fields: Map<String, Value>,
    member: &'a Member,
}

fn entry<'a>(source: &str, top: &'a Object) -> Result<Option<Entry<'a>>, String> {
    let Some((_, member)) = top.get("statusLine") else {
        return Ok(None);
    };
    match serde_json::from_str(&source[member.value_start..member.value_end]) {
        Ok(Value::Object(fields)) => Ok(Some(Entry {
            object: object_at(source, member.value_start)?,
            fields,
            member,
        })),
        _ => Err("the statusLine entry is not an object".into()),
    }
}

/// Points the `statusLine` entry of a settings file at the shim and keeps the old command
/// as its `--then` target. `None` is a file that does not exist yet.
pub fn edit_settings(text: Option<&str>, shim: &str) -> Result<Edit, String> {
    let source = text.unwrap_or("");
    let fresh = serde_json::json!({"type": "command", "command": shim});
    if source.trim().is_empty() {
        return Ok(Edit {
            before: None,
            command: shim.to_owned(),
            changed: true,
            text: format!("{{\n  \"statusLine\": {}\n}}\n", render(&fresh, "  ")),
        });
    }
    let top = parse_object(source)?;
    let Some(entry) = entry(source, &top)? else {
        let indent = top.members.first().map_or_else(
            || "  ".to_owned(),
            |first| indent_of(source, first.key_start),
        );
        let member = format!("\"statusLine\": {}", render(&fresh, &indent));
        let text = match top.members.last() {
            Some(last) => splice(
                source,
                last.value_end,
                last.value_end,
                &format!(",\n{indent}{member}"),
            ),
            None => splice(
                source,
                top.open + 1,
                top.close,
                &format!("\n{indent}{member}\n"),
            ),
        };
        return Ok(Edit {
            before: None,
            command: shim.to_owned(),
            changed: true,
            text,
        });
    };
    let before = match entry.fields.get("command") {
        Some(Value::String(command)) => Some(command.clone()),
        Some(_) => return Err("the statusLine command is not a string".into()),
        None => None,
    };
    if let Some(command) = before.as_deref().filter(|command| runs_shim(command)) {
        return Ok(Edit::unchanged(before.clone(), command.to_owned(), source));
    }
    let command = wrap(shim, before.as_deref());
    let encoded = serde_json::to_string(&command).expect("a string prints");
    let text = match entry.object.get("command") {
        // Only the command string changes, so an unwrap can give back the exact bytes.
        Some((_, at)) if entry.fields.contains_key("type") => {
            splice(source, at.value_start, at.value_end, &encoded)
        }
        _ => {
            let mut fields = entry.fields;
            fields.insert("command".into(), Value::String(command.clone()));
            fields
                .entry("type")
                .or_insert_with(|| Value::String("command".into()));
            let indent = indent_of(source, entry.member.key_start);
            splice(
                source,
                entry.member.value_start,
                entry.member.value_end,
                &render(&Value::Object(fields), &indent),
            )
        }
    };
    Ok(Edit {
        before,
        command,
        changed: true,
        text,
    })
}

/// Undoes [`edit_settings`]: when the `statusLine` entry runs the shim, its `--then` command
/// goes back in, or the entry goes when the shim ran alone. Anything else is left as it is,
/// so a file that does not run the shim, or does not exist, comes back unchanged.
pub fn unwrap_settings(text: Option<&str>) -> Result<Edit, String> {
    let source = text.unwrap_or("");
    if source.trim().is_empty() {
        return Ok(Edit::unchanged(None, String::new(), source));
    }
    let top = parse_object(source)?;
    let entry = match entry(source, &top) {
        Ok(Some(entry)) => entry,
        Ok(None) | Err(_) => return Ok(Edit::unchanged(None, String::new(), source)),
    };
    let (Some(Value::String(before)), Some((_, at))) =
        (entry.fields.get("command"), entry.object.get("command"))
    else {
        return Ok(Edit::unchanged(None, String::new(), source));
    };
    if !runs_shim(before) {
        return Ok(Edit::unchanged(
            Some(before.clone()),
            before.clone(),
            source,
        ));
    }
    let (command, text) = match unwrap(before)? {
        Some(old) => {
            let encoded = serde_json::to_string(&old).expect("a string prints");
            let text = splice(source, at.value_start, at.value_end, &encoded);
            (old, text)
        }
        None => (String::new(), remove_member(source, &top, "statusLine")),
    };
    Ok(Edit {
        before: Some(before.clone()),
        command,
        changed: true,
        text,
    })
}

/// Cuts a top-level member and the comma and line that belong to it.
fn remove_member(source: &str, top: &Object, key: &str) -> String {
    let Some((index, member)) = top.get(key) else {
        return source.to_owned();
    };
    if let Some(next) = top.members.get(index + 1) {
        splice(source, member.key_start, next.key_start, "")
    } else if let Some(previous) = index.checked_sub(1).map(|i| &top.members[i]) {
        splice(source, previous.value_end, member.value_end, "")
    } else {
        splice(source, top.open + 1, top.close, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIM: &str = r#""C:\Users\A B\AppData\Local\TouchDeck\td-statusline.exe""#;

    #[test]
    fn words_undo_quote() {
        for old in [
            "echo old",
            r#"sh 'C:\Users\dev\.claude\statusline.sh' | tee "$HOME/x""#,
            r#"a\b \\c `d` $e ""f"#,
            "",
        ] {
            let command = wrap(SHIM, Some(old));
            let words = words(&command).unwrap();
            assert_eq!(words.last().map(String::as_str), Some(old), "{command}");
        }
        assert_eq!(words(r"a\ b 'c d' e").unwrap(), ["a b", "c d", "e"]);
        assert!(words("a \"b").is_err() && words("a 'b").is_err());
    }

    fn round_trip(before: &str) -> (Edit, Edit) {
        let install = edit_settings(Some(before), SHIM).unwrap();
        assert!(install.changed && runs_shim(&install.command), "{before}");
        let undo = unwrap_settings(Some(&install.text)).unwrap();
        assert!(undo.changed);
        assert_eq!(undo.before.as_deref(), Some(install.command.as_str()));
        (install, undo)
    }

    #[test]
    fn install_then_unwrap_gives_back_the_old_command_byte_for_byte() {
        let old = r#"sh 'C:\Users\dev\.claude\statusline.sh' | tee "$HOME/x" && echo `date` \"#;
        let before = format!(
            "{{\n  \"model\": \"opus\",\n  \"statusLine\": {{\n    \"type\": \"command\",\n    \"command\": {},\n    \"padding\": 0\n  }},\n  \"env\": {{ \"A\": \"1\" }}\n}}\n",
            serde_json::to_string(old).unwrap()
        );
        let (install, undo) = round_trip(&before);
        assert_eq!(install.before.as_deref(), Some(old));
        assert_eq!(undo.command, old);
        assert_eq!(undo.text, before);
    }

    #[test]
    fn install_then_unwrap_without_an_old_command_removes_the_entry() {
        for before in [
            "{\n  \"model\": \"opus\"\n}\n",
            "{\n  \"model\": \"opus\",\n  \"env\": {}\n}",
            "{\n\t\"a\": [1, 2]\n}\n",
            "{}",
            "{}\n",
        ] {
            let (install, undo) = round_trip(before);
            assert_eq!(install.before, None);
            assert_eq!(undo.command, "");
            assert_eq!(undo.text, before, "{before:?}");
        }
        // An entry in the middle goes with its comma and its line.
        let middle = "{\n  \"a\": 1,\n  \"statusLine\": {\"type\": \"command\", \"command\": \"td-statusline\"},\n  \"b\": 2\n}\n";
        let undo = unwrap_settings(Some(middle)).unwrap();
        assert_eq!(undo.text, "{\n  \"a\": 1,\n  \"b\": 2\n}\n");
        // A missing settings file gets the entry from Install, and Unwrap leaves an empty object.
        let install = edit_settings(None, SHIM).unwrap();
        let undo = unwrap_settings(Some(&install.text)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&undo.text).unwrap(),
            serde_json::json!({})
        );
    }

    #[test]
    fn unwrap_restores_an_entry_install_had_to_rewrite() {
        let before = "{\"statusLine\": {\"command\": \"echo hi\", \"padding\": 2}}";
        let (_, undo) = round_trip(before);
        let after: Value = serde_json::from_str(&undo.text).unwrap();
        assert_eq!(after["statusLine"]["command"], "echo hi");
        assert_eq!(after["statusLine"]["padding"], 2);
    }

    #[test]
    fn unwrap_leaves_a_file_that_does_not_run_the_shim() {
        let installed = edit_settings(
            Some("{\"statusLine\": {\"type\": \"command\", \"command\": \"echo old\"}}"),
            SHIM,
        )
        .unwrap();
        let once = unwrap_settings(Some(&installed.text)).unwrap();
        for text in [
            once.text.as_str(),
            "{\"model\": \"opus\"}",
            "{\"statusLine\": \"just a string\"}",
            "{\"statusLine\": {\"command\": 3}}",
            "",
        ] {
            let again = unwrap_settings(Some(text)).unwrap();
            assert!(!again.changed, "{text}");
            assert_eq!(again.text, text);
        }
        assert!(!unwrap_settings(None).unwrap().changed);
        for bad in ["{ not json", "[1]"] {
            assert!(unwrap_settings(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn unwrap_finds_the_then_target_after_other_flags() {
        assert_eq!(unwrap("td-statusline").unwrap(), None);
        assert_eq!(
            unwrap(r#"td-statusline --dir 'x y' --then "echo \"hi\"""#).unwrap(),
            Some(r#"echo "hi""#.to_owned())
        );
        assert_eq!(unwrap(r#"td-statusline --then """#).unwrap(), None);
        assert!(unwrap("td-statusline --then").is_err());
    }
}
