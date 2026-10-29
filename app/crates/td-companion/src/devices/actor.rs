//! One Device's actor. Its work thread owns the link session and runs requests one at a
//! time; its forwarder thread turns what the Device sends into events and mirrored keys.
//! Layout and Asset work borrows the same session (see `crate::link`), since the port can
//! only be open once.
use super::{
    DeviceConsole, DeviceEvent, DeviceId, DeviceKeyMirror, Inner, InputFactory, KeyMirrorState,
};
use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};
use td_host::{Mirror, Mirrored, Outcome};
use td_link::{Incoming, Session};

pub type Work = Box<dyn FnOnce(&Session) + Send>;

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
    let (presses, taps) = mpsc::sync_channel(16);
    let action_inner = inner.clone();
    let action_id = id.clone();
    let action_session = session.clone();
    thread::spawn(move || {
        for event in taps {
            if let Some(handler) = action_inner.actions.get() {
                handler(&action_id, &action_session, &event);
            }
        }
    });
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
        .spawn(move || forward(&inner, &id, generation, incoming, mirror, presses))
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

fn forward(
    inner: &Inner,
    id: &DeviceId,
    generation: u64,
    incoming: Receiver<Incoming>,
    mut mirror: Option<Mirror>,
    presses: mpsc::SyncSender<serde_json::Value>,
) {
    for item in incoming {
        match item {
            Incoming::Line(line) => inner.events.publish(&DeviceConsole {
                device: id.clone(),
                line,
            }),
            Incoming::Event(event) => {
                if event["m"] == "press" {
                    let _ = presses.try_send(event.clone());
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
