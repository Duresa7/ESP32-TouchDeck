//! The tile actions the PC runs: `launch`, `open`, and `command`. Every other action type runs on
//! the deck.

use std::io;
use std::process::{Command, ExitStatus, Stdio};

use td_core::layout::{Action, Layout, Page, Tile, TileRender};

use crate::approval::CommandSpec;

/// What a press on a tile asks of the PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Launch {
        path: String,
        args: Vec<String>,
        cwd: String,
    },
    Open {
        target: String,
    },
    Command(CommandSpec),
    /// The deck handles this action itself.
    Deck,
    /// A renderer control on a Live tile: `action` runs on the Integration the Tile's widget action binds.
    Integration {
        source: String,
        account: String,
        action: String,
    },
}

impl Plan {
    pub fn of(action: &Action) -> Plan {
        match action {
            Action::Launch { path, args, cwd } => Plan::Launch {
                path: path.clone(),
                args: args.clone(),
                cwd: cwd.clone(),
            },
            Action::Open { target } => Plan::Open {
                target: target.clone(),
            },
            Action::Command {
                program,
                args,
                cwd,
                shell,
            } => Plan::Command(CommandSpec {
                program: program.clone(),
                args: args.clone(),
                cwd: cwd.clone(),
                shell: *shell,
            }),
            _ => Plan::Deck,
        }
    }

    /// What a tap that lifted inside a renderer control asks of the PC. Only a `widget` action with a source
    /// binds one, and only the parts its renderer draws: `previous`, `play_pause` and `next` on `now-playing`,
    /// `mute` on a `bar` that declares `muted`. The part goes to the Integration unchanged.
    pub fn part(tile: &Tile, part: &str) -> Option<Plan> {
        let Action::Widget {
            source, account, ..
        } = &tile.action
        else {
            return None;
        };
        let drawn = match tile.render? {
            TileRender::NowPlaying => ["previous", "play_pause", "next"].contains(&part),
            TileRender::Bar => {
                part == "mute" && tile.streams.as_ref()?.contains_key("muted")
            }
            TileRender::Gif | TileRender::Graph => false,
        };
        (drawn && !source.is_empty()).then(|| Plan::Integration {
            source: source.clone(),
            account: account.clone(),
            action: part.to_owned(),
        })
    }
}

pub fn find_tile<'a>(layout: &'a Layout, page: &str, tile: &str) -> Option<&'a Tile> {
    layout.pages.iter().find_map(|candidate| match candidate {
        Page::Grid { id, tiles, .. } if id == page => tiles.iter().find(|t| t.id == tile),
        _ => None,
    })
}

/// Starts a program and returns its process id without waiting for it.
pub fn launch(path: &str, args: &[String], cwd: &str) -> io::Result<u32> {
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !cwd.is_empty() {
        command.current_dir(cwd);
    }
    let child = command.spawn()?;
    Ok(child.id())
}

/// Opens a file, folder, or URL in its default app.
pub fn open(target: &str) -> io::Result<()> {
    open::that_detached(target)
}

/// Builds the process for an approved command, through the shell when `shell` is set.
pub fn command(spec: &CommandSpec) -> io::Result<Command> {
    let mut command = if spec.shell {
        shell(spec)?
    } else {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command
    };
    if !spec.cwd.is_empty() {
        command.current_dir(&spec.cwd);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

#[cfg(windows)]
fn shell(spec: &CommandSpec) -> io::Result<Command> {
    use std::os::windows::process::CommandExt;
    let line = std::iter::once(spec.program.as_str())
        .chain(spec.args.iter().map(String::as_str))
        .map(windows_argument)
        .collect::<Vec<_>>()
        .join(" ");
    if line.encode_utf16().count() >= 8191 || line.contains(['\0', '\r', '\n']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shell command exceeds cmd's limits",
        ));
    }
    let mut command = Command::new("cmd");
    // Delayed expansion happens after cmd parses operators. The values are a native argv
    // command line, never shell source; %, !, &, and redirection remain literal data.
    command
        .args(["/D", "/V:ON", "/S", "/C"])
        .raw_arg("!TOUCHDECK_APPROVED_COMMAND!")
        .env("TOUCHDECK_APPROVED_COMMAND", line);
    Ok(command)
}

#[cfg(windows)]
fn windows_argument(value: &str) -> String {
    let quoted = value.is_empty() || value.contains([' ', '\t']);
    let mut result = String::new();
    if quoted {
        result.push('"');
    }
    let mut slashes = 0;
    for character in value.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        if character == '"' {
            result.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n('\\', slashes));
        }
        result.push(character);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n(
        '\\',
        slashes * if quoted { 2 } else { 1 },
    ));
    if quoted {
        result.push('"');
    }
    result
}

#[cfg(not(windows))]
fn shell(spec: &CommandSpec) -> io::Result<Command> {
    let mut command = Command::new("sh");
    // The shell parses only this fixed script. Program and arguments are positional data.
    command
        .args(["-c", "exec \"$@\"", "touchdeck", &spec.program])
        .args(&spec.args);
    Ok(command)
}

/// Runs an approved command to its end.
pub fn run(spec: &CommandSpec) -> io::Result<ExitStatus> {
    command(spec)?.status()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> Layout {
        let bytes = include_bytes!("../../../../schema/examples/dev.json");
        Layout::parse(bytes).unwrap()
    }

    fn page_of(layout: &Layout, tile: &str) -> String {
        layout
            .pages
            .iter()
            .find_map(|page| match page {
                Page::Grid { id, tiles, .. } if tiles.iter().any(|t| t.id == tile) => {
                    Some(id.clone())
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn dev_json_app_tiles_plan_their_action() {
        let layout = dev();
        let tile = |id: &str| find_tile(&layout, &page_of(&layout, id), id).unwrap();
        assert!(matches!(
            Plan::of(&tile("launch").action),
            Plan::Launch { .. }
        ));
        assert!(matches!(Plan::of(&tile("open").action), Plan::Open { .. }));
        assert_eq!(
            Plan::of(&tile("command").action),
            Plan::Command(CommandSpec {
                program: "cmd".into(),
                args: vec!["/c".into(), "echo".into(), "TouchDeck".into()],
                cwd: String::new(),
                shell: false
            })
        );
        assert_eq!(Plan::of(&tile("f5").action), Plan::Deck);
    }

    #[test]
    fn a_tile_on_another_page_is_not_found() {
        let layout = dev();
        assert!(find_tile(&layout, "nope", "launch").is_none());
        assert!(find_tile(&layout, &page_of(&layout, "launch"), "nope").is_none());
    }

    #[test]
    fn a_command_reports_its_exit_code() {
        let exit = |code: i32| CommandSpec {
            program: if cfg!(windows) { "cmd" } else { "sh" }.into(),
            args: if cfg!(windows) {
                vec!["/C".into(), format!("exit {code}")]
            } else {
                vec!["-c".into(), format!("exit {code}")]
            },
            cwd: String::new(),
            shell: false,
        };
        assert_eq!(run(&exit(0)).unwrap().code(), Some(0));
        assert_eq!(run(&exit(3)).unwrap().code(), Some(3));
    }

    #[test]
    fn a_shell_command_runs_through_the_shell_in_its_directory() {
        let dir = std::env::temp_dir().join(format!("td-host-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = CommandSpec {
            program: if cfg!(windows) { "cmd" } else { "sh" }.into(),
            args: vec![
                if cfg!(windows) { "/C" } else { "-c" }.into(),
                "echo marker>out.txt".into(),
            ],
            cwd: dir.to_string_lossy().into_owned(),
            shell: true,
        };
        assert!(run(&spec).unwrap().success());
        let text = std::fs::read_to_string(dir.join("out.txt")).unwrap();
        assert_eq!(text.trim(), "marker");
    }

    #[test]
    fn shell_arguments_cannot_inject_a_second_command() {
        let dir = std::env::temp_dir().join(format!("td-host-injection-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("injected.txt");
        let _ = std::fs::remove_file(&marker);
        let spec = CommandSpec {
            program: "echo".into(),
            args: vec!["safe & echo injected > injected.txt".into()],
            cwd: dir.to_string_lossy().into_owned(),
            shell: true,
        };
        assert!(run(&spec).unwrap().success());
        assert!(!marker.exists(), "an argument executed as shell source");
    }

    fn renderer(render: &str, streams: serde_json::Value, action: serde_json::Value) -> Tile {
        serde_json::from_value(serde_json::json!({
            "id": "media", "label": "Player", "row": 0, "col": 0, "rowSpan": 1, "colSpan": 1,
            "render": render, "streams": streams, "action": action,
        }))
        .unwrap()
    }

    fn widget(source: &str, account: &str) -> serde_json::Value {
        serde_json::json!({"type": "widget", "source": source, "account": account, "field": "state"})
    }

    fn now_playing() -> serde_json::Value {
        serde_json::json!({"title": "text", "artist": "text", "cover": "image", "state": "number"})
    }

    fn integration(source: &str, account: &str, action: &str) -> Option<Plan> {
        Some(Plan::Integration {
            source: source.into(),
            account: account.into(),
            action: action.into(),
        })
    }

    #[test]
    fn a_now_playing_part_runs_on_the_widget_source_and_account() {
        let tile = renderer("now-playing", now_playing(), widget("media", "chosen"));
        for part in ["previous", "play_pause", "next"] {
            assert_eq!(Plan::part(&tile, part), integration("media", "chosen", part));
        }
        for part in ["mute", "launch", "", "NEXT", "next ", "play_pause\u{0}"] {
            assert_eq!(Plan::part(&tile, part), None, "{part:?}");
        }
    }

    #[test]
    fn a_bar_has_a_mute_part_only_when_it_declares_muted() {
        let action = widget("volume", "");
        let streams = serde_json::json!({"value": "number", "muted": "number"});
        let muted = renderer("bar", streams, action.clone());
        assert_eq!(Plan::part(&muted, "mute"), integration("volume", "", "mute"));
        assert_eq!(Plan::part(&muted, "next"), None);
        let plain = renderer("bar", serde_json::json!({"value": "number"}), action);
        assert_eq!(Plan::part(&plain, "mute"), None);
    }

    #[test]
    fn a_part_never_reaches_a_tile_that_starts_a_process_or_lacks_a_source() {
        let mut tile = renderer("now-playing", now_playing(), widget("", ""));
        assert_eq!(Plan::part(&tile, "next"), None);
        for action in [
            serde_json::json!({"type": "launch", "path": "example.exe", "args": [], "cwd": ""}),
            serde_json::json!({"type": "open", "target": "https://example.com"}),
            serde_json::json!({"type": "command", "program": "cmd", "args": [], "cwd": "", "shell": true}),
        ] {
            tile.action = serde_json::from_value(action).unwrap();
            assert_eq!(Plan::part(&tile, "next"), None);
        }
        let graph = renderer("graph", serde_json::json!({"value": "series"}), widget("media", ""));
        assert_eq!(Plan::part(&graph, "next"), None);
        let mut plain = graph;
        plain.render = None;
        plain.streams = None;
        assert_eq!(Plan::part(&plain, "next"), None);
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let spec = CommandSpec {
            program: "td-host-no-such-program".into(),
            args: vec![],
            cwd: String::new(),
            shell: false,
        };
        assert!(run(&spec).is_err());
        assert!(launch("td-host-no-such-program", &[], "").is_err());
    }
}

