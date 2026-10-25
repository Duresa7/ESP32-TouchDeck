use std::process::ExitCode;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use td_core::{Pong, Request};
use td_link::discover::{Candidate, candidate_ports, discover, probe_transport};
use td_link::mock::MOCK_PORT;
use td_link::{Incoming, MockDeck, Serial, Session, SessionConfig, Transport};

const USAGE: &str = "usage: td-link [--port <port> | --mock] [--timeout <ms>] <command>
commands:
  discover               probe every Espressif USB-Serial/JTAG port with hello
  hello                  print the deck's hello reply
  ping [--count <n>]     send n pings and print how many replies matched
  console \"<line>\"       send one console line and print its @td reply
  events [--seconds <n>] print each event as one JSON line";

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

fn transport(args: &Args) -> Result<Box<dyn Transport>, String> {
    if args.mock {
        return Ok(Box::new(MockDeck::new().connect()));
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

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted
        .get(((sorted.len() as f64 - 1.0) * p).round() as usize)
        .copied()
        .unwrap_or(0.0)
}

fn run(args: &Args) -> Result<bool, String> {
    match args.command.as_str() {
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
