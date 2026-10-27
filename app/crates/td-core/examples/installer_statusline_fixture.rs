//! Runner-only fixture: use the app's status-line editor, never duplicate its wrapper format.
use std::{env, fs, path::PathBuf, process::ExitCode};

use td_core::{app_dirs::write_atomic, statusline::edit_settings};

fn run() -> Result<(), String> {
    if env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || env::var("RUNNER_ENVIRONMENT").as_deref() != Ok("github-hosted")
    {
        return Err("this fixture writes user settings and requires a GitHub-hosted runner".into());
    }
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "usage: installer_statusline_fixture <settings.json> <installed helper>".into(),
        );
    }
    let settings = PathBuf::from(&args[0]);
    let helper = PathBuf::from(&args[1]);
    if !helper.is_absolute() || !helper.is_file() {
        return Err("the installed helper must exist at an absolute path".into());
    }
    // Sources' shim_command quotes the absolute sibling path when it contains spaces.
    let path = helper.to_string_lossy();
    let shim = if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path.into_owned()
    };
    let original = fs::read_to_string(&settings).map_err(|_| "cannot read settings fixture")?;
    let edit = edit_settings(Some(&original), &shim)?;
    if !edit.changed {
        return Err("the fixture must start with an unwrapped command".into());
    }
    fs::write(
        settings.with_file_name("settings.json.touchdeck.bak"),
        &original,
    )
    .map_err(|_| "cannot back up settings fixture")?;
    write_atomic(&settings, edit.text.as_bytes()).map_err(|_| "cannot wrap settings fixture")?;
    println!("status-line fixture wrapped with td_core::statusline::edit_settings");
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("status-line fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
