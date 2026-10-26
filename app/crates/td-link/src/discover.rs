use std::io::ErrorKind;
use std::time::Duration;

use serde::Serialize;
use td_core::Hello;

use crate::session::{LinkError, Session, SessionConfig};
use crate::{PID, Serial, Transport, VID};

pub const HELLO_TIMEOUT: Duration = Duration::from_millis(500);

/// What one candidate port said when probed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Probe {
    Deck { hello: Hello },
    Busy { detail: String },
    Silent,
    NotDeck { detail: String },
    Unavailable { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub port: String,
    #[serde(flatten)]
    pub probe: Probe,
}

pub fn candidate_ports() -> Vec<String> {
    let mut ports: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter(|port| matches!(&port.port_type, serialport::SerialPortType::UsbPort(usb) if usb.vid == VID && usb.pid == PID))
        .map(|port| port.port_name)
        .collect();
    ports.sort();
    ports.dedup();
    ports
}

/// Probes every Espressif USB-Serial/JTAG port. A port in use or silent is a status,
/// never an error.
pub fn discover() -> Vec<Candidate> {
    candidate_ports()
        .into_iter()
        .map(|port| Candidate {
            probe: probe_port(&port),
            port,
        })
        .collect()
}

pub fn probe_port(port: &str) -> Probe {
    match Serial::open(port) {
        Ok(serial) => probe_transport(Box::new(serial)),
        Err(error)
            if error.kind() == ErrorKind::PermissionDenied
                || error.to_string().to_lowercase().contains("busy") =>
        {
            Probe::Busy {
                detail: error.to_string(),
            }
        }
        Err(error) => Probe::Unavailable {
            detail: error.to_string(),
        },
    }
}

pub fn probe_transport(transport: Box<dyn Transport>) -> Probe {
    let session = match Session::open(
        transport,
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    ) {
        Ok((session, _incoming)) => session,
        Err(error) => {
            return Probe::Unavailable {
                detail: error.to_string(),
            };
        }
    };
    match session.hello(HELLO_TIMEOUT) {
        Ok(hello) if hello.is_deck() => Probe::Deck { hello },
        Ok(hello) => Probe::NotDeck {
            detail: format!("product {:?} protocol {}", hello.product, hello.proto),
        },
        Err(LinkError::Timeout(_)) => Probe::Silent,
        Err(error) => Probe::NotDeck {
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockDeck;
    use std::time::Instant;

    #[test]
    fn a_deck_answers_with_its_hello() {
        let deck = MockDeck::new();
        let Probe::Deck { hello } = probe_transport(Box::new(deck.connect())) else {
            panic!("mock is a deck")
        };
        assert_eq!(hello.elf_sha256, crate::mock::MOCK_ELF_SHA256);
        assert_eq!(deck.received(), [serde_json::json!({"m": "hello"})]);
    }

    #[test]
    fn a_silent_port_is_reported_within_the_hello_window() {
        let deck = MockDeck::new();
        let transport = deck.connect();
        deck.stop();
        let started = Instant::now();
        assert_eq!(probe_transport(Box::new(transport)), Probe::Silent);
        assert!(started.elapsed() < HELLO_TIMEOUT + Duration::from_millis(400));
    }

    #[test]
    fn a_status_serializes_flat() {
        let candidate = Candidate {
            port: "COM9".into(),
            probe: Probe::Busy {
                detail: "Access is denied.".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(candidate).unwrap(),
            serde_json::json!({"port": "COM9", "status": "busy", "detail": "Access is denied."})
        );
    }
}
