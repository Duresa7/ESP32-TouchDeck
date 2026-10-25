//! A deck that speaks protocol 1 in process, so the app and its tests run without a board.
//! It keeps the state the firmware keeps and records every message it receives.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, ErrorKind, Read, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use td_core::{Body, Event, Frame, Hello, ICON_PAGE, Item, Role, Settings, StreamDecoder};

use crate::{READ_TICK, Transport};

pub const MOCK_PORT: &str = "mock";
pub const MOCK_ELF_SHA256: &str =
    "6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b6d6f636b6465636b";
const ONLINE_FOR: Duration = Duration::from_secs(5);
const HEARTBEAT: Duration = Duration::from_secs(2);

#[derive(Debug)]
struct State {
    hello: Hello,
    settings: Settings,
    icons: BTreeSet<String>,
    widgets: BTreeMap<String, Value>,
    received: Vec<Value>,
    lines: Vec<String>,
    silent: bool,
    reply_delay: Duration,
    frames_rx: u64,
    frames_tx: u64,
    dropped: u64,
    last_rx: Option<Instant>,
    online_since_hello: bool,
    booted: Instant,
    link: Option<Sender<Vec<u8>>>,
    generation: u64,
    key_seq: u32,
    press_seq: u32,
}

#[derive(Debug, Clone)]
pub struct MockDeck {
    state: Arc<Mutex<State>>,
}

impl Default for MockDeck {
    fn default() -> Self {
        MockDeck::new()
    }
}

impl MockDeck {
    pub fn new() -> MockDeck {
        let hello = Hello {
            product: "touchdeck".into(),
            proto: td_core::PROTOCOL,
            elf_sha256: MOCK_ELF_SHA256.into(),
            version: "mock".into(),
            running: "ota_0".into(),
            sinks: vec!["log".into(), "link".into()],
            recovery: false,
            layout: "builtin".into(),
        };
        MockDeck {
            state: Arc::new(Mutex::new(State {
                hello,
                settings: Settings::default(),
                icons: BTreeSet::new(),
                widgets: BTreeMap::new(),
                received: Vec::new(),
                lines: Vec::new(),
                silent: false,
                reply_delay: Duration::ZERO,
                frames_rx: 0,
                frames_tx: 0,
                dropped: 0,
                last_rx: None,
                online_since_hello: false,
                booted: Instant::now(),
                link: None,
                generation: 0,
                key_seq: 0,
                press_seq: 0,
            })),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    /// Plugs a host into the deck. A later connect replaces the earlier one.
    pub fn connect(&self) -> MockTransport {
        let (to_deck, from_host) = mpsc::channel::<Vec<u8>>();
        let (to_host, from_deck) = mpsc::channel::<Vec<u8>>();
        let generation = {
            let mut state = self.state();
            state.generation += 1;
            state.link = Some(to_host.clone());
            state.generation
        };
        let deck = self.clone();
        thread::spawn(move || deck.run(&from_host, &to_host, generation));
        MockTransport { from_deck, to_deck }
    }

    /// Stops answering anything for good, as a deck that hung or lost its cable without an error.
    pub fn stop(&self) {
        self.state().silent = true;
    }

    /// Ends the stream, as a port that went away.
    pub fn hang_up(&self) {
        let mut state = self.state();
        state.link = None;
        state.generation += 1;
    }

    pub fn delay_replies(&self, delay: Duration) {
        self.state().reply_delay = delay;
    }

    pub fn emit(&self, event: &Event) {
        self.emit_value(&serde_json::to_value(event).expect("events serialize"));
    }

    /// Sends any event object, including ones `Event` does not name, such as `key.resync`.
    pub fn emit_value(&self, event: &Value) {
        let state = self.state();
        if let (false, Some(link)) = (state.silent, &state.link) {
            let payload = serde_json::to_vec(event).expect("events serialize");
            let _ = link.send(
                Frame::message(Role::Event, payload)
                    .encode()
                    .expect("events fit one frame"),
            );
        }
    }

    fn uptime_ms(&self) -> u64 {
        self.state().booted.elapsed().as_millis() as u64
    }

    /// Emits a key report the way the link sink does, with the next `seq`. `report` holds
    /// `type` and either `mods` and `keys` or `usage`.
    pub fn emit_key(&self, report: &Value) -> u32 {
        let seq = self.next_key_seq();
        let mut event = Map::from_iter([
            ("m".to_owned(), json!("key.report")),
            ("seq".to_owned(), json!(seq)),
            ("t".to_owned(), json!(self.uptime_ms())),
        ]);
        for field in ["type", "mods", "keys", "usage"] {
            if let Some(value) = report.get(field) {
                event.insert(field.to_owned(), value.clone());
            }
        }
        self.emit_value(&Value::Object(event));
        seq
    }

    /// Emits `key.resync` with the engine's state, as the link sink does after its queue overflowed.
    pub fn emit_resync(&self, mods: u8, keys: &[u8]) -> u32 {
        let seq = self.next_key_seq();
        let t = self.uptime_ms();
        self.emit_value(
            &json!({"m": "key.resync", "seq": seq, "t": t, "mods": mods, "keys": keys}),
        );
        seq
    }

    /// Spends `count` key `seq` numbers without sending, as reports the link sink dropped.
    pub fn drop_key_reports(&self, count: u32) {
        self.state().key_seq += count;
    }

    pub fn emit_press(&self, page: &str, tile: &str, kind: &str) -> u32 {
        let seq = {
            let mut state = self.state();
            state.press_seq += 1;
            state.press_seq
        };
        self.emit(&Event::Press {
            page: page.into(),
            tile: tile.into(),
            kind: kind.into(),
            seq,
            t: self.uptime_ms(),
        });
        seq
    }

    fn next_key_seq(&self) -> u32 {
        let mut state = self.state();
        state.key_seq += 1;
        state.key_seq
    }

    pub fn add_icons(&self, ids: impl IntoIterator<Item = String>) {
        self.state().icons.extend(ids);
    }

    pub fn received(&self) -> Vec<Value> {
        self.state().received.clone()
    }

    pub fn lines(&self) -> Vec<String> {
        self.state().lines.clone()
    }

    pub fn settings(&self) -> Settings {
        self.state().settings
    }

    pub fn widgets(&self) -> BTreeMap<String, Value> {
        self.state().widgets.clone()
    }

    fn run(&self, from_host: &Receiver<Vec<u8>>, to_host: &Sender<Vec<u8>>, generation: u64) {
        let mut decoder = StreamDecoder::default();
        let mut line = Vec::new();
        let mut heartbeat = (Instant::now(), 0u64);
        loop {
            if self.state().generation != generation {
                return;
            }
            let bytes = match from_host.recv_timeout(READ_TICK) {
                Ok(bytes) => bytes,
                Err(RecvTimeoutError::Timeout) => Vec::new(),
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if self.state().silent {
                continue;
            }
            if heartbeat.0.elapsed() >= HEARTBEAT {
                let uptime = self.state().booted.elapsed().as_millis();
                let _ = to_host.send(
                    format!(
                        "I ({uptime}) touchdeck: alive {}, free heap 4194304 bytes\r\n",
                        heartbeat.1
                    )
                    .into_bytes(),
                );
                heartbeat = (Instant::now(), heartbeat.1 + 1);
            }
            for item in decoder.feed(&bytes, Instant::now()) {
                let out = match item {
                    Item::Frame(frame) => self.answer_frame(frame),
                    Item::Text(text) => self.answer_text(&text, &mut line),
                };
                if !out.is_empty() {
                    let delay = self.state().reply_delay;
                    thread::sleep(delay);
                    if to_host.send(out).is_err() {
                        return;
                    }
                }
            }
        }
    }

    fn answer_text(&self, text: &[u8], line: &mut Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        for &byte in text {
            if byte != b'\n' && byte != b'\r' {
                line.push(byte);
                continue;
            }
            let command = String::from_utf8_lossy(line).into_owned();
            line.clear();
            let Some(word) = command.split(' ').find(|word| !word.is_empty()) else {
                continue;
            };
            let reply = match word {
                "version" => {
                    let hello = self.state().hello.clone();
                    json!({"cmd": "version", "ok": true, "project_name": "touchdeck", "version": hello.version, "elf_sha256": hello.elf_sha256})
                }
                "link" => {
                    let mut reply = Map::from_iter([
                        ("cmd".to_owned(), json!("link")),
                        ("ok".to_owned(), json!(true)),
                    ]);
                    reply.extend(self.link_state());
                    Value::Object(reply)
                }
                _ => json!({"cmd": word, "ok": false, "error": "unknown command"}),
            };
            self.state().lines.push(command.clone());
            out.extend(format!("@td {reply}\r\ntd> ").into_bytes());
        }
        out
    }

    fn link_state(&self) -> Map<String, Value> {
        let state = self.state();
        let online =
            state.online_since_hello && state.last_rx.is_some_and(|at| at.elapsed() < ONLINE_FOR);
        let last_rx_ms = state
            .last_rx
            .map_or(0, |at| at.duration_since(state.booted).as_millis() as u64);
        let fields = json!({
            "online": online, "frames_rx": state.frames_rx, "frames_tx": state.frames_tx,
            "crc_errors": 0, "dropped": state.dropped, "last_rx_ms": last_rx_ms,
        });
        fields.as_object().cloned().unwrap_or_default()
    }

    fn answer_frame(&self, frame: Frame) -> Vec<u8> {
        {
            let mut state = self.state();
            state.frames_rx += 1;
            state.last_rx = Some(Instant::now());
        }
        let (Role::Request(seq), Body::Message(payload)) = (frame.role, frame.body) else {
            self.state().dropped += 1;
            return Vec::new();
        };
        let request = match serde_json::from_slice::<Value>(&payload) {
            Ok(Value::Object(request)) if request.get("m").is_some_and(Value::is_string) => request,
            _ => {
                self.state().dropped += 1;
                return Vec::new();
            }
        };
        self.state().received.push(Value::Object(request.clone()));
        let m = request["m"].as_str().unwrap_or_default().to_owned();
        let fields = self.dispatch(&m, &request);
        let mut reply = Map::from_iter([
            ("m".to_owned(), json!(m)),
            ("ok".to_owned(), json!(fields.is_ok())),
        ]);
        match fields {
            Ok(fields) => reply.extend(fields),
            Err(error) => {
                reply.insert("error".into(), json!(error));
            }
        }
        let payload = serde_json::to_vec(&reply).expect("replies serialize");
        let wire = Frame::message(Role::Response(seq), payload)
            .encode()
            .expect("mock replies fit one frame");
        self.state().frames_tx += 1;
        wire
    }

    fn dispatch(
        &self,
        m: &str,
        request: &Map<String, Value>,
    ) -> Result<Map<String, Value>, &'static str> {
        let object = |value: Value| value.as_object().cloned().unwrap_or_default();
        match m {
            "hello" => {
                let mut state = self.state();
                state.online_since_hello = true;
                Ok(object(
                    serde_json::to_value(&state.hello).expect("hello serializes"),
                ))
            }
            "ping" => {
                let t = self.state().booted.elapsed().as_millis() as u64;
                Ok(object(
                    json!({"t": t, "n": request.get("n").cloned().unwrap_or(Value::Null)}),
                ))
            }
            "link.state" => Ok(self.link_state()),
            "icon.list" => {
                let state = self.state();
                let cursor = request.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ids: Vec<&String> = state.icons.iter().skip(cursor).take(ICON_PAGE).collect();
                let next = (cursor + ids.len() < state.icons.len()).then_some(cursor + ids.len());
                Ok(object(json!({"ids": ids, "next": next})))
            }
            "settings.get" => Ok(object(
                serde_json::to_value(self.state().settings).expect("settings serialize"),
            )),
            "settings.put" => {
                let mut state = self.state();
                let settings = Settings::from_value(&Value::Object(request.clone()));
                let reboot_required = settings.orientation != state.settings.orientation;
                state.settings = settings;
                let mut fields =
                    object(serde_json::to_value(settings).expect("settings serialize"));
                fields.insert("reboot_required".into(), json!(reboot_required));
                Ok(fields)
            }
            "widget.set" => {
                let tile = request
                    .get("tile")
                    .and_then(Value::as_str)
                    .ok_or("widget.set needs a tile")?;
                self.state()
                    .widgets
                    .insert(tile.to_owned(), Value::Object(request.clone()));
                Ok(Map::new())
            }
            _ => Err("unknown message"),
        }
    }
}

pub struct MockTransport {
    from_deck: Receiver<Vec<u8>>,
    to_deck: Sender<Vec<u8>>,
}

impl Transport for MockTransport {
    fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        Ok((
            Box::new(MockReader {
                from_deck: self.from_deck,
                held: Vec::new(),
            }),
            Box::new(MockWriter {
                to_deck: self.to_deck,
            }),
        ))
    }
}

struct MockReader {
    from_deck: Receiver<Vec<u8>>,
    held: Vec<u8>,
}

impl Read for MockReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.held.is_empty() {
            match self.from_deck.recv_timeout(READ_TICK) {
                Ok(bytes) => self.held = bytes,
                Err(RecvTimeoutError::Timeout) => return Err(ErrorKind::TimedOut.into()),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = buf.len().min(self.held.len());
        buf[..n].copy_from_slice(&self.held[..n]);
        self.held.drain(..n);
        Ok(n)
    }
}

struct MockWriter {
    to_deck: Sender<Vec<u8>>,
}

impl Write for MockWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.to_deck
            .send(buf.to_vec())
            .map_err(|_| io::Error::from(ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Session, SessionConfig};
    use td_core::Orientation;

    #[test]
    fn settings_put_stores_firmware_values_and_flags_an_orientation_change() {
        let deck = MockDeck::new();
        let (session, _events) = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap();
        let put = td_core::Request::SettingsPut(Settings {
            idle_timeout_s: 120,
            orientation: Orientation::Portrait,
        });
        let reply = session.request(&put, Duration::from_secs(1)).unwrap();
        assert_eq!(reply.fields.get("reboot_required"), Some(&json!(true)));
        assert_eq!(
            deck.settings(),
            Settings {
                idle_timeout_s: 120,
                orientation: Orientation::Portrait
            }
        );
        assert_eq!(deck.received().last().unwrap()["m"], "settings.put");
    }

    #[test]
    fn link_state_counts_frames_and_hello_brings_the_link_online() {
        let deck = MockDeck::new();
        let (session, _events) = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap();
        let state: td_core::LinkState = session
            .request(&td_core::Request::LinkState, Duration::from_secs(1))
            .unwrap()
            .fields()
            .unwrap();
        assert_eq!(
            (state.frames_rx, state.frames_tx, state.online),
            (1, 0, false)
        );
        session.hello(Duration::from_secs(1)).unwrap();
        let state: td_core::LinkState = session
            .request(&td_core::Request::LinkState, Duration::from_secs(1))
            .unwrap()
            .fields()
            .unwrap();
        assert_eq!(
            (state.frames_rx, state.frames_tx, state.online),
            (3, 2, true)
        );
    }

    #[test]
    fn key_reports_count_seq_and_a_drop_leaves_a_gap() {
        let deck = MockDeck::new();
        let (_session, events) = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap();
        deck.emit_key(&json!({"type": "kbd", "mods": 0, "keys": [104]}));
        deck.drop_key_reports(1);
        deck.emit_key(&json!({"type": "con", "usage": 233}));
        deck.emit_resync(0, &[]);
        deck.emit_press("apps", "launch", "launch");
        let got: Vec<Value> = (0..4)
            .map(
                |_| match events.recv_timeout(Duration::from_secs(1)).unwrap() {
                    crate::Incoming::Event(event) => event,
                    other => panic!("not an event: {other:?}"),
                },
            )
            .collect();
        assert_eq!(
            (
                got[0]["m"].as_str(),
                got[0]["seq"].as_u64(),
                &got[0]["keys"]
            ),
            (Some("key.report"), Some(1), &json!([104]))
        );
        assert_eq!(
            (got[1]["seq"].as_u64(), &got[1]["usage"]),
            (Some(3), &json!(233))
        );
        assert_eq!(
            (got[2]["m"].as_str(), got[2]["seq"].as_u64()),
            (Some("key.resync"), Some(4))
        );
        assert_eq!(
            (
                got[3]["m"].as_str(),
                got[3]["seq"].as_u64(),
                got[3]["tile"].as_str()
            ),
            (Some("press"), Some(1), Some("launch"))
        );
    }

    #[test]
    fn an_unknown_message_is_refused() {
        let deck = MockDeck::new();
        assert_eq!(
            deck.dispatch("layout.nope", &Map::new()),
            Err("unknown message")
        );
    }
}
