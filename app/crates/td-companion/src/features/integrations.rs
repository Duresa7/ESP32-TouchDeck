use crate::api::{
    ApiError, CommonError, Ctx, Endpoint, Event, Events, Feature, NoQuery, Registration, Req,
};
use crate::live::StreamKey;
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use td_host::integrations::secrets::{MemorySecrets, OsSecrets};
use td_host::integrations::{Control, Delivery, Environment, Error, LiveSample, Registry};
pub use td_host::integrations::{IntegrationSummary, IntegrationView};
use ts_rs::TS;

/// The Integrations' Controls by Integration id, held outside the registry lock so a slow poll never holds them up.
pub type Controls = Arc<BTreeMap<String, Arc<dyn Control>>>;

#[derive(Clone)]
pub struct Service {
    registry: Arc<Mutex<Result<Registry, Error>>>,
    summaries: Arc<Mutex<Vec<IntegrationSummary>>>,
    pending: Arc<Pending>,
    events: Events,
    pub(crate) controls: Controls,
}
type Pending = Mutex<HashMap<(String, String), Delivery>>;
impl Service {
    pub fn command_approval(&self, approval: Arc<dyn td_host::integrations::CommandApproval>) {
        if let Ok(registry) = self.registry.lock().unwrap().as_mut()
            && let Ok(custom) = registry.get("custom")
        {
            custom.command_approval(approval);
        }
    }
    pub fn new(mock: bool, events: Events) -> Self {
        let registry = Registry::new(Environment {
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
        });
        let controls = Arc::new(
            registry
                .as_ref()
                .map(Registry::controls)
                .unwrap_or_default(),
        );
        let service = Self {
            registry: Arc::new(Mutex::new(registry)),
            summaries: Arc::default(),
            pending: Arc::default(),
            events: events.clone(),
            controls,
        };
        if let Ok(registry) = service.registry.lock().unwrap().as_ref() {
            *service.summaries.lock().unwrap() = registry.list();
        }
        let cache = service.summaries.clone();
        let pending = service.pending.clone();
        let weak = Arc::downgrade(&service.registry);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                let Some(state) = weak.upgrade() else { break };
                let mut state = state.lock().unwrap();
                if let Ok(registry) = state.as_mut() {
                    let before = serde_json::to_value(registry.list()).unwrap();
                    apply_pending(registry, &pending);
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
        self.summaries.lock().unwrap().clone()
    }
    /// The newest samples of the wanted streams that Integrations poll. None while an Integration is busy, such as
    /// during a poll.
    pub fn live(&self, wanted: &BTreeSet<StreamKey>) -> Option<HashMap<StreamKey, LiveSample>> {
        let state = self.registry.try_lock().ok()?;
        let Ok(registry) = state.as_ref() else {
            return Some(HashMap::new());
        };
        Some(
            registry
                .live()
                .filter(|(id, _)| {
                    !self.controls.contains_key(*id)
                        && wanted.iter().any(|key| key.integration == *id)
                })
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
    /// Whether a Device's input can set this Integration stream: its Integration lists it as `settable`. Reads the
    /// cached summaries, never the registry.
    pub fn settable(&self, key: &StreamKey) -> bool {
        self.summaries.lock().unwrap().iter().any(|summary| {
            summary.id == key.integration
                && summary
                    .streams
                    .iter()
                    .any(|stream| stream.id == key.stream && stream.settable)
        })
    }
    /// The newest samples of the wanted streams that come from Controls. Never waits for the registry.
    pub fn control_live(&self, wanted: &BTreeSet<StreamKey>) -> HashMap<StreamKey, LiveSample> {
        self.controls
            .iter()
            .filter(|(id, _)| wanted.iter().any(|k| &k.integration == *id))
            .flat_map(|(id, control)| {
                control.live().into_iter().map(|sample| {
                    (
                        StreamKey {
                            integration: id.clone(),
                            account: sample.account.clone(),
                            stream: sample.stream.clone(),
                        },
                        sample,
                    )
                })
            })
            .filter(|(key, _)| wanted.contains(key))
            .collect()
    }
    /// Records what the Companion delivered to a Device for an Integration. One that comes while the registry is busy
    /// is applied by the next caller that takes the registry.
    pub fn delivered(&self, integration: &str, delivery: Delivery) {
        self.pending
            .lock()
            .unwrap()
            .insert((integration.to_owned(), delivery.device.clone()), delivery);
        if let Ok(mut state) = self.registry.try_lock()
            && let Ok(registry) = state.as_mut()
        {
            let before = serde_json::to_value(registry.list()).unwrap();
            apply_pending(registry, &self.pending);
            *self.summaries.lock().unwrap() = registry.list();
            if before != serde_json::to_value(registry.list()).unwrap() {
                publish(registry, &self.events);
            }
        }
    }
    fn with_registry<T>(
        &self,
        events: &Events,
        f: impl FnOnce(&mut Registry) -> Result<T, Error>,
    ) -> Result<T, IntegrationError> {
        let mut guard = self.registry.lock().unwrap();
        let registry = guard.as_mut().map_err(|_| IntegrationError::Credentials)?;
        let before = serde_json::to_value(registry.list()).unwrap();
        apply_pending(registry, &self.pending);
        let result = f(registry).map_err(IntegrationError::from);
        *self.summaries.lock().unwrap() = registry.list();
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
        let busy = service.registry.lock().unwrap();
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

#[cfg(test)]
mod control_tests {
    use super::*;
    use crate::devices::{BoardKind, Config, DeviceManager, LinkState};
    use crate::link::Links;
    use crate::live::Live;
    use crate::live::fake::FakeControl;
    use std::time::{Duration, Instant};
    use td_core::live::LiveValue;

    struct Running {
        service: Service,
        fake: Arc<FakeControl>,
        deck: td_link::MockDeck,
        devices: DeviceManager,
        live: Option<Live>,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.live.take();
            self.devices.stop();
        }
    }

    fn wait(what: &str, within: Duration, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + within;
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn running(integration: &str, layout: serde_json::Value) -> Running {
        let events = Events::default();
        let devices = DeviceManager::new(events.clone());
        let config = Config::mock(None);
        let ports = config.mock.as_ref().unwrap().0.clone();
        ports.plug("mock0001", BoardKind::Device);
        let deck = ports.deck("mock0001").unwrap();
        deck.set_media_capabilities(true);
        devices.start(config);
        wait("the Device connects", Duration::from_secs(5), || {
            devices
                .summaries()
                .iter()
                .any(|d| matches!(d.link, LinkState::Connected { .. }))
        });
        let links = Links::new(devices.clone());
        let layout = td_core::Layout::parse(&serde_json::to_vec(&layout).unwrap()).unwrap();
        links
            .with("mock0001", |s| {
                td_link::xfer::put_layout(s, layout.orientation, &layout)
            })
            .unwrap()
            .unwrap();
        let mut service = Service::new(true, events);
        let fake = Arc::new(FakeControl::default());
        service.controls = Arc::new(BTreeMap::from([(
            integration.to_owned(),
            fake.clone() as Arc<dyn Control>,
        )]));
        let live = Live::start(devices.clone(), links, service.clone());
        Running {
            service,
            fake,
            deck,
            devices,
            live: Some(live),
        }
    }

    fn graph() -> serde_json::Value {
        let mut layout: serde_json::Value =
            serde_json::from_slice(include_bytes!("../../../../../schema/examples/graph.json"))
                .unwrap();
        layout["pages"][0]["tiles"][0]["action"] =
            serde_json::json!({"type":"widget","source":"test","account":"","field":"level"});
        layout
    }

    fn shown(deck: &td_link::MockDeck) -> Option<serde_json::Value> {
        deck.live_sets().last().map(|set| set["value"].clone())
    }

    #[test]
    fn only_a_stream_its_integration_lists_as_settable_takes_input() {
        let service = Service::new(true, Events::default());
        let key = |integration: &str, stream: &str| StreamKey {
            integration: integration.into(),
            account: String::new(),
            stream: stream.into(),
        };
        let busy = service.registry.lock().unwrap();
        let mut summaries = service.summaries.lock().unwrap();
        let mut volume = summaries[0].clone();
        volume.id = "volume".into();
        volume.streams = vec![
            td_host::integrations::DataStream {
                id: "level".into(),
                kind: "number".into(),
                unit: None,
                settable: true,
            },
            td_host::integrations::DataStream {
                id: "muted".into(),
                kind: "number".into(),
                unit: None,
                settable: false,
            },
        ];
        summaries.push(volume);
        drop(summaries);
        assert!(service.settable(&key("volume", "level")));
        assert!(!service.settable(&key("volume", "muted")));
        assert!(!service.settable(&key("volume", "other")));
        assert!(!service.settable(&key("media", "level")));
        assert!(!service.settable(&key("this_pc", "cpu")));
        drop(busy);
    }

    #[test]
    fn control_samples_are_read_while_the_registry_is_busy() {
        let mut service = Service::new(true, Events::default());
        let fake = Arc::new(FakeControl::default());
        service.controls = Arc::new(BTreeMap::from([(
            "test".to_owned(),
            fake.clone() as Arc<dyn Control>,
        )]));
        let key = StreamKey {
            integration: "test".into(),
            account: String::new(),
            stream: "level".into(),
        };
        let busy = service.registry.lock().unwrap();
        fake.number(0.4);
        assert_eq!(service.live(&BTreeSet::from([key.clone()])), None);
        assert_eq!(
            service.control_live(&BTreeSet::from([key.clone()]))[&key].value,
            LiveValue::Number(0.4)
        );
        drop(busy);
    }

    #[test]
    fn a_woken_sample_reaches_the_device_within_50_ms_while_a_poll_holds_the_registry_for_5_s() {
        let run = running("test", graph());
        run.fake.number(0.1);
        wait("the first value arrives", Duration::from_secs(5), || {
            shown(&run.deck) == Some(serde_json::json!(0.1))
        });
        let (held, holding) = std::sync::mpsc::channel();
        let service = run.service.clone();
        let poll = std::thread::spawn(move || {
            let _registry = service.registry.lock().unwrap();
            held.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(5));
        });
        holding.recv().unwrap();
        let polling = Instant::now();
        for value in [0.7, 0.3, 0.9] {
            std::thread::sleep(Duration::from_millis(600));
            let woken = Instant::now();
            run.fake.number(value);
            wait(
                "the woken value arrives within 50 ms",
                Duration::from_millis(50),
                || shown(&run.deck) == Some(serde_json::json!(value)),
            );
            assert!(woken.elapsed() < Duration::from_millis(50));
        }
        assert!(
            polling.elapsed() < Duration::from_secs(5),
            "the poll still holds the registry"
        );
        poll.join().unwrap();
    }

    #[test]
    fn a_track_reaches_the_device_with_its_cover_made_off_the_link() {
        let run = running(
            "media",
            serde_json::json!({
                "schema": 2, "name": "Media", "orientation": "landscape",
                "pages": [{"kind": "grid", "id": "main", "title": "Media", "rows": 1, "cols": 1, "tiles": [{
                    "id": "player", "icon": null, "label": "Player", "row": 0, "col": 0, "rowSpan": 1,
                    "colSpan": 1, "render": "now-playing",
                    "streams": {"title": "text", "artist": "text", "cover": "image", "state": "number"},
                    "action": {"type": "widget", "source": "media", "account": "", "field": "title"}
                }]}]
            }),
        );
        run.fake.add_picture(
            "sha256:art",
            crate::photos::encode::tests::noise_png(640, 480, 5),
        );
        run.fake.publish(&[
            ("title", LiveValue::Text("Ło\u{301}dź".into())),
            ("artist", LiveValue::Text("Artist".into())),
            ("state", LiveValue::Number(2.0)),
            ("cover", LiveValue::Image(Some("sha256:art".into()))),
        ]);
        let value = |stream: &str| run.deck.live("main", "player", stream).map(|l| l.value);
        wait("the cover arrives", Duration::from_secs(5), || {
            value("cover").is_some_and(|v| v.is_string())
        });
        assert_eq!(value("title"), Some(serde_json::json!("Lódz")));
        let cover = value("cover").unwrap();
        let id = cover.as_str().unwrap().strip_prefix("sha256:").unwrap();
        let picture = image::load_from_memory(&run.deck.assets()[id]).unwrap();
        assert_eq!((picture.width(), picture.height()), (256, 256));
    }
}
