use std::collections::HashMap;
use std::fmt;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;
use td_core::{Body, Frame, Hello, Item, Reply, ReplyError, Request, Role, StreamDecoder};

use crate::Transport;
use crate::xfer::LayoutState;

const WRITE_DEADLINE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    pub ping_every: Option<Duration>,
    pub ping_timeout: Duration,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            ping_every: Some(Duration::from_secs(2)),
            ping_timeout: Duration::from_secs(2),
        }
    }
}

/// A chunk or an ack of a transfer, with the frame `seq` an ack must echo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XferFrame {
    Chunk {
        seq: u8,
        xfer: u8,
        idx: u16,
        data: Vec<u8>,
    },
    Ack {
        xfer: u8,
        next: u16,
    },
}

/// What the deck sends that is not a reply: events, console text lines, and the end.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event(Value),
    Line(String),
    Closed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Timeout(Duration),
    Closed(String),
    Busy,
    TransferBusy,
    ConsoleBusy,
    Io(String),
    Reply(ReplyError),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Timeout(after) => write!(f, "no reply within {} ms", after.as_millis()),
            LinkError::Closed(reason) => write!(f, "link closed: {reason}"),
            LinkError::Busy => f.write_str("255 requests are already waiting"),
            LinkError::TransferBusy => f.write_str("a transfer is already running"),
            LinkError::ConsoleBusy => f.write_str("a layout state reply is still pending"),
            LinkError::Io(error) => write!(f, "write failed: {error}"),
            LinkError::Reply(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LinkError {}

struct Pending {
    last: u8,
    waiting: HashMap<u8, SyncSender<Reply>>,
}

struct Shared {
    writer: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<Pending>,
    open: AtomicBool,
    reason: Mutex<String>,
    incoming: Sender<Incoming>,
    xfer: Mutex<Option<Sender<XferFrame>>>,
    layout_reply: Mutex<Option<SyncSender<Value>>>,
}

impl Shared {
    fn shutdown(&self, reason: &str) {
        if self.open.swap(false, Ordering::AcqRel) {
            *self.reason.lock().unwrap() = reason.to_owned();
            self.pending.lock().unwrap().waiting.clear();
            self.layout_reply.lock().unwrap().take();
            let _ = self.incoming.send(Incoming::Closed(reason.to_owned()));
        }
    }

    fn closed(&self) -> LinkError {
        LinkError::Closed(self.reason.lock().unwrap().clone())
    }

    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self.writer.lock().unwrap();
        let deadline = Instant::now() + WRITE_DEADLINE;
        let mut rest = bytes;
        while !rest.is_empty() {
            match writer.write(rest) {
                Ok(0) => return Err(ErrorKind::WriteZero.into()),
                Ok(n) => rest = &rest[n..],
                Err(error)
                    if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::Interrupted)
                        && Instant::now() < deadline => {}
                Err(error) => return Err(error),
            }
        }
        writer.flush()
    }

    fn next_seq(pending: &mut Pending) -> Result<u8, LinkError> {
        let seq = (1..=255u16)
            .map(|step| ((u16::from(pending.last) + step - 1) % 255 + 1) as u8)
            .find(|seq| !pending.waiting.contains_key(seq))
            .ok_or(LinkError::Busy)?;
        pending.last = seq;
        Ok(seq)
    }

    fn request(&self, request: &Request, timeout: Duration) -> Result<Reply, LinkError> {
        self.call(request.name(), request.to_payload(), timeout)
    }

    fn call(&self, name: &str, payload: Vec<u8>, timeout: Duration) -> Result<Reply, LinkError> {
        let (slot, reply) = mpsc::sync_channel(1);
        let seq = {
            let mut pending = self.pending.lock().unwrap();
            if !self.open.load(Ordering::Acquire) {
                return Err(self.closed());
            }
            let seq = Shared::next_seq(&mut pending)?;
            pending.waiting.insert(seq, slot);
            seq
        };
        let forget = || self.pending.lock().unwrap().waiting.remove(&seq);
        let wire = Frame::message(Role::Request(seq), payload)
            .encode()
            .map_err(|error| LinkError::Io(format!("request does not fit one frame: {error}")))?;
        if let Err(error) = self.write(&wire) {
            forget();
            return Err(LinkError::Io(error.to_string()));
        }
        match reply.recv_timeout(timeout) {
            Ok(reply) if reply.m == name => Ok(reply),
            Ok(reply) => Err(LinkError::Reply(ReplyError::Fields(format!(
                "reply names {:?}, not {name:?}",
                reply.m
            )))),
            Err(RecvTimeoutError::Timeout) => {
                forget();
                Err(LinkError::Timeout(timeout))
            }
            Err(RecvTimeoutError::Disconnected) => Err(self.closed()),
        }
    }

    fn route(&self, item: Item, line: &mut Vec<u8>) {
        match item {
            Item::Text(bytes) => {
                for byte in bytes {
                    if byte == b'\n' {
                        let text =
                            String::from_utf8_lossy(line.strip_suffix(b"\r").unwrap_or(line))
                                .into_owned();
                        line.clear();
                        if let Some(at) = text.find("@td ")
                            && let Ok(reply) = serde_json::from_str::<Value>(&text[at + 4..])
                            && reply["cmd"] == "layout"
                            && (reply["op"] == "state" || reply["ok"] == false)
                            && let Some(waiting) = self.layout_reply.lock().unwrap().take()
                        {
                            let _ = waiting.send(reply);
                        }
                        let _ = self.incoming.send(Incoming::Line(text));
                    } else {
                        line.push(byte);
                    }
                }
            }
            Item::Frame(Frame {
                role: Role::Response(seq),
                body: Body::Message(payload),
            }) => {
                if let Ok(reply) = Reply::parse(&payload)
                    && let Some(slot) = self.pending.lock().unwrap().waiting.remove(&seq)
                {
                    let _ = slot.send(reply);
                }
            }
            Item::Frame(Frame {
                role: Role::Event,
                body: Body::Message(payload),
            }) => {
                if let Ok(event @ Value::Object(_)) = serde_json::from_slice::<Value>(&payload) {
                    let _ = self.incoming.send(Incoming::Event(event));
                }
            }
            Item::Frame(Frame {
                role,
                body: Body::Chunk { xfer, idx, data },
            }) => self.to_xfer(XferFrame::Chunk {
                seq: role.seq(),
                xfer,
                idx,
                data,
            }),
            Item::Frame(Frame {
                body: Body::Ack { xfer, next },
                ..
            }) => self.to_xfer(XferFrame::Ack { xfer, next }),
            Item::Frame(_) => {}
        }
    }

    fn to_xfer(&self, frame: XferFrame) {
        if let Some(sink) = self.xfer.lock().unwrap().as_ref() {
            let _ = sink.send(frame);
        }
    }
}

/// The one transfer a session runs at a time. Chunks and acks reach [`Transfer::frames`]
/// until it drops.
pub struct Transfer<'a> {
    session: &'a Session,
    pub frames: Receiver<XferFrame>,
}

impl Transfer<'_> {
    /// Writes one chunk as a request frame with a fresh `seq`, which the receiver's ack echoes.
    pub fn send_chunk(&self, xfer: u8, idx: u16, data: &[u8]) -> Result<(), LinkError> {
        let shared = &self.session.shared;
        let seq = Shared::next_seq(&mut shared.pending.lock().unwrap())?;
        self.write(Frame {
            role: Role::Request(seq),
            body: Body::Chunk {
                xfer,
                idx,
                data: data.to_vec(),
            },
        })
    }

    /// Acks a received chunk with the next index wanted, echoing the chunk's `seq`.
    pub fn send_ack(&self, seq: u8, xfer: u8, next: u16) -> Result<(), LinkError> {
        self.write(Frame {
            role: Role::Response(seq),
            body: Body::Ack { xfer, next },
        })
    }

    fn write(&self, frame: Frame) -> Result<(), LinkError> {
        let shared = &self.session.shared;
        if !shared.open.load(Ordering::Acquire) {
            return Err(shared.closed());
        }
        let wire = frame
            .encode()
            .map_err(|error| LinkError::Io(format!("frame does not encode: {error}")))?;
        shared
            .write(&wire)
            .map_err(|error| LinkError::Io(error.to_string()))
    }
}

impl Drop for Transfer<'_> {
    fn drop(&mut self) {
        self.session.shared.xfer.lock().unwrap().take();
    }
}

/// One link session over one transport. Replies match requests by `seq`, everything
/// else arrives on the [`Incoming`] channel, and a `ping` every 2 seconds proves the deck
/// is still there. Closing or dropping the session joins its threads, which releases
/// the port.
pub struct Session {
    shared: Arc<Shared>,
    console: Mutex<()>,
    stop: Mutex<Option<Sender<()>>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Session {
    pub fn open(
        transport: Box<dyn Transport>,
        config: SessionConfig,
    ) -> io::Result<(Session, Receiver<Incoming>)> {
        let (reader, writer) = transport.split()?;
        let (incoming, events) = mpsc::channel();
        let shared = Arc::new(Shared {
            writer: Mutex::new(writer),
            pending: Mutex::new(Pending {
                last: 0,
                waiting: HashMap::new(),
            }),
            open: AtomicBool::new(true),
            reason: Mutex::new(String::new()),
            incoming,
            xfer: Mutex::new(None),
            layout_reply: Mutex::new(None),
        });
        let (stop, stopped) = mpsc::channel::<()>();
        let mut threads = vec![thread::spawn({
            let shared = shared.clone();
            move || read_loop(&shared, reader)
        })];
        if let Some(every) = config.ping_every {
            let shared = shared.clone();
            threads.push(thread::spawn(move || {
                ping_loop(&shared, &stopped, every, config.ping_timeout)
            }));
        }
        Ok((
            Session {
                shared,
                console: Mutex::new(()),
                stop: Mutex::new(Some(stop)),
                threads: Mutex::new(threads),
            },
            events,
        ))
    }

    pub fn request(&self, request: &Request, timeout: Duration) -> Result<Reply, LinkError> {
        self.shared.request(request, timeout)
    }

    /// Sends a message request that `td_core::Request` does not name, as JSON with its `m`.
    pub fn call(&self, payload: &Value, timeout: Duration) -> Result<Reply, LinkError> {
        let name = payload
            .get("m")
            .and_then(Value::as_str)
            .ok_or_else(|| LinkError::Io("a message needs a string m".into()))?;
        let bytes = serde_json::to_vec(payload).expect("JSON values serialize");
        self.shared.call(name, bytes, timeout)
    }

    /// Starts the session's one transfer. A second one is refused until the first drops.
    pub fn transfer(&self) -> Result<Transfer<'_>, LinkError> {
        let mut slot = self.shared.xfer.lock().unwrap();
        if slot.is_some() {
            return Err(LinkError::TransferBusy);
        }
        let (sink, frames) = mpsc::channel();
        *slot = Some(sink);
        Ok(Transfer {
            session: self,
            frames,
        })
    }

    pub fn hello(&self, timeout: Duration) -> Result<Hello, LinkError> {
        self.request(&Request::Hello, timeout)?
            .fields()
            .map_err(LinkError::Reply)
    }

    /// Writes one console line. The deck answers it as text on the [`Incoming`] channel.
    pub fn send_line(&self, line: &str) -> Result<(), LinkError> {
        let _console = self.console.lock().unwrap();
        self.write_line(line)
    }

    /// Reads the displayed layout through the firmware's existing `layout state` console
    /// command. Its reply still reaches [`Incoming`], including when an event handler asks.
    pub fn layout_state(&self, timeout: Duration) -> Result<LayoutState, LinkError> {
        let _console = self.console.lock().unwrap();
        let (waiting, reply) = mpsc::sync_channel(1);
        {
            let mut slot = self.shared.layout_reply.lock().unwrap();
            if slot.is_some() {
                return Err(LinkError::ConsoleBusy);
            }
            *slot = Some(waiting);
        }
        if let Err(error) = self.write_line("layout state") {
            self.shared.layout_reply.lock().unwrap().take();
            return Err(error);
        }
        let value = match reply.recv_timeout(timeout) {
            Ok(value) => value,
            // Console replies have no request ID. Drain this reply before allowing another
            // query, so a late reply cannot satisfy a query for a newly displayed layout.
            Err(RecvTimeoutError::Timeout) => return Err(LinkError::Timeout(timeout)),
            Err(RecvTimeoutError::Disconnected) => return Err(self.shared.closed()),
        };
        if value["ok"] != true {
            return Err(LinkError::Reply(ReplyError::Fields(value.to_string())));
        }
        serde_json::from_value(value)
            .map_err(|error| LinkError::Reply(ReplyError::Fields(error.to_string())))
    }

    fn write_line(&self, line: &str) -> Result<(), LinkError> {
        if line.contains(['\r', '\n']) {
            return Err(LinkError::Io("a console line holds no CR or LF".into()));
        }
        if !self.is_open() {
            return Err(self.shared.closed());
        }
        self.shared
            .write(format!("{line}\n").as_bytes())
            .map_err(|error| LinkError::Io(error.to_string()))
    }

    pub fn is_open(&self) -> bool {
        self.shared.open.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        self.shared.shutdown("closed by the app");
        self.stop.lock().unwrap().take();
        for thread in self.threads.lock().unwrap().drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

fn read_loop(shared: &Shared, mut reader: Box<dyn Read + Send>) {
    let mut decoder = StreamDecoder::default();
    let mut line = Vec::new();
    let mut buf = [0u8; 4096];
    while shared.open.load(Ordering::Acquire) {
        match reader.read(&mut buf) {
            Ok(0) => return shared.shutdown("the deck closed the stream"),
            Ok(n) => {
                for item in decoder.feed(&buf[..n], Instant::now()) {
                    shared.route(item, &mut line);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                ) =>
            {
                for item in decoder.poll(Instant::now()) {
                    shared.route(item, &mut line);
                }
            }
            Err(error) => return shared.shutdown(&format!("read failed: {error}")),
        }
    }
}

fn ping_loop(shared: &Shared, stop: &Receiver<()>, every: Duration, timeout: Duration) {
    for n in 1u64.. {
        if !matches!(stop.recv_timeout(every), Err(RecvTimeoutError::Timeout)) {
            return;
        }
        match shared.request(&Request::Ping { n: Some(n) }, timeout) {
            Ok(_) => {}
            Err(LinkError::Timeout(_)) => {
                return shared.shutdown("the deck stopped answering ping");
            }
            Err(error) => return shared.shutdown(&format!("heartbeat failed: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockDeck;
    use td_core::{Event, IconPage, Pong};

    fn quiet() -> SessionConfig {
        SessionConfig {
            ping_every: None,
            ping_timeout: Duration::from_millis(200),
        }
    }

    fn until<T>(events: &Receiver<Incoming>, mut pick: impl FnMut(Incoming) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(5);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            if let Ok(item) = events.recv_timeout(left)
                && let Some(found) = pick(item)
            {
                return found;
            }
        }
        panic!("nothing matched within 5 s");
    }

    #[test]
    fn replies_match_their_requests_by_sequence() {
        let deck = MockDeck::new();
        let (session, _events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        let session = Arc::new(session);
        let workers: Vec<_> = (0..4u64)
            .map(|worker| {
                let session = session.clone();
                thread::spawn(move || {
                    for i in 0..100u64 {
                        let n = worker * 1000 + i;
                        let pong: Pong = session
                            .request(&Request::Ping { n: Some(n) }, Duration::from_secs(2))
                            .unwrap()
                            .fields()
                            .unwrap();
                        assert_eq!(pong.n, Some(n));
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(
            deck.received().iter().filter(|m| m["m"] == "ping").count(),
            400
        );
    }

    #[test]
    fn an_event_during_a_request_reaches_the_channel_and_the_reply_still_matches() {
        let deck = MockDeck::new();
        deck.delay_replies(Duration::from_millis(150));
        let (session, events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        let emitter = {
            let deck = deck.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(40));
                deck.emit(&Event::Press {
                    page: "apps".into(),
                    tile: "launch".into(),
                    kind: "launch".into(),
                    seq: 1,
                    t: 10,
                });
            })
        };
        let hello = session.hello(Duration::from_secs(2)).unwrap();
        emitter.join().unwrap();
        assert!(hello.is_deck());
        let event = until(&events, |item| match item {
            Incoming::Event(event) => Some(event),
            _ => None,
        });
        assert_eq!(
            serde_json::from_value::<Event>(event).unwrap(),
            Event::Press {
                page: "apps".into(),
                tile: "launch".into(),
                kind: "launch".into(),
                seq: 1,
                t: 10
            }
        );
    }

    #[test]
    fn a_silent_deck_times_out_and_a_late_reply_is_dropped() {
        let deck = MockDeck::new();
        let (session, _events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        deck.stop();
        let started = Instant::now();
        assert_eq!(
            session.request(&Request::Hello, Duration::from_millis(300)),
            Err(LinkError::Timeout(Duration::from_millis(300)))
        );
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(session.is_open());
    }

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "heartbeat pipe broke",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct TimeoutReader;

    impl Read for TimeoutReader {
        fn read(&mut self, _bytes: &mut [u8]) -> io::Result<usize> {
            thread::sleep(crate::READ_TICK);
            Err(ErrorKind::TimedOut.into())
        }
    }

    struct BrokenTransport;

    impl Transport for BrokenTransport {
        fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
            Ok((Box::new(TimeoutReader), Box::new(BrokenWriter)))
        }
    }

    #[test]
    fn heartbeat_write_failure_closes_the_session() {
        let config = SessionConfig {
            ping_every: Some(Duration::from_millis(10)),
            ping_timeout: Duration::from_millis(100),
        };
        let (session, events) = Session::open(Box::new(BrokenTransport), config).unwrap();
        let closed = events.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            closed,
            Incoming::Closed("heartbeat failed: write failed: heartbeat pipe broke".into())
        );
        assert!(!session.is_open());
        assert!(matches!(
            session.request(&Request::Hello, Duration::from_millis(50)),
            Err(LinkError::Closed(_))
        ));
        session.close();
        assert!(events.try_recv().is_err(), "only one Closed event");
    }

    #[test]
    fn missed_pings_close_the_session() {
        let deck = MockDeck::new();
        let config = SessionConfig {
            ping_every: Some(Duration::from_millis(100)),
            ping_timeout: Duration::from_millis(100),
        };
        let (session, events) = Session::open(Box::new(deck.connect()), config).unwrap();
        session.hello(Duration::from_secs(1)).unwrap();
        deck.stop();
        let reason = until(&events, |item| match item {
            Incoming::Closed(reason) => Some(reason),
            _ => None,
        });
        assert_eq!(reason, "the deck stopped answering ping");
        assert!(!session.is_open());
        assert!(matches!(
            session.request(&Request::Hello, Duration::from_millis(50)),
            Err(LinkError::Closed(_))
        ));
    }

    #[test]
    fn a_hang_up_closes_the_session_at_once() {
        let deck = MockDeck::new();
        let (session, events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        deck.hang_up();
        let reason = until(&events, |item| match item {
            Incoming::Closed(reason) => Some(reason),
            _ => None,
        });
        assert_eq!(reason, "the deck closed the stream");
        assert!(!session.is_open());
    }

    #[test]
    fn console_lines_and_their_replies_arrive_as_text() {
        let deck = MockDeck::new();
        let (session, events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        session.send_line("version").unwrap();
        let reply = until(&events, |item| match item {
            Incoming::Line(line) if line.contains("@td ") => Some(line),
            _ => None,
        });
        let json: Value = serde_json::from_str(&reply[reply.find("@td ").unwrap() + 4..]).unwrap();
        assert_eq!(
            (json["cmd"].as_str(), json["ok"].as_bool()),
            (Some("version"), Some(true))
        );
        assert_eq!(deck.lines(), ["version"]);
    }

    #[test]
    fn layout_state_can_be_read_from_an_event_handler_without_consuming_console_text() {
        let deck = MockDeck::new();
        let (session, events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        deck.emit_press("shared", "feed_land", "widget");
        until(&events, |item| {
            matches!(item, Incoming::Event(_)).then_some(())
        });
        // Like the app's event handler, do not drain Incoming while waiting for state.
        let state = session.layout_state(Duration::from_secs(1)).unwrap();
        assert_eq!(state, deck.layout_state());
        let line = until(&events, |item| match item {
            Incoming::Line(line) if line.contains("@td ") => Some(line),
            _ => None,
        });
        assert!(line.contains("\"op\":\"state\""));
        assert_eq!(deck.lines(), ["layout state"]);
        assert!(session.hello(Duration::from_secs(1)).unwrap().is_deck());
    }

    #[test]
    fn a_layout_state_timeout_releases_the_console_and_the_next_query_can_succeed() {
        let deck = MockDeck::new();
        deck.delay_replies(Duration::from_millis(80));
        let (session, events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        assert_eq!(
            session.layout_state(Duration::from_millis(10)),
            Err(LinkError::Timeout(Duration::from_millis(10)))
        );
        assert_eq!(
            session.layout_state(Duration::from_secs(1)),
            Err(LinkError::ConsoleBusy)
        );
        until(&events, |item| {
            matches!(item, Incoming::Line(_)).then_some(())
        });
        deck.delay_replies(Duration::ZERO);
        session.send_line("version").unwrap();
        assert_eq!(
            session.layout_state(Duration::from_secs(1)).unwrap(),
            deck.layout_state()
        );
    }

    #[test]
    fn closing_the_connection_wakes_a_layout_state_waiter() {
        let deck = MockDeck::new();
        deck.stop();
        let (session, _events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        let session = Arc::new(session);
        let query = {
            let session = session.clone();
            thread::spawn(move || session.layout_state(Duration::from_secs(5)))
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        while session.shared.layout_reply.lock().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the query did not start");
            thread::sleep(Duration::from_millis(1));
        }
        session.close();
        assert!(matches!(query.join().unwrap(), Err(LinkError::Closed(_))));
    }

    #[test]
    fn icon_lists_page_32_at_a_time() {
        let deck = MockDeck::new();
        deck.add_icons((0..128).map(|i| format!("{i:016x}")));
        let (session, _events) = Session::open(Box::new(deck.connect()), quiet()).unwrap();
        let (mut ids, mut cursor, mut pages) = (Vec::new(), None, 0);
        loop {
            let page: IconPage = session
                .request(&Request::IconList { cursor }, Duration::from_secs(1))
                .unwrap()
                .fields()
                .unwrap();
            assert!(page.ids.len() <= td_core::ICON_PAGE);
            ids.extend(page.ids);
            pages += 1;
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!((ids.len(), pages), (128, 4));
        assert_eq!(
            ids,
            (0..128).map(|i| format!("{i:016x}")).collect::<Vec<_>>()
        );
    }
}
