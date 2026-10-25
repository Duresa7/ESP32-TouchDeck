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
const DASHBOARD_ROWS: usize = 8;
const ONLINE_FOR: Duration = Duration::from_secs(5);
const HEARTBEAT: Duration = Duration::from_secs(2);

#[derive(Debug)]
struct State {
    hello: Hello,
    settings: Settings,
    icons: BTreeSet<String>,
    widgets: BTreeMap<String, Value>,
    dashboards: BTreeMap<String, Value>,
    sets: Vec<Value>,
    received: Vec<Value>,
    lines: Vec<String>,
    silent: bool,
    reply_delay: Duration,
    frames_rx: u64,
    frames_tx: u64,
    dropped: u64,
    last_rx: Option<Instant>,
    online_since_hello: bool,
    presses: u32,
    booted: Instant,
    link: Option<Sender<Vec<u8>>>,
    generation: u64,
}

impl State {
    fn record_set(&mut self, request: &Map<String, Value>) {
        let mut set = request.clone();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64);
        set.insert("at_ms".into(), json!(at));
        self.sets.push(Value::Object(set));
    }
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
                dashboards: BTreeMap::new(),
                sets: Vec::new(),
                received: Vec::new(),
                lines: Vec::new(),
                silent: false,
                reply_delay: Duration::ZERO,
                frames_rx: 0,
                frames_tx: 0,
                dropped: 0,
                last_rx: None,
                online_since_hello: false,
                presses: 0,
                booted: Instant::now(),
                link: None,
                generation: 0,
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

    /// The latest `widget.set` per tile.
    pub fn widgets(&self) -> BTreeMap<String, Value> {
        self.state().widgets.clone()
    }

    /// The latest `dashboard.set` per page.
    pub fn dashboards(&self) -> BTreeMap<String, Value> {
        self.state().dashboards.clone()
    }

    /// Every `widget.set` and `dashboard.set` in arrival order, each with `at_ms`, the unix
    /// milliseconds at which the mock received it.
    pub fn sets(&self) -> Vec<Value> {
        self.state().sets.clone()
    }

    /// Emits the `press` event a tap on a tile makes.
    pub fn press(&self, page: &str, tile: &str, kind: &str) {
        let (seq, t) = {
            let mut state = self.state();
            state.presses += 1;
            (state.presses, state.booted.elapsed().as_millis() as u64)
        };
        self.emit(&Event::Press {
            page: page.to_owned(),
            tile: tile.to_owned(),
            kind: kind.to_owned(),
            seq,
            t,
        });
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
                let mut state = self.state();
                state
                    .widgets
                    .insert(tile.to_owned(), Value::Object(request.clone()));
                state.record_set(request);
                Ok(Map::new())
            }
            "dashboard.set" => {
                let page = request
                    .get("page")
                    .and_then(Value::as_str)
                    .ok_or("dashboard.set needs a page")?;
                let rows = request
                    .get("rows")
                    .and_then(Value::as_array)
                    .ok_or("dashboard.set needs rows")?;
                if rows.len() > DASHBOARD_ROWS {
                    return Err("a dashboard holds 8 rows");
                }
                let mut state = self.state();
                state
                    .dashboards
                    .insert(page.to_owned(), Value::Object(request.clone()));
                state.record_set(request);
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
    fn widget_and_dashboard_sets_are_recorded_in_order_with_arrival_times() {
        let deck = MockDeck::new();
        let (session, _events) = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap();
        let widget = td_core::Request::WidgetSet {
            tile: "context".into(),
            value: "42%".into(),
            caption: "Context".into(),
            bar: 0.42,
            stale: false,
        };
        let dashboard = td_core::Request::DashboardSet {
            page: "accounts".into(),
            rows: vec![td_core::DashboardRow {
                label: "a@b.c".into(),
                value: "active".into(),
                bar: -1.0,
            }],
        };
        assert!(session.request(&widget, Duration::from_secs(1)).unwrap().ok);
        assert!(
            session
                .request(&dashboard, Duration::from_secs(1))
                .unwrap()
                .ok
        );
        let sets = deck.sets();
        assert_eq!(
            sets.iter().map(|set| set["m"].clone()).collect::<Vec<_>>(),
            [json!("widget.set"), json!("dashboard.set")]
        );
        assert_eq!(sets[0]["tile"], "context");
        assert_eq!(sets[0]["value"], "42%");
        assert!(sets[0]["at_ms"].as_u64().unwrap() <= sets[1]["at_ms"].as_u64().unwrap());
        assert_eq!(deck.widgets()["context"]["stale"], false);
        assert_eq!(deck.dashboards()["accounts"]["rows"][0]["label"], "a@b.c");
    }

    #[test]
    fn a_ninth_dashboard_row_is_refused() {
        let deck = MockDeck::new();
        let rows: Vec<Value> = (0..9)
            .map(|n| json!({"label": n.to_string(), "value": "x", "bar": -1}))
            .collect();
        let request = json!({"m": "dashboard.set", "page": "p", "rows": rows});
        assert_eq!(
            deck.dispatch("dashboard.set", request.as_object().unwrap()),
            Err("a dashboard holds 8 rows")
        );
        assert!(deck.sets().is_empty());
    }

    #[test]
    fn a_press_reaches_the_host_as_an_event() {
        let deck = MockDeck::new();
        let (_session, events) = Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap();
        deck.press("usage", "proxy", "widget");
        let event = until_event(&events);
        assert_eq!(event["m"], "press");
        assert_eq!(
            (event["page"].as_str(), event["tile"].as_str()),
            (Some("usage"), Some("proxy"))
        );
        assert_eq!(event["kind"], "widget");
    }

    fn until_event(events: &Receiver<crate::Incoming>) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(crate::Incoming::Event(event)) = events.recv_timeout(left) {
                return event;
            }
        }
        panic!("no event within 5 s");
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
