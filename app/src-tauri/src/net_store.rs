//! Keyring-backed LTK store and known network decks configuration for `net-v1`.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use td_core::Hello;
use td_link::discover::{Candidate, NetDiscovered, Probe};
use td_link::net::{
    APP_ID_LEN, DECK_ID_LEN, LTK_LEN, NET_DEFAULT_PORT, NetError, NetErrorReason, PairCredential,
    PairSession,
};
use zeroize::Zeroize;

const SERVICE: &str = "touchdeck";

pub trait NetKeyStore: Send + Sync {
    fn get_ltk(&self, deck_id_hex: &str) -> Result<Option<[u8; LTK_LEN]>, String>;
    fn set_ltk(&self, deck_id_hex: &str, ltk: &[u8; LTK_LEN]) -> Result<(), String>;
    fn delete_ltk(&self, deck_id_hex: &str) -> Result<(), String>;
}

struct OsNetKeys;

impl OsNetKeys {
    fn entry(deck_id_hex: &str) -> Result<keyring::Entry, String> {
        let account = format!("net-v1-ltk-{}", deck_id_hex.to_ascii_lowercase());
        keyring::Entry::new(SERVICE, &account).map_err(|e| e.to_string())
    }
}

impl NetKeyStore for OsNetKeys {
    fn get_ltk(&self, deck_id_hex: &str) -> Result<Option<[u8; LTK_LEN]>, String> {
        match Self::entry(deck_id_hex)?.get_password() {
            Ok(mut hex_str) => {
                let decoded = hex::decode(hex_str.trim()).map_err(|e| e.to_string());
                hex_str.zeroize();
                let bytes = decoded?;
                if bytes.len() != LTK_LEN {
                    return Err("stored LTK has invalid length".into());
                }
                let mut ltk = [0u8; LTK_LEN];
                ltk.copy_from_slice(&bytes);
                Ok(Some(ltk))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set_ltk(&self, deck_id_hex: &str, ltk: &[u8; LTK_LEN]) -> Result<(), String> {
        let mut hex_str = hex::encode(ltk);
        let res = Self::entry(deck_id_hex)?
            .set_password(&hex_str)
            .map_err(|e| e.to_string());
        hex_str.zeroize();
        res
    }

    fn delete_ltk(&self, deck_id_hex: &str) -> Result<(), String> {
        match Self::entry(deck_id_hex)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Default)]
struct MemoryNetKeys {
    keys: Mutex<HashMap<String, [u8; LTK_LEN]>>,
}

impl NetKeyStore for MemoryNetKeys {
    fn get_ltk(&self, deck_id_hex: &str) -> Result<Option<[u8; LTK_LEN]>, String> {
        Ok(self
            .keys
            .lock()
            .unwrap()
            .get(&deck_id_hex.to_ascii_lowercase())
            .copied())
    }

    fn set_ltk(&self, deck_id_hex: &str, ltk: &[u8; LTK_LEN]) -> Result<(), String> {
        self.keys
            .lock()
            .unwrap()
            .insert(deck_id_hex.to_ascii_lowercase(), *ltk);
        Ok(())
    }

    fn delete_ltk(&self, deck_id_hex: &str) -> Result<(), String> {
        if let Some(mut old) = self
            .keys
            .lock()
            .unwrap()
            .remove(&deck_id_hex.to_ascii_lowercase())
        {
            old.zeroize();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownNetDeck {
    pub deck_id: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualTarget {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetConfig {
    pub app_id: String,
    pub app_name: String,
    #[serde(default)]
    pub decks: BTreeMap<String, KnownNetDeck>,
    #[serde(default)]
    pub manual_targets: Vec<ManualTarget>,
}

impl Default for NetConfig {
    fn default() -> Self {
        let mut raw = [0u8; APP_ID_LEN];
        getrandom::getrandom(&mut raw).expect("OS CSPRNG");
        Self {
            app_id: hex::encode(raw),
            app_name: "TouchDeck App".into(),
            decks: BTreeMap::new(),
            manual_targets: Vec::new(),
        }
    }
}

pub struct ActivePair {
    pub port: String,
    pub host: String,
    pub tcp_port: u16,
    pub deck_id: String,
    pub name: Option<String>,
    pub session: PairSession,
    pub failures: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetPairOffer {
    pub port: String,
    pub host: String,
    pub tcp_port: u16,
    pub deck_id: String,
    pub name: Option<String>,
    pub remaining_ms: u32,
    pub attempts_left: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetPairStep {
    pub paired: bool,
    pub locked: bool,
    pub attempts_left: u8,
    pub deck_id: String,
    pub port: String,
    pub error: Option<String>,
}

pub struct NetStore {
    keys: Arc<dyn NetKeyStore>,
    config_path: Mutex<Option<PathBuf>>,
    config: Mutex<NetConfig>,
    pub active_pair: Mutex<Option<ActivePair>>,
    mock: bool,
}

impl NetStore {
    pub fn new(mock: bool) -> Self {
        let keys: Arc<dyn NetKeyStore> = if mock {
            Arc::new(MemoryNetKeys::default())
        } else {
            Arc::new(OsNetKeys)
        };
        Self {
            keys,
            config_path: Mutex::new(None),
            config: Mutex::new(NetConfig::default()),
            active_pair: Mutex::new(None),
            mock,
        }
    }

    pub fn use_dir(&self, config_dir: Option<PathBuf>) {
        if self.mock {
            return;
        }
        if let Some(dir) = config_dir {
            let _ = std::fs::create_dir_all(&dir);
            let path = dir.join("net-decks.json");
            if let Ok(text) = std::fs::read_to_string(&path)
                && let Ok(loaded) = serde_json::from_str::<NetConfig>(&text)
                && loaded.app_id.len() == 32
            {
                *self.config.lock().unwrap() = loaded;
            }
            *self.config_path.lock().unwrap() = Some(path);
            self.persist();
        }
    }

    fn persist(&self) {
        if self.mock {
            return;
        }
        if let Some(path) = self.config_path.lock().unwrap().as_ref() {
            let cfg = self.config.lock().unwrap().clone();
            if let Ok(json) = serde_json::to_string_pretty(&cfg) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn app_identity(&self) -> ([u8; APP_ID_LEN], String) {
        let mut cfg = self.config.lock().unwrap();
        let mut app_id = [0u8; APP_ID_LEN];
        if let Ok(bytes) = hex::decode(&cfg.app_id)
            && bytes.len() == APP_ID_LEN
        {
            app_id.copy_from_slice(&bytes);
        } else {
            getrandom::getrandom(&mut app_id).expect("OS CSPRNG");
            cfg.app_id = hex::encode(app_id);
        }
        (app_id, cfg.app_name.clone())
    }

    pub fn add_manual_target(&self, input: &str, default_port: u16) -> Result<String, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Enter an IP address.".into());
        }
        let (host, port) = if let Ok(sock) = trimmed.parse::<SocketAddr>() {
            (sock.ip().to_string(), sock.port())
        } else if let Some((h, p)) = trimmed.rsplit_once(':') {
            let ip: IpAddr = h
                .parse()
                .map_err(|_| "Enter a valid IPv4 or IPv6 address.".to_owned())?;
            let port: u16 = p
                .parse::<u16>()
                .ok()
                .filter(|&port| port > 0)
                .ok_or_else(|| "Enter a valid TCP port (1-65535).".to_owned())?;
            (ip.to_string(), port)
        } else {
            let ip: IpAddr = trimmed
                .parse()
                .map_err(|_| "Enter a valid IPv4 or IPv6 address.".to_owned())?;
            (ip.to_string(), default_port)
        };

        {
            let mut cfg = self.config.lock().unwrap();
            if !cfg
                .manual_targets
                .iter()
                .any(|t| t.host == host && t.port == port)
            {
                cfg.manual_targets.push(ManualTarget {
                    host: host.clone(),
                    port,
                });
            }
        }
        self.persist();
        Ok(format_net_port(&host, port))
    }

    pub fn save_pending(
        &self,
        cred: &PairCredential,
        host: &str,
        port: u16,
        name: Option<String>,
    ) -> Result<(), NetError> {
        let deck_id_hex = hex::encode(cred.deck_id);
        self.keys
            .set_ltk(&deck_id_hex, &cred.ltk)
            .map_err(|detail| NetError::Local {
                reason: NetErrorReason::Storage,
                detail,
            })?;
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.decks.insert(
                deck_id_hex.clone(),
                KnownNetDeck {
                    deck_id: deck_id_hex,
                    host: host.to_owned(),
                    port,
                    name,
                    confirmed: false,
                },
            );
        }
        self.persist();
        Ok(())
    }

    pub fn confirm_deck(&self, deck_id_hex: &str, host: &str, port: u16) {
        {
            let mut cfg = self.config.lock().unwrap();
            if let Some(entry) = cfg.decks.get_mut(&deck_id_hex.to_ascii_lowercase()) {
                entry.confirmed = true;
                entry.host = host.to_owned();
                entry.port = port;
            }
        }
        self.persist();
    }

    pub fn forget_deck(&self, port: &str, deck_id: Option<&str>) -> Result<(), String> {
        let mut removed_ids = Vec::new();
        {
            let mut cfg = self.config.lock().unwrap();
            if let Some(id) = deck_id {
                let lower = id.to_ascii_lowercase();
                cfg.decks.remove(&lower);
                removed_ids.push(lower);
            }
            if let Some((host, tcp_port)) = parse_net_port(port) {
                let matching: Vec<String> = cfg
                    .decks
                    .iter()
                    .filter(|(_, d)| d.host == host && d.port == tcp_port)
                    .map(|(k, _)| k.clone())
                    .collect();
                for k in matching {
                    cfg.decks.remove(&k);
                    removed_ids.push(k);
                }
            }
        }
        self.persist();
        for id in removed_ids {
            self.keys.delete_ltk(&id)?;
        }
        Ok(())
    }

    pub fn resolve_credentials(
        &self,
        host: &str,
        port: u16,
    ) -> Result<([u8; DECK_ID_LEN], [u8; APP_ID_LEN], [u8; LTK_LEN], String), String> {
        let (app_id, _) = self.app_identity();
        let deck_entry = {
            let cfg = self.config.lock().unwrap();
            cfg.decks
                .values()
                .find(|d| d.host == host && d.port == port)
                .cloned()
        }
        .ok_or_else(|| format!("net:{host}:{port} is not paired yet"))?;

        let ltk = self
            .keys
            .get_ltk(&deck_entry.deck_id)?
            .ok_or_else(|| format!("no stored key for deck {}", deck_entry.deck_id))?;

        let id_bytes = hex::decode(&deck_entry.deck_id).map_err(|e| e.to_string())?;
        if id_bytes.len() != DECK_ID_LEN {
            return Err("invalid stored deck_id".into());
        }
        let mut deck_id = [0u8; DECK_ID_LEN];
        deck_id.copy_from_slice(&id_bytes);
        Ok((deck_id, app_id, ltk, deck_entry.deck_id))
    }

    pub fn build_candidates(
        &self,
        discovered: Vec<NetDiscovered>,
        connected: Option<(&str, &Hello)>,
    ) -> Vec<Candidate> {
        let mut cfg = self.config.lock().unwrap();
        // Update known deck host/port if mDNS reports a new address for a pinned deck_id
        for d in &discovered {
            if let Some(id) = d.deck_id.as_ref() {
                let lower = id.to_ascii_lowercase();
                if let Some(known) = cfg.decks.get_mut(&lower) {
                    known.host = d.host.clone();
                    known.port = d.port;
                    if d.name.is_some() {
                        known.name = d.name.clone();
                    }
                }
            }
        }

        let mut merged: Vec<NetDiscovered> = discovered;
        for known in cfg.decks.values() {
            if !merged.iter().any(|d| {
                d.deck_id.as_deref() == Some(known.deck_id.as_str())
                    || (d.host == known.host && d.port == known.port)
            }) {
                merged.push(NetDiscovered {
                    host: known.host.clone(),
                    port: known.port,
                    deck_id: Some(known.deck_id.clone()),
                    name: known.name.clone(),
                    proto: Some(1),
                    paired: None,
                });
            }
        }
        for manual in &cfg.manual_targets {
            if !merged
                .iter()
                .any(|d| d.host == manual.host && d.port == manual.port)
            {
                merged.push(NetDiscovered {
                    host: manual.host.clone(),
                    port: manual.port,
                    deck_id: None,
                    name: None,
                    proto: Some(1),
                    paired: None,
                });
            }
        }

        let mut out = Vec::new();
        for item in merged {
            let port_str = format_net_port(&item.host, item.port);
            let known_entry = item
                .deck_id
                .as_ref()
                .and_then(|id| cfg.decks.get(&id.to_ascii_lowercase()))
                .or_else(|| {
                    cfg.decks
                        .values()
                        .find(|d| d.host == item.host && d.port == item.port)
                })
                .cloned();

            let has_key = known_entry
                .as_ref()
                .and_then(|k| self.keys.get_ltk(&k.deck_id).ok().flatten())
                .is_some();

            let connected_hello = connected
                .filter(|(cport, _)| *cport == port_str)
                .map(|(_, h)| h.clone());

            let probe = if let (Some(k), true) = (known_entry, has_key) {
                Probe::NetPaired {
                    host: item.host,
                    tcp_port: item.port,
                    deck_id: k.deck_id,
                    name: item.name.or(k.name),
                    hello: connected_hello,
                }
            } else {
                Probe::NetUnpaired {
                    host: item.host,
                    tcp_port: item.port,
                    deck_id: item.deck_id,
                    name: item.name,
                }
            };
            out.push(Candidate {
                port: port_str,
                probe,
            });
        }
        drop(cfg);
        self.persist();
        out
    }
}

pub fn format_net_port(host: &str, port: u16) -> String {
    if port == NET_DEFAULT_PORT {
        format!("net:{host}")
    } else {
        format!("net:{host}:{port}")
    }
}

pub fn parse_net_port(port: &str) -> Option<(String, u16)> {
    let rest = port.strip_prefix("net:")?;
    if let Ok(sock) = rest.parse::<SocketAddr>() {
        return Some((sock.ip().to_string(), sock.port()));
    }
    if let Some((host, p)) = rest.rsplit_once(':')
        && let Ok(tcp_port) = p.parse::<u16>()
    {
        return Some((host.to_owned(), tcp_port));
    }
    Some((rest.to_owned(), NET_DEFAULT_PORT))
}