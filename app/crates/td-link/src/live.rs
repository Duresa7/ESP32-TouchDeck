//! Live values on a protocol 2 Device, as `schema/protocol/link-v2.md` § "Layout references and live values"
//! defines them.
//!
//! `v2.live.set` sends one value for one stream that a Tile on the active Layout declares. `layout` names that
//! Layout by the SHA-256 of its committed bytes, or `builtin`, so a late value never lands on a newer Layout that
//! reuses the Page and Tile ids. `n` rises per stream within a session, and `ttl_ms` is how long the Device draws
//! the value before it shows it as stale. Every call here needs protocol 2 selected first.

use std::fmt;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use td_core::layout::StreamType;
pub use td_core::live::{LiveValue, VALUE_MAX};

use crate::session::{LinkError, Session};

pub const TTL_MIN_MS: u32 = 1_000;
pub const TTL_MAX_MS: u32 = 60_000;
/// The serialized size one request counts as against `caps.live_total_hz`. A larger request counts as one more
/// update for each further 256 bytes, so a long series costs the budget it uses.
pub const REQUEST_BYTES: usize = 256;
/// The `layout` that names the built-in Layout.
pub const BUILTIN: &str = "builtin";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// One `v2.live.set` request.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveSet<'a> {
    pub layout: &'a str,
    pub page: &'a str,
    pub tile: &'a str,
    pub stream: &'a str,
    pub n: u32,
    pub ttl_ms: u32,
    pub value: &'a LiveValue,
}

impl LiveSet<'_> {
    pub const NAME: &'static str = "v2.live.set";

    /// The request with its fields in the protocol's order.
    pub fn to_payload(&self) -> Vec<u8> {
        #[derive(Serialize)]
        struct Wire<'a> {
            m: &'static str,
            layout: &'a str,
            page: &'a str,
            tile: &'a str,
            stream: &'a str,
            n: u32,
            ttl_ms: u32,
            #[serde(rename = "type")]
            kind: StreamType,
            value: &'a LiveValue,
        }
        serde_json::to_vec(&Wire {
            m: Self::NAME,
            layout: self.layout,
            page: self.page,
            tile: self.tile,
            stream: self.stream,
            n: self.n,
            ttl_ms: self.ttl_ms,
            kind: self.value.kind(),
            value: self.value,
        })
        .expect("live requests serialize")
    }
}

/// How many updates a request of this many serialized bytes counts as against the Device's budget.
pub fn cost(bytes: usize) -> u32 {
    bytes.div_ceil(REQUEST_BYTES).max(1) as u32
}

/// The `layout` a live request names a committed Layout by: the SHA-256 of its exact bytes in lowercase hex.
pub fn layout_ref(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The stable `reason` of a refused live request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiveReason {
    Layout,
    Page,
    Tile,
    Stream,
    Type,
    Size,
    Asset,
    Sequence,
    Rate,
    Args,
    Memory,
    NotNegotiated,
    Unsupported,
    Other(String),
}

impl LiveReason {
    const NAMES: [(&'static str, LiveReason); 13] = [
        ("layout", LiveReason::Layout),
        ("page", LiveReason::Page),
        ("tile", LiveReason::Tile),
        ("stream", LiveReason::Stream),
        ("type", LiveReason::Type),
        ("size", LiveReason::Size),
        ("asset", LiveReason::Asset),
        ("sequence", LiveReason::Sequence),
        ("rate", LiveReason::Rate),
        ("args", LiveReason::Args),
        ("memory", LiveReason::Memory),
        ("not-negotiated", LiveReason::NotNegotiated),
        ("unsupported", LiveReason::Unsupported),
    ];

    pub fn parse(reason: &str) -> LiveReason {
        Self::NAMES
            .iter()
            .find(|(name, _)| *name == reason)
            .map_or_else(|| LiveReason::Other(reason.to_owned()), |(_, r)| r.clone())
    }

    pub fn name(&self) -> &str {
        match self {
            LiveReason::Other(other) => other,
            known => Self::NAMES
                .iter()
                .find(|(_, r)| r == known)
                .map_or("", |(name, _)| name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveError {
    Link(LinkError),
    /// The Device refused the value. `retry_ms` comes with `rate`: when the window that refused it ends.
    Refused {
        reason: LiveReason,
        error: String,
        retry_ms: Option<u32>,
    },
}

impl LiveError {
    pub fn reason(&self) -> Option<&LiveReason> {
        match self {
            LiveError::Refused { reason, .. } => Some(reason),
            LiveError::Link(_) => None,
        }
    }
}

impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiveError::Link(error) => error.fmt(f),
            LiveError::Refused { reason, error, .. } => {
                write!(
                    f,
                    "the Device refused the live value: {error} ({})",
                    reason.name()
                )
            }
        }
    }
}

impl std::error::Error for LiveError {}

impl From<LinkError> for LiveError {
    fn from(error: LinkError) -> Self {
        LiveError::Link(error)
    }
}

/// Sends one live value and returns the `n` the Device accepted.
pub fn set_live(session: &Session, set: &LiveSet) -> Result<u32, LiveError> {
    let reply = session.call_payload(LiveSet::NAME, set.to_payload(), REQUEST_TIMEOUT)?;
    let number = |key: &str| {
        reply
            .fields
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
    };
    if !reply.ok {
        let text = |key: &str| {
            reply
                .fields
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        return Err(LiveError::Refused {
            reason: LiveReason::parse(&text("reason")),
            error: text("error"),
            retry_ms: number("retry_ms"),
        });
    }
    number("n").ok_or_else(|| {
        LiveError::Link(LinkError::Reply(td_core::ReplyError::Fields(
            "a live reply needs n".into(),
        )))
    })
}
