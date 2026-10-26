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
    Deck {
        hello: Hello,
    },
    NetUnpaired {
        host: String,
        tcp_port: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        deck_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    NetPaired {
        host: String,
        tcp_port: u16,
        deck_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hello: Option<Hello>,
    },
    Busy {
        detail: String,
    },
    Silent,
    NotDeck {
        detail: String,
    },
    Unavailable {
        detail: String,
    },
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


use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::Instant;
use serde::Deserialize;

pub const MDNS_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MDNS_PORT: u16 = 5353;
pub const TOUCHDECK_MDNS_SERVICE: &str = "_touchdeck._tcp.local.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetDiscovered {
    pub host: String,
    pub port: u16,
    pub deck_id: Option<String>,
    pub name: Option<String>,
    pub proto: Option<u8>,
    pub paired: Option<u8>,
}

pub fn build_mdns_ptr_query(service: &str) -> Vec<u8> {
    let mut pkt = Vec::with_capacity(64);
    // Header: ID=0, Flags=0, QDCOUNT=1, ANCOUNT=0, NSCOUNT=0, ARCOUNT=0
    pkt.extend_from_slice(&[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in service.trim_end_matches('.').split('.') {
        let b = label.as_bytes();
        pkt.push(b.len() as u8);
        pkt.extend_from_slice(b);
    }
    pkt.push(0);
    // QTYPE = PTR (12), QCLASS = IN (1) with QU bit 0
    pkt.extend_from_slice(&12u16.to_be_bytes());
    pkt.extend_from_slice(&1u16.to_be_bytes());
    pkt
}

fn read_dns_name(pkt: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels = Vec::new();
    let mut end_pos = None;
    let mut hops = 0;
    loop {
        if pos >= pkt.len() || hops > 32 {
            return None;
        }
        let len = pkt[pos];
        if len == 0 {
            pos += 1;
            if end_pos.is_none() {
                end_pos = Some(pos);
            }
            break;
        }
        if (len & 0xC0) == 0xC0 {
            if pos + 1 >= pkt.len() {
                return None;
            }
            let offset = (((len & 0x3F) as usize) << 8) | (pkt[pos + 1] as usize);
            if end_pos.is_none() {
                end_pos = Some(pos + 2);
            }
            pos = offset;
            hops += 1;
            continue;
        }
        if (len & 0xC0) != 0 {
            return None;
        }
        let label_len = len as usize;
        pos += 1;
        if pos + label_len > pkt.len() {
            return None;
        }
        let label = std::str::from_utf8(&pkt[pos..pos + label_len]).ok()?;
        labels.push(label.to_owned());
        pos += label_len;
    }
    Some((labels.join("."), end_pos?))
}

pub fn parse_mdns_packet(pkt: &[u8], source_ip: IpAddr) -> Option<NetDiscovered> {
    if pkt.len() < 12 {
        return None;
    }
    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let ancount = u16::from_be_bytes([pkt[6], pkt[7]]) as usize;
    let nscount = u16::from_be_bytes([pkt[8], pkt[9]]) as usize;
    let arcount = u16::from_be_bytes([pkt[10], pkt[11]]) as usize;
    let total_rr = ancount + nscount + arcount;
    if total_rr == 0 {
        return None;
    }

    let mut pos = 12;
    for _ in 0..qdcount {
        let (_, next) = read_dns_name(pkt, pos)?;
        if next + 4 > pkt.len() {
            return None;
        }
        pos = next + 4;
    }

    let mut touched_touchdeck = false;
    let mut srv_port: Option<u16> = None;
    let mut srv_target: Option<String> = None;
    let mut a_records: HashMap<String, Ipv4Addr> = HashMap::new();
    let mut txt_kv: HashMap<String, String> = HashMap::new();

    for _ in 0..total_rr {
        if pos >= pkt.len() {
            break;
        }
        let (rr_name, next) = read_dns_name(pkt, pos)?;
        if next + 10 > pkt.len() {
            return None;
        }
        let rtype = u16::from_be_bytes([pkt[next], pkt[next + 1]]);
        let rdlen = u16::from_be_bytes([pkt[next + 8], pkt[next + 9]]) as usize;
        let rdata_start = next + 10;
        let rdata_end = rdata_start + rdlen;
        if rdata_end > pkt.len() {
            return None;
        }
        let rr_lower = rr_name.to_ascii_lowercase();
        if rr_lower.contains("_touchdeck._tcp") {
            touched_touchdeck = true;
        }
        match rtype {
            12 => {
                // PTR
                if let Some((ptr_target, _)) = read_dns_name(pkt, rdata_start) {
                    if ptr_target.to_ascii_lowercase().contains("_touchdeck._tcp") {
                        touched_touchdeck = true;
                    }
                }
            }
            33 if rdlen >= 6 => {
                // SRV
                let port = u16::from_be_bytes([pkt[rdata_start + 4], pkt[rdata_start + 5]]);
                if let Some((target, _)) = read_dns_name(pkt, rdata_start + 6) {
                    if rr_lower.contains("_touchdeck._tcp") {
                        srv_port = Some(port);
                        srv_target = Some(target);
                    }
                }
            }
            16 => {
                // TXT
                if rr_lower.contains("_touchdeck._tcp") {
                    let mut tpos = rdata_start;
                    while tpos < rdata_end {
                        let slen = pkt[tpos] as usize;
                        tpos += 1;
                        if tpos + slen > rdata_end {
                            break;
                        }
                        if let Ok(entry) = std::str::from_utf8(&pkt[tpos..tpos + slen]) {
                            if let Some((k, v)) = entry.split_once('=') {
                                txt_kv.insert(k.to_owned(), v.to_owned());
                            }
                        }
                        tpos += slen;
                    }
                }
            }
            1 if rdlen == 4 => {
                // A
                let ip = Ipv4Addr::new(
                    pkt[rdata_start],
                    pkt[rdata_start + 1],
                    pkt[rdata_start + 2],
                    pkt[rdata_start + 3],
                );
                a_records.insert(rr_lower, ip);
            }
            _ => {}
        }
        pos = rdata_end;
    }

    if !touched_touchdeck {
        return None;
    }

    let host_ip = srv_target
        .as_ref()
        .and_then(|t| a_records.get(&t.to_ascii_lowercase()).copied())
        .map(IpAddr::V4)
        .unwrap_or(source_ip);

    let deck_id = txt_kv.get("id").and_then(|id| {
        let lower = id.to_ascii_lowercase();
        (lower.len() == 12 && lower.bytes().all(|b| b.is_ascii_hexdigit())).then_some(lower)
    });
    let proto = txt_kv.get("proto").and_then(|p| p.parse::<u8>().ok());
    let paired = txt_kv.get("paired").and_then(|p| p.parse::<u8>().ok());
    let name = txt_kv.get("name").cloned().or_else(|| {
        srv_target
            .as_ref()
            .map(|t| t.trim_end_matches(".local").to_owned())
    });

    Some(NetDiscovered {
        host: host_ip.to_string(),
        port: srv_port.unwrap_or(crate::net::NET_DEFAULT_PORT),
        deck_id,
        name,
        proto,
        paired,
    })
}

pub fn discover_mdns(timeout: Duration) -> Vec<NetDiscovered> {
    let Ok(sock) = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)) else {
        return Vec::new();
    };
    let _ = sock.set_multicast_loop_v4(true);
    let query = build_mdns_ptr_query(TOUCHDECK_MDNS_SERVICE);
    let dest = SocketAddr::V4(SocketAddrV4::new(MDNS_MULTICAST_ADDR, MDNS_PORT));
    let _ = sock.send_to(&query, dest);

    let deadline = Instant::now() + timeout;
    let mut found: Vec<NetDiscovered> = Vec::new();
    let mut buf = [0u8; 1500];
    while Instant::now() < deadline {
        let rem = (deadline - Instant::now()).max(Duration::from_millis(10));
        let _ = sock.set_read_timeout(Some(rem));
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let Some(disc) = parse_mdns_packet(&buf[..n], src.ip()) {
                    if !found
                        .iter()
                        .any(|d| d.deck_id == disc.deck_id && d.host == disc.host && d.port == disc.port)
                    {
                        found.push(disc);
                    }
                }
            }
            Err(_) => break,
        }
    }
    found
}

#[cfg(test)]
mod mdns_tests {
    use super::*;

    #[test]
    fn parses_mdns_touchdeck_response() {
        let query = build_mdns_ptr_query(TOUCHDECK_MDNS_SERVICE);
        assert!(query.len() > 16);

        // Build a synthetic mDNS response with 3 answer RRs: SRV (33), TXT (16), A (1)
        let mut pkt = vec![
            0x00, 0x00, // ID
            0x84, 0x00, // Response, authoritative
            0x00, 0x00, // QDCOUNT = 0
            0x00, 0x03, // ANCOUNT = 3
            0x00, 0x00, // NSCOUNT = 0
            0x00, 0x00, // ARCOUNT = 0
        ];
        let push_name = |buf: &mut Vec<u8>, name: &str| {
            for label in name.split('.') {
                buf.push(label.len() as u8);
                buf.extend_from_slice(label.as_bytes());
            }
            buf.push(0);
        };
        let push_rr = |buf: &mut Vec<u8>, name: &str, rtype: u16, rdata: &[u8]| {
            push_name(buf, name);
            buf.extend_from_slice(&rtype.to_be_bytes());
            buf.extend_from_slice(&1u16.to_be_bytes()); // CLASS IN
            buf.extend_from_slice(&120u32.to_be_bytes()); // TTL
            buf.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            buf.extend_from_slice(rdata);
        };

        // 1. SRV record for touchdeck-123456._touchdeck._tcp.local -> touchdeck-123456.local:47820
        let mut srv_rdata = vec![0, 0, 0, 0]; // priority=0, weight=0
        srv_rdata.extend_from_slice(&47_820u16.to_be_bytes());
        push_name(&mut srv_rdata, "touchdeck-123456.local");
        push_rr(
            &mut pkt,
            "touchdeck-123456._touchdeck._tcp.local",
            33,
            &srv_rdata,
        );

        // 2. TXT record with id=020000123456, proto=1, name=touchdeck-123456, paired=1
        let mut txt_rdata = Vec::new();
        for entry in [
            "id=020000123456",
            "proto=1",
            "name=touchdeck-123456",
            "paired=1",
        ] {
            txt_rdata.push(entry.len() as u8);
            txt_rdata.extend_from_slice(entry.as_bytes());
        }
        push_rr(
            &mut pkt,
            "touchdeck-123456._touchdeck._tcp.local",
            16,
            &txt_rdata,
        );

        // 3. A record for touchdeck-123456.local -> 192.168.50.42
        push_rr(
            &mut pkt,
            "touchdeck-123456.local",
            1,
            &[192, 168, 50, 42],
        );

        let parsed =
            parse_mdns_packet(&pkt, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))).expect("parses mDNS");
        assert_eq!(parsed.host, "192.168.50.42");
        assert_eq!(parsed.port, 47_820);
        assert_eq!(parsed.deck_id.as_deref(), Some("020000123456"));
        assert_eq!(parsed.name.as_deref(), Some("touchdeck-123456"));
        assert_eq!(parsed.proto, Some(1));
        assert_eq!(parsed.paired, Some(1));
    }
}