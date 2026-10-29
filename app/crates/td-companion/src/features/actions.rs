use crate::{
    actions::{ActionLog, AllowedAction, ApprovalRequest, ApprovalResolved},
    api::{ApiError, CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req},
    devices::DeviceId,
};
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, thiserror::Error)]
pub enum ActionError {
    #[error("The approval was answered or expired.")]
    ApprovalMissing,
    #[error("The allowed action does not exist.")]
    ActionMissing,
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl ApiError for ActionError {
    const CODES: &'static [&'static str] = &["approval.not_found", "action.not_found"];
    fn status(&self) -> StatusCode {
        match self {
            Self::Common(e) => e.status(),
            _ => StatusCode::NOT_FOUND,
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::ApprovalMissing => "approval.not_found",
            Self::ActionMissing => "action.not_found",
            Self::Common(e) => e.code(),
        }
    }
}
#[derive(Serialize, Deserialize, TS)]
pub struct ApprovalPath {
    pub id: String,
}
#[derive(Serialize, Deserialize, TS)]
pub struct AllowedPath {
    pub hash: String,
}
#[derive(Default, Serialize, Deserialize, TS)]
pub struct ActionQuery {
    pub device: Option<DeviceId>,
}
pub struct List;
impl Endpoint for List {
    const NAME: &'static str = "approvals.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/approvals";

    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<ApprovalRequest>;
    type Error = ActionError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.actions.pending())
    }
}
pub struct Show;
impl Endpoint for Show {
    const NAME: &'static str = "approvals.show";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/approvals/{id}/show";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ApprovalPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = ActionError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        tokio::task::spawn_blocking(move || ctx.actions.show(&req.path.id))
            .await
            .map_err(|_| CommonError::Internal)?
    }
}
pub struct Log;
impl Endpoint for Log {
    const NAME: &'static str = "actions.log";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/actions/log";

    type Path = ();
    type Query = ActionQuery;
    type Body = ();
    type Ok = Vec<ActionLog>;
    type Error = ActionError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.actions.log(req.query.device.as_ref()))
    }
}
pub struct Allowed;
impl Endpoint for Allowed {
    const NAME: &'static str = "actions.allowed";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/actions/allowed";

    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<AllowedAction>;
    type Error = ActionError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.actions.allowed())
    }
}
pub struct AllowedRemove;
impl Endpoint for AllowedRemove {
    const NAME: &'static str = "actions.allowed_remove";
    const METHOD: Method = Method::DELETE;
    const PATH: &'static str = "/actions/allowed/{hash}";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = AllowedPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = ActionError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let found = tokio::task::spawn_blocking(move || ctx.actions.remove(&req.path.hash))
            .await
            .map_err(|_| CommonError::Internal)?
            .map_err(|_| CommonError::Internal)?;
        if found {
            Ok(())
        } else {
            Err(ActionError::ActionMissing)
        }
    }
}
fn actions() -> Feature {
    Feature::new("actions")
        .endpoint::<Log>()
        .endpoint::<Allowed>()
        .endpoint::<AllowedRemove>()
        .event::<ActionLog>()
}
fn approvals() -> Feature {
    Feature::new("approvals")
        .endpoint::<List>()
        .endpoint::<Show>()
        .event::<ApprovalRequest>()
        .event::<ApprovalResolved>()
        .snapshot("approvals", |ctx| ctx.actions.pending())
}
inventory::submit! { Registration(actions) }
inventory::submit! { Registration(approvals) }
