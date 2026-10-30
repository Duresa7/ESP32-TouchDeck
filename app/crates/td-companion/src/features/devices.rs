//! Devices, New boards, ports and the read-only console.
use crate::{
    api::{ApiError, CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req},
    devices::{
        DeviceAdded, DeviceConsole, DeviceEvent, DeviceId, DeviceKeyMirror, DeviceRemoved,
        DeviceSummary, DeviceUpdated, Forget as Forgotten, LinkState, NewBoard, NewBoardAdded,
        NewBoardRemoved, PortProbe, Reach, console,
    },
};
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use td_core::{Request, Settings};
use td_link::LinkError;
use ts_rs::TS;

/// How long a settings or name request may take on the link.
const SETTINGS_TIMEOUT: Duration = Duration::from_secs(2);
const BRIGHTNESS_MAX: u8 = 100;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("The Device does not exist.")]
    NotFound,
    #[error("The Device is not connected.")]
    NotConnected { link: LinkState },
    #[error("The Device is connected. Disconnect it first.")]
    Connected,
    #[error("The Device didn't reply in time.")]
    Timeout,
    #[error("The link to the Device failed: {reason}")]
    Link { reason: String },
    #[error("This needs newer Firmware on the Device.")]
    NeedsFirmware { field: &'static str },
    #[error("The console line isn't on the read-only allowlist.")]
    ConsoleForbidden,
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl ApiError for DeviceError {
    const CODES: &'static [&'static str] = &[
        "device.not_found",
        "device.not_connected",
        "device.connected",
        "device.timeout",
        "device.link",
        "device.needs_firmware",
        "console.forbidden",
    ];
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::NotConnected { .. } | Self::Connected | Self::NeedsFirmware { .. } => {
                StatusCode::CONFLICT
            }
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::Link { .. } => StatusCode::BAD_GATEWAY,
            Self::ConsoleForbidden => StatusCode::FORBIDDEN,
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "device.not_found",
            Self::NotConnected { .. } => "device.not_connected",
            Self::Connected => "device.connected",
            Self::Timeout => "device.timeout",
            Self::Link { .. } => "device.link",
            Self::NeedsFirmware { .. } => "device.needs_firmware",
            Self::ConsoleForbidden => "console.forbidden",
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::NotConnected { link } => Some(serde_json::json!({ "link": link })),
            Self::Link { reason } => Some(serde_json::json!({ "reason": reason })),
            Self::NeedsFirmware { field } => {
                Some(serde_json::json!({ "field": field, "version": null }))
            }
            Self::Common(e) => e.details(),
            _ => None,
        }
    }
}
impl From<Reach> for DeviceError {
    fn from(reach: Reach) -> Self {
        match reach {
            Reach::NotFound => Self::NotFound,
            Reach::NotConnected(link) => Self::NotConnected { link },
        }
    }
}
impl From<LinkError> for DeviceError {
    fn from(error: LinkError) -> Self {
        match error {
            LinkError::Timeout(_) => Self::Timeout,
            error => Self::Link {
                reason: error.to_string(),
            },
        }
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct DevicePath {
    pub device: DeviceId,
}
pub struct List;
impl Endpoint for List {
    const NAME: &'static str = "devices.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/devices";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<DeviceSummary>;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.devices.summaries())
    }
}
pub struct Get;
impl Endpoint for Get {
    const NAME: &'static str = "devices.get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/devices/{device}";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = DeviceSummary;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.devices
            .get(&req.path.device)
            .ok_or(DeviceError::NotFound)
    }
}
pub struct Connect;
impl Endpoint for Connect {
    const NAME: &'static str = "devices.connect";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/devices/{device}/connect";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = DeviceSummary;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.devices
            .connect(&req.path.device)
            .ok_or(DeviceError::NotFound)
    }
}
pub struct Disconnect;
impl Endpoint for Disconnect {
    const NAME: &'static str = "devices.disconnect";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/devices/{device}/disconnect";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = DeviceSummary;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.devices
            .disconnect(&req.path.device)
            .ok_or(DeviceError::NotFound)
    }
}
pub struct Forget;
impl Endpoint for Forget {
    const NAME: &'static str = "devices.forget";
    const METHOD: Method = Method::DELETE;
    const PATH: &'static str = "/devices/{device}";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let devices = ctx.devices.clone();
        let forgotten = tokio::task::spawn_blocking(move || devices.forget(&req.path.device))
            .await
            .map_err(|_| CommonError::Internal)?;
        forgotten.map_err(|e| match e {
            Forgotten::NotFound => DeviceError::NotFound,
            Forgotten::Connected => DeviceError::Connected,
        })
    }
}
pub struct Restart;
impl Endpoint for Restart {
    const NAME: &'static str = "devices.restart";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/devices/{device}/restart";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.devices.restart(&req.path.device).await??)
    }
}

/// The Device's settings. v2 fields are `null` on v1 Firmware.
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct DeviceSettings {
    pub idle_timeout_s: u32,
    /// `landscape` or `portrait`.
    pub orientation: String,
    pub name: Option<String>,
    /// A percentage, 0 to 100.
    pub brightness: Option<u8>,
}
#[derive(Debug, Default, Serialize, Deserialize, TS)]
pub struct DeviceSettingsPatch {
    #[serde(default)]
    pub idle_timeout_s: Option<u32>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// A percentage, 0 to 100.
    #[serde(default)]
    pub brightness: Option<u8>,
}
#[derive(Debug, Serialize, Deserialize, TS)]
pub struct SettingsPatched {
    pub settings: DeviceSettings,
    /// The Device must restart before an orientation change shows.
    pub restart_required: bool,
}
fn orientation_name(settings: &Settings) -> String {
    serde_json::to_value(settings.orientation)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
/// The v2 fields the Device's Firmware supports: its name, if it speaks protocol 2.
fn name_if_v2(summary: &DeviceSummary) -> Option<String> {
    let v2 = summary.firmware.as_ref().is_some_and(|f| f.protocol >= 2);
    v2.then(|| summary.name.clone()).flatten()
}
pub struct SettingsGet;
impl Endpoint for SettingsGet {
    const NAME: &'static str = "devices.settings_get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/devices/{device}/settings";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = DeviceSettings;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let id = req.path.device;
        let settings = ctx
            .devices
            .call(&id, |s| -> Result<Settings, LinkError> {
                s.request(&Request::SettingsGet, SETTINGS_TIMEOUT)?
                    .fields()
                    .map_err(LinkError::Reply)
            })
            .await??;
        let summary = ctx.devices.get(&id).ok_or(DeviceError::NotFound)?;
        Ok(DeviceSettings {
            idle_timeout_s: settings.idle_timeout_s,
            orientation: orientation_name(&settings),
            name: name_if_v2(&summary),
            brightness: None,
        })
    }
}
pub struct SettingsPatch;
impl Endpoint for SettingsPatch {
    const NAME: &'static str = "devices.settings_patch";
    const METHOD: Method = Method::PATCH;
    const PATH: &'static str = "/devices/{device}/settings";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = DeviceSettingsPatch;
    type Ok = SettingsPatched;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let id = req.path.device;
        let patch = req.body;
        let invalid = |reason| DeviceError::from(CommonError::invalid("body", reason));
        let orientation = match patch.orientation.as_deref() {
            None => None,
            Some("landscape") => Some(td_core::Orientation::Landscape),
            Some("portrait") => Some(td_core::Orientation::Portrait),
            Some(_) => return Err(invalid("orientation is landscape or portrait")),
        };
        if patch.idle_timeout_s.is_some_and(|t| {
            !(td_core::settings::IDLE_TIMEOUT_MIN_S..=td_core::settings::IDLE_TIMEOUT_MAX_S)
                .contains(&t)
        }) {
            return Err(invalid("idle_timeout_s is out of range"));
        }
        if patch
            .name
            .as_deref()
            .is_some_and(|n| !td_link::identity::valid_name(n))
        {
            return Err(invalid("name is empty, too long or has control characters"));
        }
        if patch.brightness.is_some_and(|b| b > BRIGHTNESS_MAX) {
            return Err(invalid("brightness is a percentage from 0 to 100"));
        }
        // No Firmware reports brightness yet.
        if patch.brightness.is_some() {
            return Err(DeviceError::NeedsFirmware {
                field: "brightness",
            });
        }
        let summary = ctx.devices.get(&id).ok_or(DeviceError::NotFound)?;
        if patch.name.is_some() && summary.firmware.as_ref().is_some_and(|f| f.protocol < 2) {
            return Err(DeviceError::NeedsFirmware { field: "name" });
        }
        let devices = ctx.devices.clone();
        let device = id.clone();
        let (settings, restart_required) = ctx
            .devices
            .call(&id, move |s| -> Result<(Settings, bool), LinkError> {
                let mut settings: Settings = s
                    .request(&Request::SettingsGet, SETTINGS_TIMEOUT)?
                    .fields()
                    .map_err(LinkError::Reply)?;
                let mut restart = false;
                if patch.idle_timeout_s.is_some() || orientation.is_some() {
                    settings.idle_timeout_s =
                        patch.idle_timeout_s.unwrap_or(settings.idle_timeout_s);
                    settings.orientation = orientation.unwrap_or(settings.orientation);
                    let reply = s.request(&Request::SettingsPut(settings), SETTINGS_TIMEOUT)?;
                    let fields = serde_json::Value::Object(reply.fields.clone());
                    restart = fields["reboot_required"].as_bool().unwrap_or(false);
                    settings = Settings::from_value(&fields);
                }
                if let Some(name) = patch.name {
                    let name = s.set_name(&name, SETTINGS_TIMEOUT)?;
                    devices.named(&device, name);
                }
                Ok((settings, restart))
            })
            .await??;
        let summary = ctx.devices.get(&id).ok_or(DeviceError::NotFound)?;
        Ok(SettingsPatched {
            settings: DeviceSettings {
                idle_timeout_s: settings.idle_timeout_s,
                orientation: orientation_name(&settings),
                name: name_if_v2(&summary),
                brightness: None,
            },
            restart_required,
        })
    }
}
fn devices() -> Feature {
    Feature::new("devices")
        .endpoint::<List>()
        .endpoint::<Get>()
        .endpoint::<Connect>()
        .endpoint::<Disconnect>()
        .endpoint::<Forget>()
        .endpoint::<Restart>()
        .endpoint::<SettingsGet>()
        .endpoint::<SettingsPatch>()
        .event::<DeviceAdded>()
        .event::<DeviceUpdated>()
        .event::<DeviceRemoved>()
        .event::<DeviceKeyMirror>()
        .event::<DeviceEvent>()
        .snapshot("devices", |ctx| ctx.devices.summaries())
}
inventory::submit! { Registration(devices) }

pub struct NewBoardsList;
impl Endpoint for NewBoardsList {
    const NAME: &'static str = "new_boards.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/new-boards";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<NewBoard>;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.devices.new_boards())
    }
}
fn new_boards() -> Feature {
    Feature::new("new_boards")
        .endpoint::<NewBoardsList>()
        .event::<NewBoardAdded>()
        .event::<NewBoardRemoved>()
        .snapshot("new_boards", |ctx| ctx.devices.new_boards())
}
inventory::submit! { Registration(new_boards) }

pub struct PortsList;
impl Endpoint for PortsList {
    const NAME: &'static str = "ports.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/ports";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<PortProbe>;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.devices.ports())
    }
}
pub struct PortsRescan;
impl Endpoint for PortsRescan {
    const NAME: &'static str = "ports.rescan";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/ports/rescan";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.devices.rescan();
        Ok(())
    }
}
fn ports() -> Feature {
    Feature::new("ports")
        .endpoint::<PortsList>()
        .endpoint::<PortsRescan>()
}
inventory::submit! { Registration(ports) }

#[derive(Serialize, Deserialize, TS)]
pub struct ConsoleLine {
    pub line: String,
}
pub struct ConsoleSend;
impl Endpoint for ConsoleSend {
    const NAME: &'static str = "console.send";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/devices/{device}/console";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ConsoleLine;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        if !console::allowed(&req.body.line) {
            return Err(DeviceError::ConsoleForbidden);
        }
        let line = req.body.line;
        Ok(ctx
            .devices
            .call(&req.path.device, move |s| s.send_line(&line))
            .await??)
    }
}
fn console_feature() -> Feature {
    Feature::new("console")
        .endpoint::<ConsoleSend>()
        .event::<DeviceConsole>()
}
inventory::submit! { Registration(console_feature) }
