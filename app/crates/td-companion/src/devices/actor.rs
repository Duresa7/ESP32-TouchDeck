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

const INPUT_PACE: Duration = Duration::from_millis(20);

/// Dropping the handle ends the work thread, which closes the session and frees the port.
pub(super) struct Handle {
    pub generation: u64,
    pub opened: Instant,
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
    incoming: td_link::IncomingReceiver,
    mirror: Option<Mirror>,
) -> Handle {
    let (work, jobs) = mpsc::channel::<Work>();
    let session = Arc::new(session);
    let shared = session.clone();
    let (presses, taps) = mpsc::sync_channel::<serde_json::Value>(16);
    let action_inner = inner.clone();
    let action_id = id.clone();
    thread::spawn(move || {
        for event in taps {
            if let Some(handler) = action_inner.actions.get() {
                handler(&action_id, &event);
            }
        }
    });
    let (parts, part_taps) = mpsc::sync_channel::<serde_json::Value>(0);
    let part_inner = inner.clone();
    let part_id = id.clone();
    thread::spawn(move || {
        for event in part_taps {
            if let Some(handler) = part_inner.actions.get() {
                handler(&part_id, &event);
            }
        }
    });
    let (drags, inputs) = mpsc::sync_channel::<()>(1);
    let hub = Arc::new(parking_lot::Mutex::new(InputHub::default()));
    let input_hub = hub.clone();
    let input_inner = inner.clone();
    let input_id = id.clone();
    thread::spawn(move || run_inputs(&input_inner, &input_id, &inputs, &input_hub));
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
                Touches {
                    presses,
                    parts,
                    drags,
                    hub,
                },
            )
        })
        .expect("the Device forwarder starts");
    Handle {
        generation,
        opened: Instant::now(),
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

fn run_inputs(
    inner: &Inner,
    id: &DeviceId,
    inputs: &Receiver<()>,
    hub: &parking_lot::Mutex<InputHub>,
) {
    while inputs.recv().is_ok() {
        thread::sleep(INPUT_PACE);
        let _ = inputs.try_recv();
        if let Some(handler) = inner.inputs.get() {
            let generation = handler.generation();
            let batch = {
                let mut hub = hub.lock();
                hub.reset(generation);
                hub.take()
            };
            for input in batch {
                if handler.generation() != generation || !hub.lock().is_current(&input) {
                    break;
                }
                let Some(result) = handler.set(id, &input) else {
                    hub.lock().dropped(&input);
                    continue;
                };
                let outcome = {
                    let mut hub = hub.lock();
                    hub.reset(handler.generation());
                    if handler.generation() != generation {
                        break;
                    }
                    hub.settled(&input, result, Instant::now())
                };
                if let Some(outcome) = outcome {
                    handler.gesture(id, &input, outcome);
                }
            }
        }
    }
}

struct Touches {
    parts: mpsc::SyncSender<serde_json::Value>,
    presses: mpsc::SyncSender<serde_json::Value>,
    drags: mpsc::SyncSender<()>,
    hub: Arc<parking_lot::Mutex<InputHub>>,
}

fn forward(
    inner: &Inner,
    id: &DeviceId,
    generation: u64,
    incoming: impl IntoIterator<Item = Incoming>,
    mut mirror: Option<Mirror>,
    touches: Touches,
) {
    let mut gate = PressGate::default();
    let mut dropped_parts = 0_u64;
    for item in incoming {
        match item {
            Incoming::Line(line) => inner.events.publish(&DeviceConsole {
                device: id.clone(),
                line,
            }),
            Incoming::Event(event) => {
                if event["m"] == "press" {
                    if event.get("part").is_some() {
                        if !gate.accept(&event, Instant::now())
                            || touches.parts.try_send(event.clone()).is_err()
                        {
                            dropped_parts = dropped_parts.saturating_add(1);
                        }
                    } else {
                        let _ = touches.presses.try_send(event.clone());
                    }
                } else if event["m"] == "v2.input" {
                    if let Some(handler) = inner.inputs.get() {
                        let mut hub = touches.hub.lock();
                        hub.reset(handler.generation());
                        hub.push(&event);
                    }
                    let _ = touches.drags.try_send(());
                } else if event["m"] == "v2.page"
                    && let Some(page) = event["page"].as_str()
                {
                    inner.page_shown(id, generation, page);
                }
                if let Some(report) = Mirrored::parse(&event) {
                    key_report(inner, id, mirror.as_mut(), &report);
                }
                inner.events.publish(&DeviceEvent {
                    device: id.clone(),
                    event,
                });
            }
            Incoming::Overflow => {
                if let Some(mirror) = mirror.as_mut() {
                    mirror.release_all();
                }
                touches.hub.lock().cancel();
                inner.events.publish(&DeviceConsole {
                    device: id.clone(),
                    line: "Dropped Device events: overflow".into(),
                });
            }
            Incoming::Closed(reason) => {
                if dropped_parts > 0 {
                    inner.events.publish(&DeviceConsole {
                        device: id.clone(),
                        line: format!("Dropped control taps: {dropped_parts}"),
                    });
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_releases_held_keys_before_the_next_event() {
        let devices = super::super::DeviceManager::new(crate::api::Events::default());
        let sink = td_host::FakeSink::default();
        let mut mirror = Mirror::new(Box::new(sink.clone()));
        mirror.set_enabled(true);
        let down = serde_json::json!({"m":"key.report","seq":1,"type":"kbd","mods":1,"keys":[4,5]});
        let incoming = [
            Incoming::Event(down),
            Incoming::Overflow,
            Incoming::Line("after".into()),
        ];
        let incoming = incoming.into_iter().inspect(|item| {
            if matches!(item, Incoming::Line(_)) {
                let events = sink.take();
                assert_eq!(events.len(), 6);
                for event in &events[..3] {
                    let td_host::KeyEvent::Down(code) = event else {
                        panic!("expected down")
                    };
                    assert!(events[3..].contains(&td_host::KeyEvent::Up(*code)));
                }
            }
        });
        forward(
            &devices.inner,
            &DeviceId::parse("mock0001").unwrap(),
            0,
            incoming,
            Some(mirror),
            Touches {
                parts: mpsc::sync_channel(0).0,
                presses: mpsc::sync_channel(16).0,
                drags: mpsc::sync_channel(1).0,
                hub: Arc::default(),
            },
        );
    }
}
