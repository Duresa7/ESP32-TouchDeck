//! One Device's actor. Its work thread owns the link session and runs requests one at a
//! time; its forwarder thread turns what the Device sends into events and mirrored keys.
//! Layout and Asset work borrows the same session (see `crate::link`), since the port can
//! only be open once.
use super::{
    DeviceConsole, DeviceEvent, DeviceId, DeviceKeyMirror, Inner, InputFactory, KeyMirrorState,
    input::{InputHub, PressGate},
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use td_host::{Mirror, Mirrored, Outcome};
use td_link::{Incoming, Session};

pub type Work = Box<dyn FnOnce(&Session) + Send>;

/// The shortest time between two batches of a Device's drags. The Firmware sends at most one move every 50 ms, so
/// this only coalesces a Device that sends faster.
const INPUT_PACE: Duration = Duration::from_millis(20);

/// Dropping the handle ends the work thread, which closes the session and frees the port.
pub(super) struct Handle {
    pub generation: u64,
    work: Sender<Work>,
    session: Arc<Session>,
}
impl Handle {
    pub(super) fn sender(&self) -> Sender<Work> {
        self.work.clone()
    }
    /// The live session. It stops answering once the work thread closes it.
    pub(super) fn session(&self) -> Arc<Session> {
        self.session.clone()
    }
}

pub(super) fn spawn(
    inner: Arc<Inner>,
    id: DeviceId,
    generation: u64,
    session: Session,
    incoming: Receiver<Incoming>,
    mirror: Option<Mirror>,
) -> Handle {
    let (work, jobs) = mpsc::channel::<Work>();
    let session = Arc::new(session);
    let shared = session.clone();
    let (presses, taps) = mpsc::sync_channel::<serde_json::Value>(16);
    let action_inner = inner.clone();
    let action_id = id.clone();
    thread::spawn(move || {
        let mut parts = PressGate::default();
        for event in taps {
            if event.get("part").is_some() && !parts.accept(&event, Instant::now()) {
                continue;
            }
            if let Some(handler) = action_inner.actions.get() {
                handler(&action_id, &event);
            }
        }
    });
    let (drags, inputs) = mpsc::sync_channel::<serde_json::Value>(super::input::PENDING_MAX);
    let input_inner = inner.clone();
    let input_id = id.clone();
    thread::spawn(move || run_inputs(&input_inner, &input_id, &inputs));
    thread::Builder::new()
        .name(format!("td-device-{id}"))
        .spawn(move || {
            for job in jobs {
                job(&session);
            }
            session.close();
        })
        .expect("the Device thread starts");
    thread::Builder::new()
        .name(format!("td-device-{id}-in"))
        .spawn(move || {
            forward(
                &inner,
                &id,
                generation,
                incoming,
                mirror,
                Touches { presses, drags },
            )
        })
        .expect("the Device forwarder starts");
    Handle {
        generation,
        work,
        session: shared,
    }
}

/// A key mirror for a Device, unless it types through its own USB HID or the PC has no sink.
pub(super) fn mirror(
    input: &InputFactory,
    hello: &td_core::Hello,
) -> (Option<Mirror>, KeyMirrorState) {
    match input() {
        Ok(sink) => {
            let mut mirror = Mirror::new(sink);
            let wanted = Mirror::wanted(&hello.sinks);
            mirror.set_enabled(wanted);
            let reason = if wanted {
                "Typing the Device's keys on this PC."
            } else {
                "Off: the Device types through its own USB HID."
            };
            let state = KeyMirrorState {
                enabled: wanted,
                reason: Some(reason.into()),
            };
            (Some(mirror), state)
        }
        Err(error) => {
            let state = KeyMirrorState {
                enabled: false,
                reason: Some(format!("Off: {error}")),
            };
            (None, state)
        }
    }
}

/// Runs one session's drags: each batch takes what arrived since the last, newest per Tile stream, and sets it.
fn run_inputs(inner: &Inner, id: &DeviceId, inputs: &Receiver<serde_json::Value>) {
    let mut hub = InputHub::default();
    while let Ok(first) = inputs.recv() {
        hub.push(&first);
        while let Ok(next) = inputs.try_recv() {
            hub.push(&next);
        }
        let batch = hub.take();
        if let Some(handler) = inner.inputs.get() {
            for input in batch {
                let Some(result) = handler.set(id, &input) else {
                    continue;
                };
                if let Some(outcome) = hub.settled(&input, result, Instant::now()) {
                    handler.gesture(id, &input, outcome);
                }
            }
        }
        thread::sleep(INPUT_PACE);
    }
}

/// Where the forwarder hands a Device's touches. Both are bounded, and a full one drops the event.
struct Touches {
    presses: mpsc::SyncSender<serde_json::Value>,
    drags: mpsc::SyncSender<serde_json::Value>,
}

fn forward(
    inner: &Inner,
    id: &DeviceId,
    generation: u64,
    incoming: Receiver<Incoming>,
    mut mirror: Option<Mirror>,
    touches: Touches,
) {
    for item in incoming {
        match item {
            Incoming::Line(line) => inner.events.publish(&DeviceConsole {
                device: id.clone(),
                line,
            }),
            Incoming::Event(event) => {
                if event["m"] == "press" {
                    let _ = touches.presses.try_send(event.clone());
                } else if event["m"] == "v2.input" {
                    let _ = touches.drags.try_send(event.clone());
                }
                if let Some(report) = Mirrored::parse(&event) {
                    key_report(inner, id, mirror.as_mut(), &report);
                }
                inner.events.publish(&DeviceEvent {
                    device: id.clone(),
                    event,
                });
            }
            Incoming::Closed(reason) => {
                if let Some(mirror) = mirror.as_mut() {
                    mirror.release_all();
                }
                inner.closed(id, generation, &reason);
                return;
            }
        }
    }
}

/// Types one key report on the PC. While a dialog suppresses input, the report is dropped,
/// not replayed: the gap it leaves releases every held key when typing resumes.
fn key_report(inner: &Inner, id: &DeviceId, mirror: Option<&mut Mirror>, report: &Mirrored) {
    let _gate = inner.input_gate.lock();
    let arrived_ms = crate::now_ms();
    let (outcome, gap, errors) = match mirror {
        _ if inner.input_suppressed() => ("suppressed", None, Vec::new()),
        None => ("off", None, Vec::new()),
        Some(mirror) => {
            let outcome = mirror.apply(report);
            let errors = std::mem::take(&mut mirror.errors);
            match outcome {
                Outcome::Typed { gap } => ("typed", gap.map(|(a, b)| [a, b]), errors),
                Outcome::Duplicate => ("duplicate", None, errors),
                Outcome::Off => ("off", None, errors),
                Outcome::Unknown => ("unknown", None, errors),
            }
        }
    };
    inner.events.publish(&DeviceKeyMirror {
        device: id.clone(),
        seq: report.seq(),
        report: report.describe(),
        outcome: outcome.into(),
        gap,
        errors,
        arrived_ms,
    });
}
