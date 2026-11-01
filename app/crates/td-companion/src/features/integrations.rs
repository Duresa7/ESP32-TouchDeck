use crate::api::{
    ApiError, CommonError, Ctx, Endpoint, Event, Events, Feature, NoQuery, Registration, Req,
};
use crate::live::StreamKey;
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use td_host::integrations::secrets::{MemorySecrets, OsSecrets};
use td_host::integrations::{Delivery, Environment, Error, LiveSample, Registry};
pub use td_host::integrations::{IntegrationSummary, IntegrationView};
use ts_rs::TS;

#[derive(Clone)]
pub struct Service(
    Arc<Mutex<Result<Registry, Error>>>,
    Arc<Mutex<Vec<IntegrationSummary>>>,
    Arc<Pending>,
);
type Pending = Mutex<HashMap<(String, String), Delivery>>;
impl Service {
    pub fn command_approval(&self, approval: Arc<dyn td_host::integrations::CommandApproval>) {
        if let Ok(registry) = self.0.lock().unwrap().as_mut()
            && let Ok(custom) = registry.get("custom")
        {
            custom.command_approval(approval);
        }
    }
    pub fn new(mock: bool, events: Events) -> Self {
        let service = Self(
            Arc::new(Mutex::new(Registry::new(Environment {
                secrets: if mock {
                    Arc::new(MemorySecrets::default())
                } else {
                    Arc::new(OsSecrets)
                },
                config_file: (!mock).then(|| td_core::app_dirs::Dirs::current().sources_config()),
                fetch: if mock {
                    Arc::new(|| Box::new(FakeFetch))
                } else {
                    Arc::new(|| Box::new(td_host::sources::HttpFetch::new()))
                },
            }))),
            Arc::new(Mutex::new(Vec::new())),
            Arc::default(),
        );
        if let Ok(registry) = service.0.lock().unwrap().as_ref() {
            *service.1.lock().unwrap() = registry.list();
        }
        let cache = service.1.clone();
        let pending = service.2.clone();
        let weak = Arc::downgrade(&service.0);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let Some(state) = weak.upgrade() else { break };
                let mut state = state.lock().unwrap();
                if let Ok(registry) = state.as_mut() {
                    apply_pending(registry, &pending);
                    let before = serde_json::to_value(registry.list()).unwrap();
                    registry.tick();
                    *cache.lock().unwrap() = registry.list();
                    if before != serde_json::to_value(registry.list()).unwrap() {
                        publish(registry, &events);
                    }
                }
            }
        });
        service
    }
    pub fn summaries(&self) -> Vec<IntegrationSummary> {
        self.1.lock().unwrap().clone()
    }
    /// The newest samples of the wanted streams. None while an Integration is busy, such as during a poll.
    pub fn live(&self, wanted: &BTreeSet<StreamKey>) -> Option<HashMap<StreamKey, LiveSample>> {
        let state = self.0.try_lock().ok()?;
        let Ok(registry) = state.as_ref() else {
            return Some(HashMap::new());
        };
        Some(
            registry
                .live()
                .filter(|(id, _)| wanted.iter().any(|key| key.integration == *id))
                .flat_map(|(id, samples)| {
                    samples.into_iter().map(move |sample| {
                        let key = StreamKey {
                            integration: id.to_owned(),
                            account: sample.account.clone(),
                            stream: sample.stream.clone(),
                        };
                        (key, sample)
                    })
                })
                .filter(|(key, _)| wanted.contains(key))
                .collect(),
        )
    }
    /// Records what the Companion delivered to a Device for an Integration. One that comes while the registry is busy
    /// is applied by the next caller that takes the registry.
    pub fn delivered(&self, integration: &str, delivery: Delivery) {
        self.2
            .lock()
            .unwrap()
            .insert((integration.to_owned(), delivery.device.clone()), delivery);
        if let Ok(mut state) = self.0.try_lock()
            && let Ok(registry) = state.as_mut()
        {
            apply_pending(registry, &self.2);
        }
    }
    fn with_registry<T>(
        &self,
        events: &Events,
        f: impl FnOnce(&mut Registry) -> Result<T, Error>,
    ) -> Result<T, IntegrationError> {
        let mut guard = self.0.lock().unwrap();
        let registry = guard.as_mut().map_err(|_| IntegrationError::Credentials)?;
        apply_pending(registry, &self.2);
        let before = serde_json::to_value(registry.list()).unwrap();
        let result = f(registry).map_err(IntegrationError::from);
        *self.1.lock().unwrap() = registry.list();
        if before != serde_json::to_value(registry.list()).unwrap() {
            publish(registry, events);
        }
        result
    }
}
fn apply_pending(registry: &mut Registry, pending: &Pending) {
    for ((integration, _), delivery) in pending.lock().unwrap().drain() {
        if let Ok(integration) = registry.get(&integration) {
            integration.delivered(delivery);
        }
    }
}
struct FakeFetch;
impl td_host::sources::Fetch for FakeFetch {
    fn get(&self, url: &str, bearer: &str) -> Result<String, String> {
        #[cfg(feature = "test-hooks")]
        if loopback_stub(url) {
            return td_host::sources::HttpFetch::new().get(url, bearer);
        }
        let _ = (url, bearer);
        Ok("[]".into())
    }
    fn post_json(&self, url: &str, bearer: &str, body: &Value) -> Result<String, String> {
        #[cfg(feature = "test-hooks")]
        if loopback_stub(url) {
            return td_host::sources::HttpFetch::new().post_json(url, bearer, body);
        }
        let _ = (url, bearer, body);
        Err("No fixture".into())
    }
}
#[cfg(feature = "test-hooks")]
fn loopback_stub(url: &str) -> bool {
    url.strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(port, _)| port.parse::<u16>().is_ok_and(|port| port > 0))
}
#[derive(Serialize, Deserialize, TS)]
pub struct IntegrationStatusEvent(pub IntegrationSummary);
impl Event for IntegrationStatusEvent {
    const NAME: &'static str = "integration.status";
}
fn publish(registry: &Registry, events: &Events) {
    for summary in registry.list() {
        events.publish(&IntegrationStatusEvent(summary));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IntegrationError {
    #[error("The Integration does not exist.")]
    NotFound,
    #[error("Invalid Integration config.")]
    ConfigInvalid,
    #[error("Invalid Integration config.")]
    ConfigField {
        field: &'static str,
        message: &'static str,
    },
    #[error("The credential store is unavailable.")]
    Credentials,
    #[error(transparent)]
    Common(#[from] CommonError),
}
impl From<Error> for IntegrationError {
    fn from(e: Error) -> Self {
        match e {
            Error::NotFound => Self::NotFound,
            Error::ConfigInvalid => Self::ConfigInvalid,
            Error::ConfigField { field, message } => Self::ConfigField { field, message },
            Error::Credentials => Self::Credentials,
            Error::Storage => Self::Common(CommonError::Internal),
        }
    }
}
impl ApiError for IntegrationError {
    const CODES: &'static [&'static str] = &[
        "integration.not_found",
        "integration.config_invalid",
        "integration.credentials",
    ];
    fn status(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::ConfigInvalid | Self::ConfigField { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Credentials => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Common(e) => e.status(),
        }
    }
    fn code(&self) -> &'static str {
        match self {
            Self::NotFound => Self::CODES[0],
            Self::ConfigInvalid | Self::ConfigField { .. } => Self::CODES[1],
            Self::Credentials => Self::CODES[2],
            Self::Common(e) => e.code(),
        }
    }
    fn details(&self) -> Option<Value> {
        match self {
            Self::ConfigInvalid => {
                Some(serde_json::json!({"issues":[{"message":"Check the Integration fields."}]}))
            }
            Self::ConfigField { field, message } => {
                Some(serde_json::json!({"issues":[{"path":field,"message":message}]}))
            }
            Self::Common(e) => e.details(),
            _ => None,
        }
    }
}
async fn work<T: Send + 'static>(
    ctx: Ctx,
    f: impl FnOnce(&mut Registry) -> Result<T, Error> + Send + 'static,
) -> Result<T, IntegrationError> {
    tokio::task::spawn_blocking(move || ctx.integrations.with_registry(&ctx.events, f))
        .await
        .map_err(|_| IntegrationError::Credentials)?
}
#[derive(Serialize, Deserialize, TS)]
pub struct IntegrationPath {
    pub integration: String,
}
#[derive(Serialize, Deserialize, TS)]
pub struct SecretPath {
    pub integration: String,
    pub field: String,
}
#[derive(Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SecretInput {
    pub value: String,
}
#[derive(Serialize, Deserialize, TS)]
pub struct SecretHint {
    pub hint: String,
}

pub struct List;
impl Endpoint for List {
    const NAME: &'static str = "integrations.list";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/integrations";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<IntegrationSummary>;
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, |r| Ok(r.list())).await
    }
}
pub struct Get;
impl Endpoint for Get {
    const NAME: &'static str = "integrations.get";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/integrations/{integration}";
    type Path = IntegrationPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = IntegrationView;
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, move |r| r.get(&req.path.integration)?.view()).await
    }
}
pub struct ConfigPut;
impl Endpoint for ConfigPut {
    const NAME: &'static str = "integrations.config_put";
    const METHOD: Method = Method::PUT;
    const PATH: &'static str = "/integrations/{integration}/config";
    type Path = IntegrationPath;
    type Query = NoQuery;
    type Body = Value;
    type Ok = IntegrationView;
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, move |r| {
            r.get(&req.path.integration)?.configure(req.body)
        })
        .await
    }
}
pub struct SecretPut;
impl Endpoint for SecretPut {
    const NAME: &'static str = "integrations.secret_put";
    const METHOD: Method = Method::PUT;
    const PATH: &'static str = "/integrations/{integration}/secrets/{field}";
    type Path = SecretPath;
    type Query = NoQuery;
    type Body = SecretInput;
    type Ok = SecretHint;
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, move |r| {
            r.get(&req.path.integration)?
                .secret_put(&req.path.field, &req.body.value)?;
            Ok(SecretHint { hint: "Set".into() })
        })
        .await
    }
}
pub struct SecretDelete;
impl Endpoint for SecretDelete {
    const NAME: &'static str = "integrations.secret_delete";
    const METHOD: Method = Method::DELETE;
    const PATH: &'static str = "/integrations/{integration}/secrets/{field}";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = SecretPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, move |r| {
            r.get(&req.path.integration)?.secret_delete(&req.path.field)
        })
        .await
    }
}
pub struct Refresh;
impl Endpoint for Refresh {
    const NAME: &'static str = "integrations.refresh";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/integrations/{integration}/refresh";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = IntegrationPath;
    type Query = NoQuery;
    type Body = ();
    type Ok = ();
    type Error = IntegrationError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        work(ctx, move |r| {
            r.get(&req.path.integration)?.refresh();
            Ok(())
        })
        .await
    }
}
fn feature() -> Feature {
    Feature::new("integrations")
        .endpoint::<List>()
        .endpoint::<Get>()
        .endpoint::<ConfigPut>()
        .endpoint::<SecretPut>()
        .endpoint::<SecretDelete>()
        .endpoint::<Refresh>()
        .event::<IntegrationStatusEvent>()
        .snapshot("integrations", |ctx| ctx.integrations.summaries())
}
inventory::submit! { Registration(feature) }

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(service: &Service, events: &Events) -> Vec<(String, u64)> {
        let list = service.with_registry(events, |r| Ok(r.list())).unwrap();
        let this_pc = list.into_iter().find(|i| i.id == "this_pc").unwrap();
        this_pc
            .status
            .delivery
            .into_iter()
            .map(|d| (d.device, d.sent))
            .collect()
    }

    #[test]
    fn a_delivery_reported_while_the_integrations_are_busy_still_shows() {
        let events = Events::default();
        let service = Service::new(true, events.clone());
        let busy = service.0.lock().unwrap();
        for sent in 3..=4 {
            service.delivered(
                "this_pc",
                Delivery {
                    device: "mock0001".into(),
                    sent,
                    error: None,
                },
            );
        }
        drop(busy);

        assert_eq!(sent(&service, &events), [("mock0001".to_owned(), 4)]);
    }
}
