use crate::api::Events;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
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
pub struct PortDevice {
    pub serial: Option<String>,
    pub port: String,
    pub hello: td_core::Hello,
}
pub trait PortSource: Send + Sync + 'static {
    fn scan(&self) -> Vec<PortDevice>;
}
pub struct RealPorts;
impl PortSource for RealPorts {
    fn scan(&self) -> Vec<PortDevice> {
        let ports = serialport::available_ports().unwrap_or_default();
        td_link::discover::discover()
            .into_iter()
            .filter_map(|candidate| {
                let td_link::discover::Probe::Deck { hello } = candidate.probe else {
                    return None;
                };
                let serial = ports
                    .iter()
                    .find(|p| p.port_name == candidate.port)
                    .and_then(|p| match &p.port_type {
                        serialport::SerialPortType::UsbPort(u) => u.serial_number.clone(),
                        _ => None,
                    });
                Some(PortDevice {
                    serial,
                    port: candidate.port,
                    hello,
                })
            })
            .collect()
    }
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
        self.state.lock().unwrap().values().cloned().collect()
    }
    pub fn refresh(&self, ports: Vec<PortDevice>) {
        let mut state = self.state.lock().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for port in ports {
            let stable_id = port.serial.is_some();
            let clean = |s: &str| {
                s.chars()
                    .filter(char::is_ascii_alphanumeric)
                    .flat_map(char::to_lowercase)
                    .take(32)
                    .collect::<String>()
            };
            let id = port
                .serial
                .as_deref()
                .map(clean)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("port-{}", clean(&port.port)));
            seen.insert(id.clone());
            if state
                .get(&id)
                .is_some_and(|d| matches!(&d.link,LinkState::Connected{port:p,..} if p==&port.port))
            {
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
            let event = if state.contains_key(&id) {
                "device.updated"
            } else {
                "device.added"
            };
            self.events
                .publish(serde_json::json!({"type":event,"device":id,"data":d}));
            state.insert(id, d);
        }
        let removed: Vec<_> = state
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in removed {
            state.remove(&id);
            self.events
                .publish(serde_json::json!({"type":"device.removed","data":{"id":id}}));
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
            .unwrap()
            .insert(id.into(), td_link::MockDeck::new());
    }
    pub fn unplug(&self, id: &str) {
        self.devices.lock().unwrap().remove(id);
    }
}
#[cfg(any(test, feature = "test-hooks"))]
impl PortSource for MockPorts {
    fn scan(&self) -> Vec<PortDevice> {
        self.devices
            .lock()
            .unwrap()
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
