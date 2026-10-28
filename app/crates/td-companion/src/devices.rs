use crate::api::{Event, Events};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};
use ts_rs::TS;
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct FirmwareInfo {
    pub version: String,
    pub protocol: u32,
    pub build: String,
    pub recovery: bool,
    pub update_available: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum LinkState {
    Connected {
        port: String,
        since_ms: u64,
        protocol: u32,
    },
    Offline,
    Lost {
        reason: String,
        detail: String,
        at_ms: u64,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct KeyMirrorState {
    pub enabled: bool,
    pub reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct DeviceSummary {
    pub id: String,
    pub rev: u64,
    pub name: Option<String>,
    pub setup: String,
    pub link: LinkState,
    pub firmware: Option<FirmwareInfo>,
    pub capabilities: Option<serde_json::Value>,
    pub orientation: Option<String>,
    pub layout: Option<serde_json::Value>,
    pub key_mirror: KeyMirrorState,
    pub stable_id: bool,
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceAdded(pub DeviceSummary);
impl Event for DeviceAdded {
    const NAME: &'static str = "device.added";
    fn device(&self) -> Option<&str> {
        Some(&self.0.id)
    }
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceUpdated(pub DeviceSummary);
impl Event for DeviceUpdated {
    const NAME: &'static str = "device.updated";
    fn device(&self) -> Option<&str> {
        Some(&self.0.id)
    }
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceRemoved {
    pub id: String,
}
impl Event for DeviceRemoved {
    const NAME: &'static str = "device.removed";
    fn device(&self) -> Option<&str> {
        Some(&self.id)
    }
}
pub struct PortDevice {
    pub serial: Option<String>,
    pub port: String,
    pub hello: td_core::Hello,
}
pub trait PortSource: Send + Sync + 'static {
    fn scan(&self) -> Vec<PortDevice>;
}
/// Today's `td-link` discovery. Each port is probed once when it appears, because opening
/// it on every scan would disturb the Device on it. A port that didn't answer as a Device is
/// probed again after `RETRY`.
#[derive(Default)]
pub struct RealPorts {
    seen: Mutex<HashMap<(String, Option<String>), Seen>>,
}
struct Seen {
    hello: Option<td_core::Hello>,
    at: Instant,
}
const RETRY: Duration = Duration::from_secs(10);
impl PortSource for RealPorts {
    fn scan(&self) -> Vec<PortDevice> {
        let ports: Vec<(String, Option<String>)> = serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| match p.port_type {
                serialport::SerialPortType::UsbPort(u)
                    if u.vid == td_link::VID && u.pid == td_link::PID =>
                {
                    Some((p.port_name, u.serial_number))
                }
                _ => None,
            })
            .collect();
        let mut seen = self.seen.lock();
        seen.retain(|key, _| ports.contains(key));
        let mut found = Vec::new();
        for key in ports {
            let fresh = seen
                .get(&key)
                .is_some_and(|s| s.hello.is_some() || s.at.elapsed() < RETRY);
            if !fresh {
                let hello = match td_link::discover::probe_port(&key.0) {
                    td_link::discover::Probe::Deck { hello } => Some(hello),
                    _ => None,
                };
                seen.insert(
                    key.clone(),
                    Seen {
                        hello,
                        at: Instant::now(),
                    },
                );
            }
            if let Some(hello) = seen[&key].hello.clone() {
                found.push(PortDevice {
                    serial: key.1,
                    port: key.0,
                    hello,
                });
            }
        }
        found
    }
}
/// The serial lowercased with separators removed, or `port-<port>` when the driver reports none.
fn base_id(port: &PortDevice) -> String {
    let clean = |s: &str| {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .take(32)
            .collect::<String>()
    };
    port.serial
        .as_deref()
        .map(clean)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("port-{}", clean(&port.port)))
}
fn connected_on(state: &BTreeMap<String, DeviceSummary>, id: &str, port: &str) -> bool {
    state
        .get(id)
        .is_some_and(|d| matches!(&d.link, LinkState::Connected { port: p, .. } if p == port))
}
/// The id for a second Device whose serial normalizes to one already in use: the first 27
/// characters, `x`, and 4 hex digits of the port name's SHA-256. It stays the same while the
/// Device stays on that port.
fn suffixed_id(base: &str, port: &str) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(port.as_bytes());
    let head: String = base.chars().take(27).collect();
    format!("{head}x{:02x}{:02x}", hash[0], hash[1])
}
#[derive(Clone)]
pub struct DeviceManager {
    state: Arc<Mutex<BTreeMap<String, DeviceSummary>>>,
    events: Events,
}
impl DeviceManager {
    pub fn new(events: Events) -> Self {
        Self {
            state: Arc::new(Mutex::new(BTreeMap::new())),
            events,
        }
    }
    pub fn summaries(&self) -> Vec<DeviceSummary> {
        self.state.lock().values().cloned().collect()
    }
    pub fn refresh(&self, mut ports: Vec<PortDevice>) {
        let mut state = self.state.lock();
        let mut seen = std::collections::BTreeSet::new();
        // A Device that already holds an id keeps it, so a newcomer can't take it over.
        ports.sort_by_cached_key(|p| (!connected_on(&state, &base_id(p), &p.port), p.port.clone()));
        for port in ports {
            let base = base_id(&port);
            let suffixed = suffixed_id(&base, &port.port);
            let id = if connected_on(&state, &suffixed, &port.port) || seen.contains(&base) {
                suffixed
            } else {
                base.clone()
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let stable_id = port.serial.is_some() && id == base;
            if connected_on(&state, &id, &port.port) {
                continue;
            }
            let rev = state.get(&id).map_or(1, |d| d.rev + 1);
            let d = DeviceSummary {
                id: id.clone(),
                rev,
                name: None,
                setup: "needs-setup".into(),
                link: LinkState::Connected {
                    port: port.port,
                    since_ms: crate::now_ms(),
                    protocol: port.hello.proto,
                },
                firmware: Some(FirmwareInfo {
                    version: port.hello.version,
                    protocol: port.hello.proto,
                    build: port.hello.elf_sha256,
                    recovery: port.hello.recovery,
                    update_available: None,
                }),
                capabilities: None,
                orientation: None,
                layout: None,
                key_mirror: KeyMirrorState {
                    enabled: false,
                    reason: Some("Not configured".into()),
                },
                stable_id,
            };
            if state.contains_key(&id) {
                self.events.publish(&DeviceUpdated(d.clone()));
            } else {
                self.events.publish(&DeviceAdded(d.clone()));
            }
            state.insert(id, d);
        }
        let removed: Vec<_> = state
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in removed {
            state.remove(&id);
            self.events.publish(&DeviceRemoved { id });
        }
    }
}
#[cfg(any(test, feature = "test-hooks"))]
#[derive(Default, Clone)]
pub struct MockPorts {
    devices: Arc<Mutex<BTreeMap<String, td_link::MockDeck>>>,
}
#[cfg(any(test, feature = "test-hooks"))]
impl MockPorts {
    pub fn plug(&self, id: &str) {
        self.devices
            .lock()
            .insert(id.into(), td_link::MockDeck::new());
    }
    pub fn unplug(&self, id: &str) {
        self.devices.lock().remove(id);
    }
    /// The mock Device plugged in under `id`.
    pub fn deck(&self, id: &str) -> Option<td_link::MockDeck> {
        self.devices.lock().get(id).cloned()
    }
}
#[cfg(any(test, feature = "test-hooks"))]
impl PortSource for MockPorts {
    fn scan(&self) -> Vec<PortDevice> {
        self.devices
            .lock()
            .iter()
            .filter_map(|(id, mock)| {
                let td_link::discover::Probe::Deck { hello } =
                    td_link::discover::probe_transport(Box::new(mock.connect()))
                else {
                    return None;
                };
                Some(PortDevice {
                    serial: Some(id.clone()),
                    port: format!("mock-{id}"),
                    hello,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str, serial: &str) -> PortDevice {
        let td_link::discover::Probe::Deck { hello } =
            td_link::discover::probe_transport(Box::new(td_link::MockDeck::new().connect()))
        else {
            panic!("the mock Device should answer")
        };
        PortDevice {
            serial: Some(serial.into()),
            port: name.into(),
            hello,
        }
    }
    fn ids(devices: &DeviceManager) -> Vec<(String, u64, bool)> {
        devices
            .summaries()
            .into_iter()
            .map(|d| (d.id, d.rev, d.stable_id))
            .collect()
    }

    #[test]
    fn serials_that_normalize_alike_get_distinct_stable_ids() {
        let devices = DeviceManager::new(Events::default());
        devices.refresh(vec![port("COM5", "AB:12")]);
        // A newcomer on a lower port sorts first but can't take the id.
        devices.refresh(vec![port("COM3", "ab12"), port("COM5", "AB:12")]);
        let second = suffixed_id("ab12", "COM3");
        assert_eq!(
            ids(&devices),
            [("ab12".into(), 1, true), (second.clone(), 1, false)]
        );
        assert!(second.len() <= 32 && second.chars().all(|c| c.is_ascii_alphanumeric()));
        // Scan order doesn't matter, and nothing changes between scans.
        devices.refresh(vec![port("COM5", "AB:12"), port("COM3", "ab12")]);
        assert_eq!(
            ids(&devices),
            [("ab12".into(), 1, true), (second.clone(), 1, false)]
        );
        // The second Device keeps its id after the first one leaves.
        devices.refresh(vec![port("COM3", "ab12")]);
        assert_eq!(ids(&devices), [(second, 1, false)]);
    }

    #[test]
    fn a_long_serial_keeps_the_id_within_32_characters() {
        let devices = DeviceManager::new(Events::default());
        let serial = "a".repeat(40);
        devices.refresh(vec![port("COM1", &serial), port("COM2", &serial)]);
        let ids: Vec<_> = devices.summaries().into_iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|id| id.len() == 32), "{ids:?}");
    }
}
