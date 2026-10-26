//! The status line shim.
//!
//! Claude Code pipes a JSON document to its status line command. This command saves the
//! document to `<state dir>/touchdeck/claude-status/<session_id>.json`, where the app reads
//! it, and with `--then "<command>"` also pipes the same document to that command and prints
//! its stdout, so an existing status line keeps working. It exits with the command's code,
//! and with 0 when there is none, so it never breaks the status line.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;

use serde_json::Value;

const USAGE: &str = "usage: td-statusline [--dir <dir>] [--then \"<command>\"] [--shell cmd|sh]
  reads Claude Code's status line JSON on stdin and writes it to
  <state dir>/touchdeck/claude-status/<session_id>.json
  --dir <dir>        write to this directory instead
  --then \"<cmd>\"     pipe the same JSON to cmd and print its stdout
  --shell cmd|sh     the shell for --then: cmd on Windows, sh elsewhere by default
state dir: $TOUCHDECK_STATE_DIR, else %LOCALAPPDATA% on Windows and
$XDG_STATE_HOME or ~/.local/state elsewhere";

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
            _ => return Err(format!("unknown argument {arg:?}")),
        }
    }
    Ok(options)
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Mirrors `td_host::sources::state_base`, so the app finds what this writes.
fn state_base() -> PathBuf {
    env_dir("TOUCHDECK_STATE_DIR")
        .or_else(|| {
            if cfg!(windows) {
                env_dir("LOCALAPPDATA")
            } else {
                env_dir("XDG_STATE_HOME").or_else(|| {
                    env_dir("HOME")
                        .or_else(|| env_dir("USERPROFILE"))
                        .map(|home| home.join(".local/state"))
                })
            }
        })
        .unwrap_or_else(std::env::temp_dir)
}

fn status_dir(explicit: Option<&Path>) -> PathBuf {
    explicit.map_or_else(
        || state_base().join("touchdeck").join("claude-status"),
        Path::to_path_buf,
    )
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

fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("td-statusline: {error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
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
            shell: None,
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
            shell: None,
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
            shell: None,
        };
        assert_eq!(run(&options, &document("c", 1), &mut Vec::new()), 3);

        let options = Options {
            dir: Some(dir.clone()),
            then: Some(ECHO.into()),
            shell: None,
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
        assert!(status_dir(None).ends_with(Path::new("touchdeck").join("claude-status")));
    }
}
