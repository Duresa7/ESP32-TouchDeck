//! A Session over a carrier that moves frames as values: no SLIP, no bytes.
use std::io::{self, ErrorKind};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use td_core::{Body, Frame, Item, Request, Role};
use td_link::{
    Carrier, FrameReader, FrameWriter, Incoming, IncomingReceiver, LinkError, Outbound, READ_TICK,
    Received, SendWindow, Session, SessionConfig, XferFrame,
};

/// The test carrier: frames cross two channels as values. It has no console.
struct Frames {
    from_device: Receiver<Frame>,
    to_device: Sender<Frame>,
}

impl Carrier for Frames {
    fn split(self: Box<Self>) -> io::Result<(Box<dyn FrameReader>, Box<dyn FrameWriter>)> {
        Ok((
            Box::new(FramesIn(self.from_device)),
            Box::new(FramesOut(self.to_device)),
        ))
    }
}

struct FramesIn(Receiver<Frame>);

impl FrameReader for FramesIn {
    fn recv(&mut self) -> io::Result<Received> {
        match self.0.recv_timeout(READ_TICK) {
            Ok(frame) => Ok(Received::Items(vec![Item::Frame(frame)])),
            Err(RecvTimeoutError::Timeout) => Ok(Received::Items(Vec::new())),
            Err(RecvTimeoutError::Disconnected) => Ok(Received::Ended),
        }
    }
}

struct FramesOut(Sender<Frame>);

impl FrameWriter for FramesOut {
    fn send(&mut self, out: Outbound<'_>, window: &SendWindow<'_>) -> io::Result<()> {
        if !window.is_open() {
            return Err(ErrorKind::BrokenPipe.into());
        }
        match out {
            Outbound::Frame(frame) => self
                .0
                .send(frame.clone())
                .map_err(|_| ErrorKind::BrokenPipe.into()),
            Outbound::Line(_) => Err(io::Error::new(
                ErrorKind::Unsupported,
                "this carrier has no console",
            )),
        }
    }
}

/// The Device end of the test carrier.
struct Device {
    to_session: Sender<Frame>,
    from_session: Receiver<Frame>,
}

impl Device {
    fn next(&self) -> Frame {
        self.from_session
            .recv_timeout(Duration::from_secs(5))
            .expect("the Session sent a frame")
    }

    fn send(&self, frame: Frame) {
        self.to_session.send(frame).unwrap();
    }

    /// Answers message requests until the Session goes away, and returns what it asked.
    fn serve(self, answer: impl Fn(&Value) -> Option<Value> + Send + 'static) -> Sender<()> {
        let (stop, stopped) = mpsc::channel::<()>();
        thread::spawn(move || {
            while let Err(mpsc::TryRecvError::Empty) = stopped.try_recv() {
                let Ok(Frame {
                    role: Role::Request(seq),
                    body: Body::Message(payload),
                }) = self.from_session.recv_timeout(Duration::from_millis(10))
                else {
                    continue;
                };
                let request: Value = serde_json::from_slice(&payload).unwrap();
                if let Some(mut reply) = answer(&request) {
                    reply["m"] = request["m"].clone();
                    reply["ok"] = json!(true);
                    self.send(Frame::message(
                        Role::Response(seq),
                        serde_json::to_vec(&reply).unwrap(),
                    ));
                }
            }
        });
        stop
    }
}

fn open(config: SessionConfig) -> (Session, IncomingReceiver, Device) {
    let (to_session, from_device) = mpsc::channel();
    let (to_device, from_session) = mpsc::channel();
    let (session, incoming) = Session::over(
        Box::new(Frames {
            from_device,
            to_device,
        }),
        config,
    )
    .unwrap();
    (
        session,
        incoming,
        Device {
            to_session,
            from_session,
        },
    )
}

fn quiet() -> SessionConfig {
    SessionConfig {
        ping_every: None,
        ..SessionConfig::default()
    }
}

fn hello() -> Value {
    json!({
        "product": "touchdeck", "proto": td_core::PROTOCOL, "elf_sha256": "00",
        "version": "1.0.0", "running": "ota_0", "sinks": [], "recovery": false,
        "layout": "default",
    })
}

fn closed(incoming: &IncomingReceiver) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(Incoming::Closed(reason)) = incoming.recv_timeout(left) {
            return reason;
        }
    }
    panic!("the Session did not close within 5 s");
}

#[test]
fn hello_and_ping_cross_a_frame_carrier() {
    let (session, _incoming, device) = open(quiet());
    let _stop = device.serve(|request| match request["m"].as_str() {
        Some("hello") => Some(hello()),
        Some("ping") => Some(json!({"t": 7, "n": request["n"]})),
        _ => None,
    });
    let said = session.hello(Duration::from_secs(2)).unwrap();
    assert!(said.is_deck());
    let pong = session
        .request(&Request::Ping { n: Some(3) }, Duration::from_secs(2))
        .unwrap();
    assert_eq!(pong.fields["n"], 3);
}

#[test]
fn device_events_reach_the_incoming_channel() {
    let (_session, incoming, device) = open(quiet());
    let event =
        json!({"m": "press", "page": "home", "tile": "a", "kind": "launch", "seq": 1, "t": 5});
    device.send(Frame::message(
        Role::Event,
        serde_json::to_vec(&event).unwrap(),
    ));
    assert_eq!(
        incoming.recv_timeout(Duration::from_secs(5)),
        Ok(Incoming::Event(event))
    );
}

#[test]
fn a_transfer_sends_chunks_and_receives_acks_as_frames() {
    let (session, _incoming, device) = open(quiet());
    let transfer = session.transfer().unwrap();
    transfer.send_chunk(4, 0, b"tile").unwrap();
    let chunk = device.next();
    let Role::Request(seq) = chunk.role else {
        panic!("a chunk is a request: {chunk:?}");
    };
    assert_eq!(
        chunk.body,
        Body::Chunk {
            xfer: 4,
            idx: 0,
            data: b"tile".to_vec()
        }
    );
    device.send(Frame {
        role: Role::Response(seq),
        body: Body::Ack { xfer: 4, next: 1 },
    });
    assert_eq!(
        transfer.frames.recv_timeout(Duration::from_secs(5)),
        Ok(XferFrame::Ack { xfer: 4, next: 1 })
    );
}

#[test]
fn a_silent_device_misses_pings_and_the_session_closes() {
    let (session, incoming, _device) = open(SessionConfig {
        ping_every: Some(Duration::from_millis(20)),
        ping_timeout: Duration::from_millis(100),
        ..SessionConfig::default()
    });
    assert_eq!(closed(&incoming), "the deck stopped answering ping");
    assert!(!session.is_open());
}

#[test]
fn a_carrier_that_ends_closes_the_session() {
    let (session, incoming, device) = open(quiet());
    drop(device);
    assert_eq!(closed(&incoming), "the deck closed the stream");
    assert!(matches!(
        session.hello(Duration::from_millis(100)),
        Err(LinkError::Closed(_))
    ));
}

#[test]
fn a_carrier_without_a_console_refuses_console_lines() {
    let (session, _incoming, _device) = open(quiet());
    assert_eq!(
        session.send_line("version"),
        Err(LinkError::Io("this carrier has no console".into()))
    );
    assert!(session.is_open());
}
