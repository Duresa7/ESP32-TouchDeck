use crate::api::{CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req, SessionKind};
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Serialize, Deserialize, TS)]
pub struct SystemInfo {
    pub version: String,
    pub build: String,
    pub api: String,
    pub mock: bool,
    pub data_dir_override: bool,
}
pub struct Get;
impl Endpoint for Get {
    const NAME: &'static str = "system.get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/system";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = SystemInfo;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(SystemInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            build: env!("TD_BUILD").into(),
            api: "v1".into(),
            mock: ctx.mock,
            data_dir_override: std::env::var_os("TOUCHDECK_APP_DATA_DIR").is_some(),
        })
    }
}
pub struct Shutdown;
impl Endpoint for Shutdown {
    const NAME: &'static str = "system.shutdown";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/system/shutdown";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        if req.client().kind != SessionKind::Window {
            return Err(CommonError::Forbidden("A window session is required."));
        }
        let _ = ctx.shutdown.send(true);
        Ok(())
    }
}
fn feature() -> Feature {
    Feature::new("system")
        .endpoint::<Get>()
        .endpoint::<Shutdown>()
}
inventory::submit! {Registration(feature)}
