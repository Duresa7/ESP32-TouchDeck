pub mod edit;
pub mod frame;
pub mod layout;
pub mod settings;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use frame::{Body, Frame, FrameError, Item, Role, StreamDecoder};
pub use layout::{Layout, LayoutError, Orientation, Rule};
pub use settings::Settings;

pub const PROTOCOL: u32 = 1;
pub const ICON_PAGE: usize = 32;

/// A request the app sends in a message frame, tagged by `m` as on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "m")]
pub enum Request {
    #[serde(rename = "hello")]
    Hello,
    #[serde(rename = "ping")]
    Ping {
        #[serde(default)]
        n: Option<u64>,
    },
    #[serde(rename = "link.state")]
    LinkState,
    #[serde(rename = "icon.list")]
    IconList {
        #[serde(default)]
        cursor: Option<u32>,
    },
    #[serde(rename = "settings.get")]
    SettingsGet,
    #[serde(rename = "settings.put")]
    SettingsPut(Settings),
}

impl Request {
    pub fn name(&self) -> &'static str {
        match self {
            Request::Hello => "hello",
            Request::Ping { .. } => "ping",
            Request::LinkState => "link.state",
            Request::IconList { .. } => "icon.list",
            Request::SettingsGet => "settings.get",
            Request::SettingsPut(_) => "settings.put",
        }
    }

    pub fn to_payload(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("requests serialize")
    }
}

/// An event the deck sends with flags 2, tagged by `m`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "m")]
pub enum Event {
    #[serde(rename = "press")]
    Press {
        page: String,
        tile: String,
        kind: String,
        seq: u32,
        t: u64,
    },
    #[serde(rename = "key.report")]
    KeyReport {
        seq: u32,
        t: u64,
        #[serde(rename = "type")]
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mods: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keys: Option<Vec<u8>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<u16>,
    },
}

/// A response message: the request's `m`, `ok`, and the fields that message defines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub m: String,
    pub ok: bool,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyError {
    NotJson(String),
    Failed(String),
    Fields(String),
}

impl std::fmt::Display for ReplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplyError::NotJson(error) => write!(f, "reply is not a message object: {error}"),
            ReplyError::Failed(error) => write!(f, "deck refused the request: {error}"),
            ReplyError::Fields(error) => write!(f, "reply fields: {error}"),
        }
    }
}

impl std::error::Error for ReplyError {}

impl Reply {
    pub fn parse(payload: &[u8]) -> Result<Reply, ReplyError> {
        serde_json::from_slice(payload).map_err(|error| ReplyError::NotJson(error.to_string()))
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("replies serialize")
    }

    pub fn fields<T: serde::de::DeserializeOwned>(&self) -> Result<T, ReplyError> {
        if !self.ok {
            let error = self
                .fields
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("no error given");
            return Err(ReplyError::Failed(error.to_owned()));
        }
        serde_json::from_value(Value::Object(self.fields.clone()))
            .map_err(|error| ReplyError::Fields(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub product: String,
    pub proto: u32,
    pub elf_sha256: String,
    pub version: String,
    pub running: String,
    pub sinks: Vec<String>,
    pub recovery: bool,
    pub layout: String,
}

impl Hello {
    pub fn is_deck(&self) -> bool {
        self.product == "touchdeck" && self.proto == PROTOCOL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    pub t: u64,
    pub n: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkState {
    pub online: bool,
    pub frames_rx: u64,
    pub frames_tx: u64,
    pub crc_errors: u64,
    pub dropped: u64,
    pub last_rx_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IconPage {
    pub ids: Vec<String>,
    pub next: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_the_wire_names() {
        assert_eq!(Request::Hello.to_payload(), br#"{"m":"hello"}"#);
        assert_eq!(Request::LinkState.to_payload(), br#"{"m":"link.state"}"#);
        assert_eq!(
            Request::Ping { n: Some(0) }.to_payload(),
            br#"{"m":"ping","n":0}"#
        );
        let put = Request::SettingsPut(Settings {
            idle_timeout_s: 120,
            orientation: Orientation::Portrait,
        });
        assert_eq!(
            put.to_payload(),
            br#"{"m":"settings.put","idle_timeout_s":120,"orientation":"portrait"}"#
        );
    }

    #[test]
    fn a_refused_reply_carries_its_error() {
        let reply = Reply::parse(br#"{"m":"nope","ok":false,"error":"unknown message"}"#).unwrap();
        assert_eq!(
            reply.fields::<Pong>(),
            Err(ReplyError::Failed("unknown message".into()))
        );
    }

    #[test]
    fn hello_fields_parse_from_a_reply() {
        let reply = Reply::parse(
            br#"{"m":"hello","ok":true,"product":"touchdeck","proto":1,"elf_sha256":"ab","version":"v","running":"ota_0","sinks":["log"],"recovery":false,"layout":"builtin"}"#,
        )
        .unwrap();
        assert!(reply.fields::<Hello>().unwrap().is_deck());
    }
}
