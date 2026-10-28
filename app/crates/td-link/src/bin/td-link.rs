use std::process::ExitCode;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use td_core::{Layout, Orientation, Pong, Request, Settings};
use td_link::asset::{asset_space, delete_asset, list_assets, put_asset};
use td_link::discover::{Candidate, candidate_ports, discover, probe_transport};
use td_link::mock::MOCK_PORT;
use td_link::xfer::{check_layout_orientation, get_layout, put_layout};
use td_link::{AssetFormat, Incoming, MockDeck, Serial, Session, SessionConfig, Transport};

const USAGE: &str = "usage: td-link [--port <port> | --mock] [--timeout <ms>] <command>
commands:
  discover               probe every Espressif USB-Serial/JTAG port with hello
  discover-identities    probe identities and verify USB serial association
  identity               negotiate and print Device identity
  name                   read the Device name
  set-name <name>        store the Device name
  hello                  print the deck's hello reply
  ping [--count <n>]     send n pings and print how many replies matched
  console \"<line>\"       send one console line and print its @td reply
  events [--seconds <n>] print each event as one JSON line
  put-layout <file> [--orientation landscape|portrait]
                         validate a layout and push it; orientation defaults to the file's
  get-layout [--orientation <o>] [--out <file>]
                         pull the stored layout and write it to a file or stdout
  settings get           print the deck settings
  settings put [--idle <s>] [--orientation <o>]
                         store settings; unnamed fields keep the deck's values
  asset put <file> [--format tdi1|gif|jpeg]
                         store an Asset unless the Device has it; a PNG becomes a 64 by 64 tdi1
  asset ls               list the Assets on the Device
  asset rm <id>          remove one Asset by its SHA-256 id
  asset df               print the Device's space for Assets
  update <bundle>        update firmware over the link
  recover <bundle>       restore firmware through USB-Serial/JTAG
  reboot                 restart the deck through its console";

struct Args {
    port: Option<String>,
    mock: bool,
    timeout: Duration,
    command: String,
    rest: Vec<String>,
}

fn parse(mut argv: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        port: None,
        mock: false,
        timeout: Duration::from_millis(500),
        command: String::new(),
        rest: Vec::new(),
    };
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--port" => args.port = Some(argv.next().ok_or("--port needs a value")?),
            "--mock" => args.mock = true,
            "--timeout" => {
                let ms: u64 = argv
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|&ms| ms > 0)
                    .ok_or("--timeout needs milliseconds")?;
                args.timeout = Duration::from_millis(ms);
            }
            _ if args.command.is_empty() => args.command = arg,
            _ => args.rest.push(arg),
        }
    }
    if args.command.is_empty() {
        return Err("no command".into());
    }
    Ok(args)
}

fn option<T: std::str::FromStr>(rest: &[String], name: &str, default: T) -> Result<T, String> {
    match rest.iter().position(|arg| arg == name) {
        Some(index) => rest
            .get(index + 1)
            .and_then(|v| v.parse().ok())
            .ok_or(format!("{name} needs a positive number")),
        None => Ok(default),
    }
}

fn text_option<'a>(rest: &'a [String], name: &str) -> Result<Option<&'a str>, String> {
    match rest.iter().position(|arg| arg == name) {
        Some(index) => rest
            .get(index + 1)
            .map(|v| Some(v.as_str()))
            .ok_or(format!("{name} needs a value")),
        None => Ok(None),
    }
}

fn orientation(rest: &[String]) -> Result<Option<Orientation>, String> {
    match text_option(rest, "--orientation")? {
        None => Ok(None),
        Some("landscape") => Ok(Some(Orientation::Landscape)),
        Some("portrait") => Ok(Some(Orientation::Portrait)),
        Some(other) => Err(format!(
            "--orientation is landscape or portrait, not {other:?}"
        )),
    }
}

fn transport(args: &Args) -> Result<Box<dyn Transport>, String> {
    if args.mock {
        // Assets need protocol 2, so the asset commands talk to a v2 mock Device.
        let device = if args.command == "asset" {
            MockDeck::v2()
        } else {
            MockDeck::new()
        };
        return Ok(Box::new(device.connect()));
    }
    let port = match &args.port {
        Some(port) => port.clone(),
        None => match candidate_ports().as_slice() {
            [port] => port.clone(),
            ports => {
                return Err(format!(
                    "expected one Espressif USB-Serial/JTAG port, found {ports:?}; pass --port"
                ));
            }
        },
    };
    Serial::open(&port)
        .map(|serial| Box::new(serial) as Box<dyn Transport>)
        .map_err(|error| format!("{port}: {error}"))
}

fn open(args: &Args, config: SessionConfig) -> Result<(Session, Receiver<Incoming>), String> {
    Session::open(transport(args)?, config).map_err(|error| error.to_string())
}

fn quiet() -> SessionConfig {
    SessionConfig {
        ping_every: None,
        ..SessionConfig::default()
    }
}

fn print(value: &Value) {
    println!("{}", serde_json::to_string(value).expect("JSON prints"));
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The bytes and format to store for a file: a PNG goes through the icon packer to a 64 by 64
/// tdi1, and anything else is stored as it is, in the format named or sniffed from its first bytes.
fn asset_file(path: &str, format: Option<&str>) -> Result<(Vec<u8>, AssetFormat), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
    let named = format
        .map(|name| {
            AssetFormat::parse(name).ok_or(format!("--format is tdi1, gif or jpeg, not {name:?}"))
        })
        .transpose()?;
    if bytes.starts_with(PNG_SIGNATURE) && named.is_none_or(|format| format == AssetFormat::Tdi1) {
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|error| format!("{path}: {error}"))?
            .to_rgba8();
        return Ok((td_host::icon_pack::pack_any(&image), AssetFormat::Tdi1));
    }
    let format = named.or_else(|| AssetFormat::sniff(&bytes)).ok_or(format!(
        "{path}: not a PNG, tdi1, GIF or JPEG file; name one with --format"
    ))?;
    Ok((bytes, format))
}

/// Opens a session and selects protocol 2, which Assets need.
fn open_v2(args: &Args) -> Result<Session, String> {
    let (session, _) = open(args, quiet())?;
    let identity = session
        .identity(args.timeout)
        .map_err(|error| error.to_string())?;
    if identity.protocol != 2 {
        return Err(
            "the Device runs protocol 1 Firmware, which stores no Assets; update it".into(),
        );
    }
    Ok(session)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted
        .get(((sorted.len() as f64 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

fn run(args: &Args) -> Result<bool, String> {
    match args.command.as_str() {
        "discover-identities" => {
            print(&json!(td_link::discover::discover_identities()));
            Ok(true)
        }
        "update" | "recover" => {
            let path = args
                .rest
                .first()
                .ok_or("update/recover needs a .tdfw bundle")?;
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let bundle = td_link::bundle::Bundle::parse(&bytes)?;
            let port = match &args.port {
                Some(p) => p.clone(),
                None if args.mock => MOCK_PORT.into(),
                None => match candidate_ports().as_slice() {
                    [p] => p.clone(),
                    _ => return Err("select one port with --port".into()),
                },
            };
            let mut connector: Box<dyn td_link::update::Connector> = if args.mock {
                Box::new(MockDeck::new())
            } else {
                Box::new(td_link::update::PortConnector(&port))
            };
            let mut recovery = td_link::recover::RomRecovery(&port);
            let recovery = if args.command == "recover" {
                if args.mock {
                    return Err("ROM recovery needs a real USB-Serial/JTAG port".into());
                }
                Some(&mut recovery as &mut dyn td_link::recover::Recovery)
            } else {
                None
            };
            Ok(td_link::update::run(
                &mut *connector,
                recovery,
                &bundle,
                td_link::update::Deadlines::default(),
                &mut |state| print(&serde_json::to_value(state).unwrap()),
            )
            .is_ok())
        }
        "discover" => {
            let candidates = if args.mock {
                vec![Candidate {
                    port: MOCK_PORT.into(),
                    probe: probe_transport(Box::new(MockDeck::new().connect())),
                }]
            } else {
                discover()
            };
            print(&serde_json::to_value(candidates).expect("candidates serialize"));
            Ok(true)
        }
        "identity" | "name" | "set-name" => {
            let (session, _) = open(args, quiet())?;
            let value = match args.command.as_str() {
                "identity" => json!(session.identity(args.timeout).map_err(|e| e.to_string())?),
                "name" => json!({"name": session.name(args.timeout).map_err(|e| e.to_string())?}),
                _ => {
                    json!({"name": session.set_name(args.rest.first().ok_or("set-name needs a name")?, args.timeout).map_err(|e| e.to_string())?})
                }
            };
            print(&value);
            Ok(true)
        }
        "hello" => {
            let (session, _) = open(args, quiet())?;
            let reply = session
                .request(&Request::Hello, args.timeout)
                .map_err(|error| error.to_string())?;
            print(&reply.to_value());
            Ok(reply.ok)
        }
        "ping" => {
            let count: u64 = option(&args.rest, "--count", 1)?;
            let (session, _) = open(args, quiet())?;
            let (mut matched, mut rtt) = (0u64, Vec::new());
            for n in 0..count {
                let started = Instant::now();
                let reply = session
                    .request(&Request::Ping { n: Some(n) }, args.timeout)
                    .map_err(|error| format!("ping {n}: {error}"))?;
                rtt.push(started.elapsed().as_secs_f64() * 1000.0);
                if reply.fields::<Pong>().is_ok_and(|pong| pong.n == Some(n)) {
                    matched += 1;
                }
            }
            rtt.sort_by(f64::total_cmp);
            let ms = |p| (percentile(&rtt, p) * 1000.0).round() / 1000.0;
            print(
                &json!({"ok": matched == count, "count": count, "matched": matched, "rtt_ms": {"p50": ms(0.5), "p95": ms(0.95), "max": ms(1.0)}}),
            );
            Ok(matched == count)
        }
        "console" => {
            let line = args.rest.first().ok_or("console needs a line")?;
            let (session, incoming) = open(args, quiet())?;
            session.send_line(line).map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(2).max(args.timeout);
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                if let Ok(Incoming::Line(text)) = incoming.recv_timeout(left)
                    && let Some(at) = text.find("@td {")
                {
                    let reply: Value = serde_json::from_str(&text[at + 4..])
                        .map_err(|error| format!("bad @td reply: {error}"))?;
                    print(&reply);
                    return Ok(reply["ok"] == true);
                }
            }
            Err(format!("no @td reply to {line:?} within 2 s"))
        }
        "events" => {
            let seconds: u64 = option(&args.rest, "--seconds", 10)?;
            let (session, incoming) = open(args, SessionConfig::default())?;
            session
                .hello(args.timeout)
                .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(seconds);
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                match incoming.recv_timeout(left) {
                    Ok(Incoming::Event(event)) => print(&event),
                    Ok(Incoming::Closed(reason)) => return Err(reason),
                    _ => {}
                }
            }
            Ok(true)
        }
        "put-layout" => {
            let path = args.rest.first().ok_or("put-layout needs a file")?;
            let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
            let layout = Layout::parse(&bytes).map_err(|errors| {
                let all: Vec<String> = errors.iter().map(ToString::to_string).collect();
                format!("{path} is not a valid layout: {}", all.join("; "))
            })?;
            let orientation = orientation(&args.rest)?.unwrap_or(layout.orientation);
            check_layout_orientation(&layout, orientation).map_err(|error| error.to_string())?;
            let (session, _) = open(args, quiet())?;
            let report =
                put_layout(&session, orientation, &layout).map_err(|error| error.to_string())?;
            let mut out = serde_json::to_value(&report).expect("reports serialize");
            out["ok"] = json!(true);
            print(&out);
            Ok(true)
        }
        "get-layout" => {
            let orientation = orientation(&args.rest)?.unwrap_or(Orientation::Landscape);
            let (session, _) = open(args, quiet())?;
            let report = get_layout(&session, orientation).map_err(|error| error.to_string())?;
            match text_option(&args.rest, "--out")? {
                Some(out) => {
                    std::fs::write(out, &report.data).map_err(|error| format!("{out}: {error}"))?;
                    let mut summary = serde_json::to_value(&report).expect("reports serialize");
                    summary["ok"] = json!(true);
                    summary["out"] = json!(out);
                    print(&summary);
                }
                None => println!("{}", String::from_utf8_lossy(&report.data)),
            }
            Ok(true)
        }
        "settings" => {
            let (session, _) = open(args, quiet())?;
            let current: Settings = session
                .request(&Request::SettingsGet, args.timeout)
                .and_then(|reply| reply.fields().map_err(td_link::LinkError::Reply))
                .map_err(|error| error.to_string())?;
            match args.rest.first().map(String::as_str) {
                Some("get") | None => {
                    print(&serde_json::to_value(current).expect("settings serialize"));
                    Ok(true)
                }
                Some("put") => {
                    let settings = Settings {
                        idle_timeout_s: option(&args.rest, "--idle", current.idle_timeout_s)?,
                        orientation: orientation(&args.rest)?.unwrap_or(current.orientation),
                    };
                    let reply = session
                        .request(&Request::SettingsPut(settings), args.timeout)
                        .map_err(|error| error.to_string())?;
                    print(&reply.to_value());
                    Ok(reply.ok)
                }
                Some(other) => Err(format!("settings takes get or put, not {other:?}")),
            }
        }
        "asset" => {
            let error = |error: td_link::AssetError| error.to_string();
            match args.rest.first().map(String::as_str) {
                Some("put") => {
                    let path = args.rest.get(1).ok_or("asset put needs a file")?;
                    let (data, format) = asset_file(path, text_option(&args.rest, "--format")?)?;
                    let session = open_v2(args)?;
                    let report = put_asset(&session, &data, format).map_err(error)?;
                    let mut out = serde_json::to_value(&report).expect("reports serialize");
                    out["ok"] = json!(true);
                    print(&out);
                }
                Some("ls") => {
                    let session = open_v2(args)?;
                    let items = list_assets(&session).map_err(error)?;
                    print(&json!({"ok": true, "count": items.len(), "items": items}));
                }
                Some("rm") => {
                    let id = args.rest.get(1).ok_or("asset rm needs an id")?;
                    let session = open_v2(args)?;
                    let deleted = delete_asset(&session, id).map_err(error)?;
                    print(&json!({"ok": true, "id": id, "deleted": deleted}));
                }
                Some("df") => {
                    let session = open_v2(args)?;
                    let space = asset_space(&session).map_err(error)?;
                    let mut out = serde_json::to_value(space).expect("space serializes");
                    out["ok"] = json!(true);
                    print(&out);
                }
                other => return Err(format!("asset takes put, ls, rm or df, not {other:?}")),
            }
            Ok(true)
        }
        "reboot" => {
            let (session, incoming) = open(args, quiet())?;
            session
                .send_line("reboot")
                .map_err(|error| error.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(2).max(args.timeout);
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                match incoming.recv_timeout(left) {
                    Ok(Incoming::Line(text)) if text.contains("@td {") => {
                        let at = text.find("@td {").expect("just found");
                        let reply: Value = serde_json::from_str(&text[at + 4..])
                            .map_err(|error| format!("bad @td reply: {error}"))?;
                        print(&reply);
                        return Ok(reply["ok"] == true);
                    }
                    Ok(Incoming::Closed(reason)) => {
                        print(&json!({"ok": true, "closed": reason}));
                        return Ok(true);
                    }
                    _ => {}
                }
            }
            Err("no reply to reboot within 2 s".into())
        }
        other => Err(format!("unknown command {other:?}\n{USAGE}")),
    }
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            print(&json!({"ok": false, "error": error}));
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_becomes_a_tdi1_asset_and_other_files_keep_their_bytes() {
        let dir = std::env::temp_dir().join(format!("td-link-asset-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("wide.png");
        image::RgbaImage::from_pixel(128, 32, image::Rgba([255, 0, 0, 255]))
            .save(&png)
            .unwrap();
        let (data, format) = asset_file(png.to_str().unwrap(), None).unwrap();
        assert_eq!((data.len(), format), (12300, AssetFormat::Tdi1));
        assert!(data.starts_with(b"TDI1"));

        let gif = dir.join("raw.gif");
        std::fs::write(&gif, b"GIF89a-not-validated-here").unwrap();
        let (data, format) = asset_file(gif.to_str().unwrap(), None).unwrap();
        assert_eq!(
            (data.as_slice(), format),
            (&b"GIF89a-not-validated-here"[..], AssetFormat::Gif)
        );

        let unknown = dir.join("notes.txt");
        std::fs::write(&unknown, b"hello").unwrap();
        assert!(asset_file(unknown.to_str().unwrap(), None).is_err());
        assert!(asset_file(unknown.to_str().unwrap(), Some("bmp")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn asset_commands_run_against_the_v2_mock() {
        let dir = std::env::temp_dir().join(format!("td-link-asset-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("icon.png");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 128, 255, 255]))
            .save(&png)
            .unwrap();
        let args = |rest: &[&str]| {
            parse(
                ["--mock", "asset"]
                    .iter()
                    .chain(rest)
                    .map(|arg| (*arg).to_owned()),
            )
            .unwrap()
        };
        assert_eq!(run(&args(&["put", png.to_str().unwrap()])), Ok(true));
        assert_eq!(run(&args(&["ls"])), Ok(true));
        assert_eq!(run(&args(&["df"])), Ok(true));
        assert_eq!(run(&args(&["rm", &"0".repeat(64)])), Ok(true));
        assert!(run(&args(&["rm", "zz"])).is_err());
        assert!(run(&args(&["mv"])).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_mismatched_orientation_override_is_refused_before_opening_a_port() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../schema/examples/dev.json");
        let args = parse(
            [
                "--port".to_owned(),
                "no-port-should-be-opened".to_owned(),
                "put-layout".to_owned(),
                path.display().to_string(),
                "--orientation".to_owned(),
                "portrait".to_owned(),
            ]
            .into_iter(),
        )
        .unwrap();
        let error = run(&args).unwrap_err();
        assert!(
            error.contains("declares landscape") && error.contains("target slot is portrait"),
            "{error}"
        );
    }
}
