use crate::{
    api::{CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req},
    jobs::{Job, JobError, JobUpdated},
};
use axum::http::Method;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS)]
pub struct JobPath {
    pub job: String,
}
pub struct List;
impl Endpoint for List {
    const NAME: &'static str = "jobs.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/jobs";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<Job>;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(ctx.jobs.list())
    }
}
pub struct Get;
impl Endpoint for Get {
    const NAME: &'static str = "jobs.get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/jobs/{job}";
    type Path = JobPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = Job;
    type Error = JobError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.jobs.get(&req.path.job).ok_or(JobError::NotFound)
    }
}
/// Asks the Job to stop and returns it. It reads `cancelled` once its work notices.
pub struct Cancel;
impl Endpoint for Cancel {
    const NAME: &'static str = "jobs.cancel";
    const METHOD: Method = Method::DELETE;
    const PATH: &'static str = "/jobs/{job}";
    type Path = JobPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = Job;
    type Error = JobError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.jobs.cancel(&req.path.job)
    }
}
fn feature() -> Feature {
    Feature::new("jobs")
        .endpoint::<List>()
        .endpoint::<Get>()
        .endpoint::<Cancel>()
        .event::<JobUpdated>()
        .snapshot("jobs", |ctx| ctx.jobs.list())
}
inventory::submit! { Registration(feature) }
