use crate::{
    api::{ApiError, CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req},
    devices::{DeviceAdded, DeviceRemoved, DeviceSummary, DeviceUpdated},
};
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("The Device does not exist.")]
    NotFound,
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl ApiError for DeviceError {
    const CODES: &'static [&'static str] = &["device.not_found"];
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "device.not_found",
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::NotFound => None,
            Self::Common(e) => e.details(),
        }
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct DevicePath {
    pub device: String,
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
        let id = &req.path.device;
        let serial = id.strip_prefix("port-").unwrap_or(id);
        if serial.is_empty()
            || serial.len() > 32
            || !serial
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(CommonError::invalid("device", "Invalid Device id.").into());
        }
        ctx.devices
            .summaries()
            .into_iter()
            .find(|d| d.id == *id)
            .ok_or(DeviceError::NotFound)
    }
}
fn feature() -> Feature {
    Feature::new("devices")
        .endpoint::<List>()
        .endpoint::<Get>()
        .event::<DeviceAdded>()
        .event::<DeviceUpdated>()
        .event::<DeviceRemoved>()
        .snapshot("devices", |ctx| ctx.devices.summaries())
}
inventory::submit! { Registration(feature) }
