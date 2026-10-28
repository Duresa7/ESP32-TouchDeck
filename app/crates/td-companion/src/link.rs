//! Link sessions for Layout and Asset work, one user per Device at a time.
//!
//! Until the Device manager keeps a session per Device on its own actor (#74), each use opens
//! a session to the Device's port and closes it afterwards. `Links::with` is the seam: the
//! actor's session replaces `Sessions::open` behind it.
use crate::{
    api::{ApiError, CommonError},
    devices::{DeviceManager, DeviceSummary, LinkState},
};
use axum::http::StatusCode;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use td_link::{LinkError, Session, SessionConfig, xfer::XferError};

/// Opens a link session to a connected Device's port. Blocking.
pub trait Sessions: Send + Sync + 'static {
    fn open(&self, device: &DeviceSummary, port: &str) -> Result<Session, String>;
}

/// Today's serial ports.
pub struct SerialSessions;
impl Sessions for SerialSessions {
    fn open(&self, _: &DeviceSummary, port: &str) -> Result<Session, String> {
        let serial = td_link::Serial::open(port).map_err(|e| format!("{port}: {e}"))?;
        open(Box::new(serial))
    }
}
fn open(transport: Box<dyn td_link::Transport>) -> Result<Session, String> {
    let config = SessionConfig {
        ping_every: None,
        ..SessionConfig::default()
    };
    Session::open(transport, config)
        .map(|(session, _incoming)| session)
        .map_err(|e| e.to_string())
}
#[cfg(any(test, feature = "test-hooks"))]
impl Sessions for crate::devices::MockPorts {
    fn open(&self, device: &DeviceSummary, _: &str) -> Result<Session, String> {
        let deck = self
            .deck(&device.id)
            .ok_or_else(|| format!("{} is not plugged in", device.id))?;
        open(Box::new(deck.connect()))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum DeviceLinkError {
    #[error("The Device does not exist.")]
    NotFound,
    #[error("The Device is not connected.")]
    NotConnected(Box<LinkState>),
    #[error("The Device didn't reply in time.")]
    Timeout,
    #[error("The link to the Device failed: {0}")]
    Link(String),
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl DeviceLinkError {
    pub fn from_xfer(error: XferError) -> Self {
        match error {
            XferError::Link(LinkError::Timeout(_)) => Self::Timeout,
            other => Self::Link(other.to_string()),
        }
    }
}
impl ApiError for DeviceLinkError {
    const CODES: &'static [&'static str] = &[
        "device.not_found",
        "device.not_connected",
        "device.timeout",
        "device.link",
    ];
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::NotConnected(_) => StatusCode::CONFLICT,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::Link(_) => StatusCode::BAD_GATEWAY,
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "device.not_found",
            Self::NotConnected(_) => "device.not_connected",
            Self::Timeout => "device.timeout",
            Self::Link(_) => "device.link",
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::NotConnected(link) => Some(serde_json::json!({ "link": link })),
            Self::Link(reason) => Some(serde_json::json!({ "reason": reason })),
            Self::Common(e) => e.details(),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Links {
    devices: DeviceManager,
    sessions: Arc<dyn Sessions>,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}
impl Links {
    pub(crate) fn new(devices: DeviceManager, sessions: Arc<dyn Sessions>) -> Self {
        Self {
            devices,
            sessions,
            locks: Default::default(),
        }
    }
    /// The Device's summary and port, when it's connected.
    pub fn connected(&self, id: &str) -> Result<(DeviceSummary, String), DeviceLinkError> {
        let device = self
            .devices
            .summaries()
            .into_iter()
            .find(|d| d.id == id)
            .ok_or(DeviceLinkError::NotFound)?;
        match &device.link {
            LinkState::Connected { port, .. } => {
                let port = port.clone();
                Ok((device, port))
            }
            other => Err(DeviceLinkError::NotConnected(Box::new(other.clone()))),
        }
    }
    /// Runs `work` with a session to the Device. Blocking: call it off the async runtime.
    /// Work on one Device waits for the work before it; other Devices don't wait.
    pub fn with<R>(
        &self,
        id: &str,
        work: impl FnOnce(&Session) -> R,
    ) -> Result<R, DeviceLinkError> {
        let lock = self.locks.lock().entry(id.into()).or_default().clone();
        let _held = lock.lock();
        let (device, port) = self.connected(id)?;
        let session = self
            .sessions
            .open(&device, &port)
            .map_err(DeviceLinkError::Link)?;
        let out = work(&session);
        session.close();
        Ok(out)
    }
}
