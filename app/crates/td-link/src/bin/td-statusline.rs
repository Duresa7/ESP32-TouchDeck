//! The status line shim.
//!
//! Claude Code pipes a JSON document to its status line command. This command saves the
//! document to `<local data dir>/io.github.duresa7.touchdeck/claude-status/<session_id>.json`,
//! where the app reads it, and with `--then "<command>"` also pipes the same document to that
//! command and prints its stdout, so an existing status line keeps working. It exits with the
//! command's code, and with 0 when there is none, so it never breaks the status line.
//!
//! `--unwrap` undoes the app's Install for an uninstaller: it puts the `--then` command back
//! in Claude Code's settings file, or removes the entry. It never reads stdin.

use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;

use serde_json::Value;
use td_core::app_dirs::{Dirs, write_atomic};
use td_core::statusline::{claude_settings_path, unwrap_settings};

const USAGE: &str = "usage: td-statusline [--dir <dir>] [--then \"<command>\"] [--shell cmd|sh]
       td-statusline --unwrap [--settings <file>]
  reads Claude Code's status line JSON on stdin and writes it to
  <local data dir>/io.github.duresa7.touchdeck/claude-status/<session_id>.json
  --dir <dir>        write to this directory instead
  --then \"<cmd>\"     pipe the same JSON to cmd and print its stdout
  --shell cmd|sh     the shell for --then: cmd on Windows, sh elsewhere by default
  --unwrap           do not read stdin; if the settings file's statusLine runs
                     td-statusline, put its --then command back, or remove the entry
  --settings <file>  the file --unwrap edits: $CLAUDE_CONFIG_DIR/settings.json,
                     else ~/.claude/settings.json
local data dir: $TOUCHDECK_STATE_DIR, else %LOCALAPPDATA% on Windows and
$XDG_DATA_HOME or ~/.local/share elsewhere
exit codes for --unwrap: 0 restored, removed, or nothing to do; 1 the file could
not be read, parsed, or written; 2 bad arguments";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shell {
    Cmd,
    Sh,
}

#[derive(Debug, Default)]
struct Options {
    dir: Option<PathBuf>,
    then: Option<String>,
    shell: Option<Shell>,
    unwrap: bool,
    settings: Option<PathBuf>,
}

fn parse(mut argv: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--dir" => options.dir = Some(argv.next().ok_or("--dir needs a directory")?.into()),
            "--then" => options.then = Some(argv.next().ok_or("--then needs a command")?),
            "--shell" => {
                options.shell = Some(match argv.next().as_deref() {
                    Some("cmd") => Shell::Cmd,
                    Some("sh") => Shell::Sh,
                    _ => return Err("--shell is cmd or sh".into()),
                });
            }
            "--unwrap" => options.unwrap = true,
            "--settings" => {
                options.settings = Some(argv.next().ok_or("--settings needs a file")?.into());
            }
            _ => return Err(format!("unknown argument {arg:?}")),
        }
    }
    if options.unwrap
        && (options.dir.is_some() || options.then.is_some() || options.shell.is_some())
    {
        return Err("--unwrap takes only --settings".into());
    }
    if !options.unwrap && options.settings.is_some() {
        return Err("--settings goes with --unwrap".into());
    }
    Ok(options)
}

fn status_dir(explicit: Option<&Path>) -> PathBuf {
    explicit.map_or_else(|| Dirs::current().claude_status(), Path::to_path_buf)
}

fn file_stem(session: &str) -> String {
    let stem: String = session
        .chars()
        .take(64)
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "unknown".to_owned()
    } else {
        stem
    }
}

/// Writes the document to a temporary file in the directory and renames it into place, so
/// a reader sees the old document or the new one and never half of one.
fn save(dir: &Path, input: &[u8]) -> Result<PathBuf, String> {
    let document: Value = serde_json::from_slice(input).map_err(|error| error.to_string())?;
    if !document.is_object() {
        return Err("the status line document is not a JSON object".into());
    }
    let stem = file_stem(document["session_id"].as_str().unwrap_or(""));
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let temp = dir.join(format!(".{stem}.{}.tmp", std::process::id()));
    let target = dir.join(format!("{stem}.json"));
    fs::write(&temp, input).map_err(|error| format!("{}: {error}", temp.display()))?;
    fs::rename(&temp, &target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("{}: {error}", target.display())
    })?;
    Ok(target)
}

fn shell_command(shell: Option<Shell>, command: &str) -> Command {
    let shell = shell.unwrap_or(if cfg!(windows) { Shell::Cmd } else { Shell::Sh });
    match shell {
        #[cfg(windows)]
        Shell::Cmd => {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new(std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd".into()));
            cmd.args(["/S", "/C"]).raw_arg(format!("\"{command}\""));
            cmd
        }
        #[cfg(not(windows))]
        Shell::Cmd => {
            let mut sh = Command::new("sh");
            sh.arg("-c").arg(command);
            sh
        }
        Shell::Sh => {
            let mut sh = Command::new("sh");
            sh.arg("-c").arg(command);
            sh
        }
    }
}

/// Pipes the document to the command and returns its stdout and exit code.
fn forward(shell: Option<Shell>, command: &str, input: &[u8]) -> io::Result<(Vec<u8>, i32)> {
    let mut child = shell_command(shell, command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let document = input.to_vec();
    let feeder = thread::spawn(move || {
        let _ = stdin.write_all(&document);
    });
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .expect("stdout is piped")
        .read_to_end(&mut stdout)?;
    let status = child.wait()?;
    let _ = feeder.join();
    Ok((stdout, status.code().unwrap_or(1)))
}

fn run(options: &Options, input: &[u8], out: &mut dyn Write) -> i32 {
    if let Err(error) = save(&status_dir(options.dir.as_deref()), input) {
        eprintln!("td-statusline: not saved: {error}");
    }
    let Some(command) = &options.then else {
        return 0;
    };
    match forward(options.shell, command, input) {
        Ok((stdout, code)) => {
            let _ = out.write_all(&stdout);
            let _ = out.flush();
            code
        }
        Err(error) => {
            eprintln!("td-statusline: cannot run {command:?}: {error}");
            1
        }
    }
}

/// Undoes Install in a settings file. An uninstaller runs this with no console, so every
/// write to `out` may go nowhere and is never allowed to fail the run.
fn unwrap_file(path: &Path, out: &mut dyn Write) -> i32 {
    let text = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            let _ = writeln!(out, "td-statusline: {}: {error}", path.display());
            return 1;
        }
    };
    let edit = match unwrap_settings(text.as_deref()) {
        Ok(edit) => edit,
        Err(error) => {
            let _ = writeln!(out, "td-statusline: {}: {error}", path.display());
            return 1;
        }
    };
    let said = if !edit.changed {
        format!(
            "{} does not run td-statusline, nothing to do",
            path.display()
        )
    } else if let Err(error) = write_atomic(path, edit.text.as_bytes()) {
        let _ = writeln!(out, "td-statusline: {}: {error}", path.display());
        return 1;
    } else if edit.command.is_empty() {
        format!("removed the statusLine entry from {}", path.display())
    } else {
        format!("restored {:?} in {}", edit.command, path.display())
    };
    let _ = writeln!(out, "td-statusline: {said}");
    0
}

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            let _ = writeln!(io::stderr(), "td-statusline: {error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if options.unwrap {
        let path = options
            .settings
            .clone()
            .unwrap_or_else(claude_settings_path);
        let code = unwrap_file(&path, &mut io::stdout().lock());
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    let mut input = Vec::new();
    if let Err(error) = io::stdin().read_to_end(&mut input) {
        eprintln!("td-statusline: cannot read stdin: {error}");
        return ExitCode::from(2);
    }
    let code = run(&options, &input, &mut io::stdout().lock());
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    fn scratch(name: &str) -> PathBuf {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "td-statusline-{name}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn document(session: &str, filler: usize) -> Vec<u8> {
        format!(
            r#"{{"session_id":"{session}","model":{{"display_name":"Opus"}},"pad":"{}"}}"#,
            "x".repeat(filler)
        )
        .into_bytes()
    }

    const ECHO: &str = "echo passthrough";
    const COPY: &str = if cfg!(windows) {
        "findstr \"^\""
    } else {
        "cat"
    };

    #[test]
    fn the_file_is_written_whole_and_named_for_the_session() {
        let dir = scratch("whole");
        let input = document("abc-123", 10);
        let path = save(&dir, &input).unwrap();
        assert_eq!(path, dir.join("abc-123.json"));
        assert_eq!(fs::read(&path).unwrap(), input);
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names.len(), 1, "no temporary file stays behind: {names:?}");
    }

    #[test]
    fn a_reader_never_sees_half_a_document() {
        let dir = scratch("atomic");
        let (small, large) = (document("s", 10), document("s", 400_000));
        save(&dir, &small).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let reader = {
            let (path, done) = (dir.join("s.json"), done.clone());
            let (small, large) = (small.clone(), large.clone());
            thread::spawn(move || {
                let mut reads = 0u32;
                while !done.load(Ordering::Relaxed) {
                    if let Ok(bytes) = fs::read(&path) {
                        assert!(
                            bytes == small || bytes == large,
                            "a partial read of {} bytes",
                            bytes.len()
                        );
                        reads += 1;
                    }
                }
                reads
            })
        };
        for round in 0..150 {
            save(&dir, if round % 2 == 0 { &large } else { &small }).unwrap();
        }
        done.store(true, Ordering::Relaxed);
        assert!(reader.join().unwrap() > 0);
    }

    #[test]
    fn a_hostile_session_id_stays_inside_the_directory() {
        let dir = scratch("hostile");
        let path = save(&dir, &document("../../evil\\\\x:y", 1)).unwrap();
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert!(file_stem("").starts_with("unknown"));
        assert_eq!(file_stem("a/b"), "a_b");
    }

    #[test]
    fn then_stdout_is_forwarded_and_the_file_equals_the_input() {
        let dir = scratch("then");
        let input = document("fwd", 5);
        let options = Options {
            dir: Some(dir.clone()),
            then: Some(ECHO.into()),
            ..Options::default()
        };
        let mut out = Vec::new();
        assert_eq!(run(&options, &input, &mut out), 0);
        assert_eq!(String::from_utf8(out).unwrap().trim(), "passthrough");
        assert_eq!(fs::read(dir.join("fwd.json")).unwrap(), input);
    }

    #[test]
    fn then_receives_the_same_document_on_stdin() {
        let dir = scratch("stdin");
        let input = document("pipe", 5000);
        let options = Options {
            dir: Some(dir),
            then: Some(COPY.into()),
            ..Options::default()
        };
        let mut out = Vec::new();
        assert_eq!(run(&options, &input, &mut out), 0);
        let sent: Value = serde_json::from_slice(&input).unwrap();
        let got: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(got, sent);
    }

    #[test]
    fn then_exit_code_passes_through_and_bad_input_still_forwards() {
        let dir = scratch("code");
        let options = Options {
            dir: Some(dir.clone()),
            then: Some("exit 3".into()),
            ..Options::default()
        };
        assert_eq!(run(&options, &document("c", 1), &mut Vec::new()), 3);

        let options = Options {
            dir: Some(dir.clone()),
            then: Some(ECHO.into()),
            ..Options::default()
        };
        let mut out = Vec::new();
        assert_eq!(run(&options, b"not json", &mut out), 0);
        assert_eq!(String::from_utf8(out).unwrap().trim(), "passthrough");
        assert!(!dir.join("unknown.json").exists(), "bad input is not saved");
    }

    #[test]
    fn without_then_it_prints_nothing_and_exits_zero() {
        let dir = scratch("alone");
        let options = Options {
            dir: Some(dir.clone()),
            ..Options::default()
        };
        let mut out = Vec::new();
        assert_eq!(run(&options, &document("alone", 1), &mut out), 0);
        assert!(out.is_empty() && dir.join("alone.json").is_file());
    }

    #[test]
    fn arguments_parse() {
        let args = |list: &[&str]| parse(list.iter().map(|arg| (*arg).to_owned()));
        let options = args(&["--dir", "d", "--then", "echo hi", "--shell", "sh"]).unwrap();
        assert_eq!(options.dir, Some(PathBuf::from("d")));
        assert_eq!(options.then.as_deref(), Some("echo hi"));
        assert_eq!(options.shell, Some(Shell::Sh));
        assert!(args(&["--then"]).is_err());
        assert!(args(&["--shell", "fish"]).is_err());
        assert!(args(&["--nope"]).is_err());
        assert!(
            status_dir(None).ends_with(Path::new(td_core::app_dirs::APP_ID).join("claude-status"))
        );

        let options = args(&["--unwrap", "--settings", "s.json"]).unwrap();
        assert!(options.unwrap && options.settings == Some(PathBuf::from("s.json")));
        assert!(args(&["--unwrap", "--then", "x"]).is_err());
        assert!(args(&["--settings", "s.json"]).is_err());
        assert!(args(&["--unwrap", "--settings"]).is_err());
    }

    #[test]
    fn unwrap_restores_the_old_command_and_then_has_nothing_to_do() {
        let dir = scratch("unwrap");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let before = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"echo old\"\n  }\n}\n";
        let edit = td_core::statusline::edit_settings(Some(before), "td-statusline").unwrap();
        fs::write(&path, &edit.text).unwrap();
        let mut out = Vec::new();
        assert_eq!(unwrap_file(&path, &mut out), 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
        assert!(String::from_utf8(out).unwrap().contains("restored"));
        let mut out = Vec::new();
        assert_eq!(unwrap_file(&path, &mut out), 0);
        assert!(String::from_utf8(out).unwrap().contains("nothing to do"));
        assert_eq!(unwrap_file(&dir.join("missing.json"), &mut Vec::new()), 0);
        assert!(
            !dir.join("missing.json").exists(),
            "a missing file stays missing"
        );
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(unwrap_file(&path, &mut Vec::new()), 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
    }
}
