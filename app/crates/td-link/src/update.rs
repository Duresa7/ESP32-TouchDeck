use crate::bundle::{Bundle, sha256};
use crate::recover::Recovery;
use crate::{Incoming, Session, SessionConfig, XferFrame};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};
use td_core::frame::CHUNK_DATA_MAX;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state")]
pub enum UpdateState {
    Checking,
    Sending { pct: u8 },
    Rebooting,
    Confirming,
    Done { message: String },
    Failed { reason: String },
    Recovering { pct: u8 },
}
pub trait Connector {
    fn connect(&mut self) -> Result<(Session, Receiver<Incoming>), String>;
}
pub struct PortConnector<'a>(pub &'a str);
impl Connector for PortConnector<'_> {
    fn connect(&mut self) -> Result<(Session, Receiver<Incoming>), String> {
        Session::open(
            Box::new(crate::Serial::open(self.0).map_err(|e| e.to_string())?),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .map_err(|e| e.to_string())
    }
}
impl Connector for crate::MockDeck {
    fn connect(&mut self) -> Result<(Session, Receiver<Incoming>), String> {
        Session::open(
            Box::new(crate::MockDeck::connect(self)),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .map_err(|e| e.to_string())
    }
}
#[derive(Clone, Copy)]
pub struct Deadlines {
    pub reconnect: Duration,
    pub confirm: Duration,
    pub tick: Duration,
}
impl Default for Deadlines {
    fn default() -> Self {
        Self {
            reconnect: Duration::from_secs(15),
            confirm: Duration::from_secs(30),
            tick: Duration::from_millis(250),
        }
    }
}
const REQUEST: Duration = Duration::from_secs(2);
fn state(session: &Session, incoming: &Receiver<Incoming>) -> Result<Value, String> {
    session
        .send_line("update state")
        .map_err(|e| e.to_string())?;
    let end = Instant::now() + REQUEST;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        if let Ok(Incoming::Line(line)) = incoming.recv_timeout(left)
            && let Some(at) = line.find("@td ")
            && let Ok(value) = serde_json::from_str::<Value>(&line[at + 4..])
            && value["cmd"] == "update"
            && value["op"] == "state"
            && value["ok"] == true
        {
            return Ok(value);
        }
    }
    Err("no update state reply".into())
}
fn checked(session: &Session, message: Value, timeout: Duration) -> Result<Value, String> {
    let reply = session.call(&message, timeout).map_err(|e| e.to_string())?;
    if !reply.ok {
        return Err(reply.to_value().to_string());
    }
    Ok(reply.to_value())
}
fn send(
    session: &Session,
    bundle: &Bundle,
    emit: &mut dyn FnMut(UpdateState),
) -> Result<String, String> {
    let transfer = session.transfer().map_err(|e| e.to_string())?;
    let opened = checked(
        session,
        json!({"m":"update.begin","bytes":bundle.app().len(),"sha256":sha256(bundle.app())}),
        Duration::from_secs(15),
    )?;
    let id = opened["xfer"]
        .as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .filter(|n| *n != 0)
        .ok_or("bad update transfer id")?;
    let slot = opened["slot"]
        .as_str()
        .filter(|s| matches!(*s, "ota_0" | "ota_1"))
        .ok_or("bad update slot")?
        .to_owned();
    emit(UpdateState::Sending { pct: 0 });
    let chunks = bundle.app().chunks(CHUNK_DATA_MAX);
    let count = chunks.len();
    let mut shown = 0;
    for (idx, data) in chunks.enumerate() {
        let idx = u16::try_from(idx).map_err(|_| "image has too many chunks")?;
        let mut acked = false;
        for _ in 0..5 {
            transfer
                .send_chunk(id, idx, data)
                .map_err(|e| e.to_string())?;
            let end = Instant::now() + Duration::from_millis(200);
            while let Some(left) = end.checked_duration_since(Instant::now()) {
                match transfer.frames.recv_timeout(left) {
                    Ok(XferFrame::Ack { xfer, next }) if xfer == id && next == idx + 1 => {
                        acked = true;
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            if acked {
                break;
            }
        }
        if !acked {
            return Err(format!("chunk {idx} acknowledgement timeout"));
        }
        let pct = ((usize::from(idx) + 1) * 100 / count) as u8;
        if pct != shown {
            emit(UpdateState::Sending { pct });
            shown = pct;
        }
    }
    checked(
        session,
        json!({"m":"update.end","xfer":id}),
        Duration::from_secs(10),
    )?;
    Ok(slot)
}
fn confirm(
    connector: &mut dyn Connector,
    bundle: &Bundle,
    slot: Option<&str>,
    limits: Deadlines,
    emit: &mut dyn FnMut(UpdateState),
) -> Result<(), String> {
    let start = Instant::now();
    let mut confirming = None;
    loop {
        // Each attempt drops both serial handles before opening another connection.
        if let Ok((session, incoming)) = connector.connect()
            && let Ok(hello) = session.hello(Duration::from_millis(500))
            && hello.is_deck()
        {
            if confirming.is_none() {
                emit(UpdateState::Confirming);
                confirming = Some(Instant::now());
            }
            if let Ok(view) = state(&session, &incoming) {
                let wrong = hello.elf_sha256 != bundle.manifest.elf_sha256
                    || slot.is_some_and(|s| view["running"] != s);
                if wrong {
                    if slot.is_some_and(|s| view[s] == "ABORTED" || view[s] == "INVALID") {
                        return Err("rolled back".into());
                    }
                } else if view["confirmed"] == true {
                    return Ok(());
                }
            }
        }
        if let Some(at) = confirming {
            if at.elapsed() >= limits.confirm {
                return Err("confirm timeout".into());
            }
        } else if start.elapsed() >= limits.reconnect {
            return Err("port return timeout".into());
        }
        std::thread::sleep(limits.tick);
    }
}

/// Sends through the Device actor's existing session, then releases it before reconnecting.
pub fn run_session(
    session: &Session,
    connector: &mut dyn Connector,
    bundle: &Bundle,
    limits: Deadlines,
    emit: &mut dyn FnMut(UpdateState),
) -> Result<(), String> {
    emit(UpdateState::Checking);
    let hello = session
        .hello(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    if !hello.is_deck() {
        return Err("port is not a TouchDeck".into());
    }
    if hello.elf_sha256 == bundle.manifest.elf_sha256 {
        emit(UpdateState::Done {
            message: "Already on this build".into(),
        });
        return Ok(());
    }
    let slot = send(session, bundle, emit)?;
    session.close();
    emit(UpdateState::Rebooting);
    confirm(connector, bundle, Some(&slot), limits, emit)?;
    emit(UpdateState::Done {
        message: "Firmware updated".into(),
    });
    Ok(())
}

pub fn run(
    connector: &mut dyn Connector,
    recovery: Option<&mut dyn Recovery>,
    bundle: &Bundle,
    limits: Deadlines,
    emit: &mut dyn FnMut(UpdateState),
) -> Result<(), String> {
    emit(UpdateState::Checking);
    let result: Result<(), String> = (|| {
        let slot = if let Some(recovery) = recovery {
            emit(UpdateState::Recovering { pct: 0 });
            recovery.flash(bundle, emit)?;
            Some("ota_0".into())
        } else {
            let (session, _incoming) = connector.connect()?;
            let hello = session
                .hello(Duration::from_secs(10))
                .map_err(|e| e.to_string())?;
            if !hello.is_deck() {
                return Err("port is not a TouchDeck".into());
            }
            if hello.elf_sha256 == bundle.manifest.elf_sha256 {
                emit(UpdateState::Done {
                    message: "Already on this build".into(),
                });
                return Ok(());
            }
            Some(send(&session, bundle, emit)?)
        };
        emit(UpdateState::Rebooting);
        confirm(connector, bundle, slot.as_deref(), limits, emit)?;
        emit(UpdateState::Done {
            message: "Firmware updated".into(),
        });
        Ok(())
    })();
    if let Err(reason) = &result {
        emit(UpdateState::Failed {
            reason: reason.clone(),
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bundle() -> Bundle {
        Bundle::parse(include_bytes!("../tests/fixtures/touchdeck-test.tdfw")).unwrap()
    }
    fn limits() -> Deadlines {
        Deadlines {
            reconnect: Duration::from_millis(50),
            confirm: Duration::from_millis(50),
            tick: Duration::from_millis(1),
        }
    }
    #[test]
    fn update_transfers_reboots_and_confirms() {
        let mut mock = crate::MockDeck::new();
        mock.lose_chunks([1]);
        let mut states = Vec::new();
        run(&mut mock, None, &bundle(), limits(), &mut |s| {
            states.push(s)
        })
        .unwrap();
        assert_eq!(states.first(), Some(&UpdateState::Checking));
        assert!(states.contains(&UpdateState::Sending { pct: 100 }));
        assert!(states.contains(&UpdateState::Rebooting));
        assert!(states.contains(&UpdateState::Confirming));
        assert!(matches!(states.last(), Some(UpdateState::Done { .. })));
        assert_eq!(mock.reboots(), 1);
        let received = mock.received();
        assert!(
            received
                .iter()
                .any(|r| r["m"] == "update.begin" && r["sha256"] == sha256(bundle().app()))
        );
        assert!(received.iter().any(|r| r["m"] == "update.end"));
        let mut again = Vec::new();
        run(&mut mock, None, &bundle(), limits(), &mut |s| again.push(s)).unwrap();
        assert_eq!(
            again,
            [
                UpdateState::Checking,
                UpdateState::Done {
                    message: "Already on this build".into()
                }
            ]
        );
        assert_eq!(mock.reboots(), 1);
    }
    #[test]
    fn missing_confirmation_fails() {
        let mut mock = crate::MockDeck::new();
        mock.update_mode("no-confirm");
        let mut states = Vec::new();
        assert_eq!(
            run(&mut mock, None, &bundle(), limits(), &mut |s| states
                .push(s)),
            Err("confirm timeout".into())
        );
        assert_eq!(
            states.last(),
            Some(&UpdateState::Failed {
                reason: "confirm timeout".into()
            })
        );
    }
    #[test]
    fn rollback_is_distinct_from_success() {
        let mut mock = crate::MockDeck::new();
        mock.update_mode("rollback");
        assert_eq!(
            run(&mut mock, None, &bundle(), limits(), &mut |_| {}),
            Err("rolled back".into())
        );
    }
    struct Vanished {
        mock: crate::MockDeck,
        calls: u32,
    }
    impl Connector for Vanished {
        fn connect(&mut self) -> Result<(Session, Receiver<Incoming>), String> {
            self.calls += 1;
            if self.calls > 1 {
                Err("unplugged".into())
            } else {
                Connector::connect(&mut self.mock)
            }
        }
    }
    #[test]
    fn port_never_returns_fails() {
        let mut connector = Vanished {
            mock: crate::MockDeck::new(),
            calls: 0,
        };
        assert_eq!(
            run(&mut connector, None, &bundle(), limits(), &mut |_| {}),
            Err("port return timeout".into())
        );
    }
}
