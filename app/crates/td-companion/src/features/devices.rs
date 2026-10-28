use crate::{
    api::{CommonError, Ctx, Endpoint, Feature, Registration, Req},
    devices::DeviceSummary,
};
use axum::http::Method;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
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
    type Query = ();
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
    type Query = ();
    type Body = ();
    type Ok = DeviceSummary;
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let id = &req.path.device;
        let serial = id.strip_prefix("port-").unwrap_or(id);
        if serial.is_empty()
            || serial.len() > 32
            || !serial
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            return Err(CommonError::invalid("device", "Invalid Device id."));
        }
        ctx.devices
            .summaries()
            .into_iter()
            .find(|d| d.id == *id)
            .ok_or_else(|| CommonError::new(404, "device.not_found", "The Device does not exist."))
    }
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceAdded {
    pub data: DeviceSummary,
}
impl crate::api::Event for DeviceAdded {
    const NAME: &'static str = "device.added";
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceUpdated {
    pub data: DeviceSummary,
}
impl crate::api::Event for DeviceUpdated {
    const NAME: &'static str = "device.updated";
}
#[derive(Serialize, Deserialize, TS)]
pub struct DeviceRemoved {
    pub id: String,
}
impl crate::api::Event for DeviceRemoved {
    const NAME: &'static str = "device.removed";
}
fn feature() -> Feature {
    Feature::new("devices")
        .endpoint::<List>()
        .endpoint::<Get>()
        .event::<DeviceAdded>()
        .event::<DeviceUpdated>()
        .event::<DeviceRemoved>()
        .snapshot("devices", |ctx| {
            serde_json::to_value(ctx.devices.summaries()).unwrap()
        })
}
inventory::submit! {Registration(feature)}
