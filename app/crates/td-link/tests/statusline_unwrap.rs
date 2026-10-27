//! `td-statusline --unwrap` as an uninstaller runs it: no console and nothing on stdin.

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SHIM: &str = env!("CARGO_BIN_EXE_td-statusline");
const BEFORE: &str = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"echo old\",\n    \"padding\": 0\n  }\n}\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("td-unwrap-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".claude")).unwrap();
    dir
}

/// Waits for the child without ever writing to or closing its stdin.
fn finish(mut child: Child) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().unwrap_or(-1);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("td-statusline --unwrap waited on stdin");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn install(path: &PathBuf) {
    let edit = td_core::statusline::edit_settings(Some(BEFORE), &format!("\"{SHIM}\"")).unwrap();
    fs::write(path, edit.text).unwrap();
}

#[test]
fn unwrap_with_stdin_closed_restores_the_old_command_promptly() {
    let home = scratch("closed");
    let settings = home.join(".claude").join("settings.json");
    install(&settings);
    let child = Command::new(SHIM)
        .args(["--unwrap", "--settings"])
        .arg(&settings)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert_eq!(finish(child), 0);
    assert_eq!(fs::read_to_string(&settings).unwrap(), BEFORE);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn unwrap_never_reads_an_open_stdin_and_defaults_to_claude_config_dir() {
    let home = scratch("open");
    let settings = home.join(".claude").join("settings.json");
    install(&settings);
    // The pipe stays open and empty for the whole run, so a read would block until the deadline.
    let child = Command::new(SHIM)
        .arg("--unwrap")
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert_eq!(finish(child), 0);
    assert_eq!(fs::read_to_string(&settings).unwrap(), BEFORE);

    // A second run has nothing to do and still exits 0.
    let child = Command::new(SHIM)
        .arg("--unwrap")
        .env("CLAUDE_CONFIG_DIR", home.join(".claude"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert_eq!(finish(child), 0);
    assert_eq!(fs::read_to_string(&settings).unwrap(), BEFORE);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn unwrap_reports_bad_arguments_and_bad_files() {
    let home = scratch("bad");
    let run = |args: &[&str]| {
        let child = Command::new(SHIM)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        finish(child)
    };
    assert_eq!(run(&["--unwrap", "--then", "x"]), 2);
    let broken = home.join("broken.json");
    fs::write(&broken, "{ not json").unwrap();
    assert_eq!(
        run(&["--unwrap", "--settings", broken.to_str().unwrap()]),
        1
    );
    assert_eq!(fs::read_to_string(&broken).unwrap(), "{ not json");
    fs::remove_dir_all(home).unwrap();
}
