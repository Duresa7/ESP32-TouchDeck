mod auth;
mod error;
pub mod security;
mod socket;
use auth::Auth;
#[path = "../process.rs"]
mod process;
pub use process::run;
#[cfg(any(test, feature = "test-hooks"))]
#[path = "../testing.rs"]
pub mod testing;
pub use auth::{ClientInfo, SessionInfo};
use axum::{
    Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{MethodFilter, on},
};
pub use error::{ApiError, CommonError, ErrorBody};
use serde::{Serialize, de::DeserializeOwned};
pub use socket::{Event, Events};
use std::{
    future::Future,
    sync::{Arc, Mutex},
};
use ts_rs::TS;

#[derive(Clone)]
pub struct Ctx {
    pub devices: crate::devices::DeviceManager,
    pub events: Events,
    auth: Arc<Mutex<Auth>>,
    pub(crate) shutdown: tokio::sync::watch::Sender<bool>,
    pub(crate) port: u16,
    runtime: Option<std::path::PathBuf>,
    runtime_lock: Arc<Mutex<()>>,
    pub(crate) mock: bool,
    pub(crate) started: u64,
}
impl Ctx {
    fn new(port: u16, mock: bool, runtime: Option<std::path::PathBuf>) -> Self {
        let events = Events::default();
        Self {
            devices: crate::devices::DeviceManager::new(events.clone()),
            events,
            auth: Arc::new(Mutex::new(Auth::new())),
            shutdown: tokio::sync::watch::channel(false).0,
            port,
            runtime,
            runtime_lock: Arc::new(Mutex::new(())),
            mock,
            started: crate::now_ms(),
        }
    }
    fn pairing_token(&self) -> String {
        self.auth.lock().unwrap().pairing.clone()
    }
    fn runtime_value(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"api":["v1"],"port":self.port,"token":self.pairing_token(),"pid":std::process::id(),"build":env!("TD_BUILD"),"started_at_ms":self.started})
    }
}
pub trait RequestBody: DeserializeOwned + Serialize + TS + Send {}
impl<T: DeserializeOwned + Serialize + TS + Send> RequestBody for T {}
pub trait ResponseBody: DeserializeOwned + Serialize + TS + Send {}
impl<T: DeserializeOwned + Serialize + TS + Send> ResponseBody for T {}
pub trait Endpoint: Sized + Send + Sync + 'static {
    const NAME: &'static str;
    const METHOD: Method;
    const PATH: &'static str;
    const STATUS: StatusCode = StatusCode::OK;
    const BODY_LIMIT: usize = 1 << 20;
    type Path: DeserializeOwned + Serialize + TS + Send;
    type Query: DeserializeOwned + Serialize + TS + Send + Default;
    type Body: RequestBody;
    type Ok: ResponseBody;
    type Error: ApiError;
    fn handle(
        ctx: Ctx,
        req: Req<Self>,
    ) -> impl Future<Output = Result<Self::Ok, Self::Error>> + Send;
}
pub struct Req<E: Endpoint> {
    pub path: E::Path,
    pub query: E::Query,
    pub body: E::Body,
    pub client: ClientInfo,
}
pub type NoBody = ();
pub type Json<T> = T;
/// TypeScript type strings exist only where the generator and contract tests run, so the
/// release binary carries none of them.
#[cfg(any(test, feature = "test-hooks"))]
pub type EventType = String;
#[cfg(not(any(test, feature = "test-hooks")))]
pub type EventType = ();
pub struct EndpointInfo {
    pub name: &'static str,
    pub method: Method,
    pub path: &'static str,
    #[cfg(any(test, feature = "test-hooks"))]
    pub path_type: String,
    #[cfg(any(test, feature = "test-hooks"))]
    pub query_type: String,
    #[cfg(any(test, feature = "test-hooks"))]
    pub body_type: String,
    #[cfg(any(test, feature = "test-hooks"))]
    pub ok_type: String,
}
pub struct Snapshot {
    pub key: &'static str,
    pub read: fn(&Ctx) -> serde_json::Value,
}
pub struct Feature {
    pub snapshots: Vec<Snapshot>,
    /// Each event's name and, for the generated client, its TypeScript type.
    pub events: Vec<(&'static str, EventType)>,
    pub name: &'static str,
    pub endpoints: Vec<EndpointInfo>,
    pub(crate) router: Router<Ctx>,
}
impl Feature {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            endpoints: vec![],
            snapshots: vec![],
            events: vec![],
            router: Router::new(),
        }
    }
    pub fn event<E: Event + TS>(mut self) -> Self {
        #[cfg(any(test, feature = "test-hooks"))]
        let typ = E::inline();
        #[cfg(not(any(test, feature = "test-hooks")))]
        let typ = ();
        self.events.push((E::NAME, typ));
        self
    }
    pub fn snapshot(mut self, key: &'static str, read: fn(&Ctx) -> serde_json::Value) -> Self {
        self.snapshots.push(Snapshot { key, read });
        self
    }
    pub fn endpoint<E: Endpoint>(mut self) -> Self {
        self.endpoints.push(EndpointInfo {
            name: E::NAME,
            method: E::METHOD,
            path: E::PATH,
            #[cfg(any(test, feature = "test-hooks"))]
            path_type: E::Path::inline(),
            #[cfg(any(test, feature = "test-hooks"))]
            query_type: E::Query::inline(),
            #[cfg(any(test, feature = "test-hooks"))]
            body_type: E::Body::inline(),
            #[cfg(any(test, feature = "test-hooks"))]
            ok_type: E::Ok::name(),
        });
        self.router = self.router.route(
            &format!("/api/v1{}", E::PATH),
            on(MethodFilter::try_from(E::METHOD).unwrap(), dispatch::<E>),
        );
        self
    }
}
pub struct Registration(pub fn() -> Feature);
inventory::collect!(Registration);
pub fn registry() -> Vec<Feature> {
    let mut v: Vec<_> = inventory::iter::<Registration>
        .into_iter()
        .map(|r| (r.0)())
        .collect();
    v.sort_by_key(|f| f.name);
    v
}
async fn dispatch<E: Endpoint>(State(ctx): State<Ctx>, request: Request) -> Response {
    let result = async {
        let (mut parts, body) = request.into_parts();
        let client = parts
            .extensions
            .remove::<ClientInfo>()
            .ok_or_else(|| CommonError::new(401, "auth.missing", "A session is required."))?;
        let path = if !E::PATH.contains('{') {
            serde_json::from_str("null")
                .map_err(|e| CommonError::invalid("path", &e.to_string()))?
        } else {
            use axum::extract::FromRequestParts;
            Path::<E::Path>::from_request_parts(&mut parts, &ctx)
                .await
                .map_err(|e| CommonError::invalid("path", &e.to_string()))?
                .0
        };
        // A query type without fields, such as `()`, ignores the query string.
        let query = if size_of::<E::Query>() == 0 {
            E::Query::default()
        } else {
            serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
                .map_err(|e| CommonError::invalid("query", &e.to_string()))?
        };
        let bytes = to_bytes(body, E::BODY_LIMIT).await.map_err(|_| {
            CommonError::new(413, "request.too_large", "The request body is too large.")
        })?;
        if !bytes.is_empty() && !is_json(&parts.headers) {
            return Err(CommonError::new(
                415,
                "request.media_type",
                "The request requires JSON.",
            ));
        }
        let body = serde_json::from_slice(if bytes.is_empty() { b"null" } else { &bytes })
            .map_err(|e| CommonError::invalid("body", &e.to_string()))?;
        let value = E::handle(
            ctx,
            Req {
                path,
                query,
                body,
                client,
            },
        )
        .await
        .map_err(|e| CommonError {
            status: e.status(),
            body: e.body(),
        })?;
        Ok::<_, CommonError>(if E::STATUS == StatusCode::NO_CONTENT {
            E::STATUS.into_response()
        } else {
            (E::STATUS, axum::Json(value)).into_response()
        })
    }
    .await;
    result.unwrap_or_else(IntoResponse::into_response)
}
pub fn router(ctx: Ctx) -> Router {
    let mut router = Router::new();
    for feature in registry() {
        router = router.merge(feature.router);
    }
    router = router.route("/api/v1/events", axum::routing::get(socket::upgrade));
    router = auth::routes(router);
    router
        .fallback(|| async {
            CommonError::new(404, "route.not_found", "The route does not exist.")
        })
        .method_not_allowed_fallback(|| async {
            CommonError::new(
                405,
                "route.method_not_allowed",
                "The method is not allowed.",
            )
        })
        .layer(middleware::from_fn_with_state(ctx.clone(), guard))
        .with_state(ctx)
}
async fn guard(State(ctx): State<Ctx>, mut request: Request, next: Next) -> Response {
    let check = async {
        let h = request.headers();
        let header = |name: &str| h.get(name).map(|v| v.to_str().unwrap_or(""));
        security::admit(
            &security::RequestMeta {
                host: header("host"),
                origin: header("origin"),
                fetch_site: header("sec-fetch-site"),
                fetch_mode: header("sec-fetch-mode"),
                api: request.uri().path().starts_with("/api/"),
                get: request.method() == Method::GET,
            },
            ctx.port,
        )?;
        if matches!(
            *request.method(),
            Method::POST | Method::PUT | Method::PATCH
        ) && h.contains_key("content-type")
            && !is_json(h)
        {
            return Err(CommonError::new(
                415,
                "request.media_type",
                "The request requires JSON.",
            ));
        }
        let public = auth::is_public(request.method(), request.uri().path());
        if !public {
            let client = ctx
                .auth
                .lock()
                .unwrap()
                .authenticate(request.headers(), request.uri().path() == "/api/v1/events")?;
            request.extensions_mut().insert(client);
        }
        Ok::<_, CommonError>(())
    }
    .await;
    let mut response = match check {
        Ok(()) => next.run(request).await,
        Err(e) => e.into_response(),
    };
    for (k, v) in [
        ("x-frame-options", "DENY"),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("cache-control", "no-store"),
    ] {
        response
            .headers_mut()
            .insert(axum::http::HeaderName::from_static(k), v.parse().unwrap());
    }
    response.headers_mut().insert("content-security-policy",format!("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; connect-src 'self' ws://127.0.0.1:{} ws://localhost:{}; font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",ctx.port,ctx.port).parse().unwrap());
    response
}

/// Whether the request declares a JSON body. A body without this header is refused, so a
/// cross-site form post never reaches a handler.
pub(crate) fn is_json(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next().unwrap_or("").trim() == "application/json")
}
#[cfg(any(test, feature = "test-hooks"))]
pub(crate) fn auth_declarations() -> Vec<String> {
    auth::declarations()
}
