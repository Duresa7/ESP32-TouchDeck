use super::{CommonError, Ctx};
use axum::{
    Router,
    extract::{Path, Request, State},
    http::{HeaderMap, Method},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use subtle::ConstantTimeEq;
use ts_rs::TS;

#[derive(Clone, Serialize, Deserialize, TS)]
pub struct ClientInfo {
    pub id: String,
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct SessionInfo {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub created_ms: u64,
    pub last_used_ms: u64,
    pub current: bool,
}
struct Session {
    info: SessionInfo,
    revoked: bool,
    sockets: usize,
    closed_ms: u64,
}
pub(crate) struct Auth {
    pub pairing: String,
    pub(crate) offset_ms: u64,
    sessions: HashMap<Vec<u8>, Session>,
}
pub(crate) fn token(prefix: &str) -> String {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("OS random source unavailable");
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes))
}
fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
impl Auth {
    pub fn new() -> Self {
        Self {
            pairing: token("tdp_"),
            offset_ms: 0,
            sessions: HashMap::new(),
        }
    }
    pub fn authenticate(
        &mut self,
        headers: &HeaderMap,
        ws: bool,
    ) -> Result<ClientInfo, CommonError> {
        let token = if ws {
            headers
                .get("sec-websocket-protocol")
                .and_then(|h| h.to_str().ok())
                .and_then(|h| {
                    h.split(',')
                        .map(str::trim)
                        .find_map(|p| p.strip_prefix("touchdeck.bearer."))
                })
        } else {
            bearer(headers)
        };
        let token = token
            .ok_or_else(|| CommonError::new(401, "auth.missing", "A session token is required."))?;
        let session = self
            .sessions
            .get_mut(Sha256::digest(token.as_bytes()).as_slice())
            .ok_or_else(|| CommonError::new(401, "auth.invalid", "The session is invalid."))?;
        if session.revoked {
            return Err(CommonError::new(
                401,
                "auth.revoked",
                "The session was revoked.",
            ));
        }
        if session.sockets == 0
            && (crate::now_ms() + self.offset_ms).saturating_sub(session.closed_ms) >= 60_000
        {
            session.revoked = true;
            return Err(CommonError::new(
                401,
                "auth.revoked",
                "The window session ended.",
            ));
        }
        session.info.last_used_ms = crate::now_ms() + self.offset_ms;
        Ok(ClientInfo {
            id: session.info.id.clone(),
            kind: session.info.kind.clone(),
        })
    }
    pub fn active(&self, id: &str) -> bool {
        self.sessions
            .values()
            .any(|s| s.info.id == id && !s.revoked)
    }
    pub fn socket_open(&mut self, id: &str) -> Result<(), CommonError> {
        if self.sessions.values().map(|s| s.sockets).sum::<usize>() >= 16 {
            return Err(CommonError::new(
                429,
                "limits.exceeded",
                "The Companion has too many sockets.",
            ));
        }
        let s = self
            .sessions
            .values_mut()
            .find(|s| s.info.id == id && !s.revoked)
            .ok_or_else(|| CommonError::new(401, "auth.revoked", "The session was revoked."))?;
        if s.sockets >= 4 {
            return Err(CommonError::new(
                429,
                "limits.exceeded",
                "The session has too many sockets.",
            ));
        }
        s.sockets += 1;
        Ok(())
    }
    pub fn socket_close(&mut self, id: &str) {
        if let Some(s) = self.sessions.values_mut().find(|s| s.info.id == id) {
            s.sockets = s.sockets.saturating_sub(1);
            s.closed_ms = crate::now_ms() + self.offset_ms;
        }
    }
}
pub(crate) fn is_public(method: &Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("GET", "/api/v1/system/ping")
            | ("POST", "/api/v1/auth/proof")
            | ("POST", "/api/v1/auth/sessions")
    )
}
pub(crate) fn routes(router: Router<Ctx>) -> Router<Ctx> {
    router
        .route(
            "/api/v1/system/ping",
            get(|| async { axum::Json(serde_json::json!({"api":["v1"]})) }),
        )
        .route("/api/v1/auth/proof", post(proof))
        .route("/api/v1/auth/sessions", post(create).get(sessions))
        .route("/api/v1/auth/sessions/{id}", delete(revoke))
        .route("/api/v1/auth/reset", post(reset))
}
async fn json<T: serde::de::DeserializeOwned>(request: Request) -> Result<T, CommonError> {
    if !request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.split(';').next().unwrap_or("").trim() == "application/json")
    {
        return Err(CommonError::new(
            415,
            "request.media_type",
            "The request requires JSON.",
        ));
    }
    let bytes = axum::body::to_bytes(request.into_body(), 1 << 20)
        .await
        .map_err(|_| CommonError::new(413, "request.too_large", "The request is too large."))?;
    serde_json::from_slice(&bytes).map_err(|e| CommonError::invalid("body", &e.to_string()))
}
#[derive(Serialize, Deserialize, TS)]
pub struct ProofRequest {
    pub nonce: String,
}
#[derive(Serialize, Deserialize, TS)]
pub struct Proof {
    pub proof: String,
}
#[derive(Serialize, Deserialize, TS)]
pub struct SessionRequest {
    pub kind: String,
    pub label: Option<String>,
}
#[derive(Serialize, Deserialize, TS)]
pub struct SessionCreated {
    pub id: String,
    pub token: String,
}
async fn proof(State(ctx): State<Ctx>, request: Request) -> Result<axum::Json<Proof>, CommonError> {
    let body: ProofRequest = json(request).await?;
    let nonce = URL_SAFE_NO_PAD
        .decode(body.nonce)
        .map_err(|_| CommonError::invalid("nonce", "Expected base64url."))?;
    if nonce.len() != 32 {
        return Err(CommonError::invalid("nonce", "Expected 32 bytes."));
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(ctx.pairing_token().as_bytes()).unwrap();
    mac.update(b"touchdeck-companion-proof\0");
    mac.update(&nonce);
    Ok(axum::Json(Proof {
        proof: URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()),
    }))
}
async fn create(
    State(ctx): State<Ctx>,
    request: Request,
) -> Result<axum::Json<SessionCreated>, CommonError> {
    let supplied = bearer(request.headers()).unwrap_or("").to_owned();
    let body: SessionRequest = json(request).await?;
    let mut auth = ctx.auth.lock().unwrap();
    if !bool::from(auth.pairing.as_bytes().ct_eq(supplied.as_bytes())) {
        return Err(CommonError::new(
            401,
            "auth.invalid",
            "The pairing token is invalid.",
        ));
    }
    if body.kind != "window" {
        return Err(CommonError::new(
            403,
            "auth.forbidden",
            "Only window sessions can be created here.",
        ));
    }
    if auth.sessions.len() >= 1024 {
        return Err(CommonError::new(
            429,
            "limits.exceeded",
            "Too many sessions have been created. Restart the Companion.",
        ));
    }
    let id = token("session_");
    let token = token("tds_");
    let now = crate::now_ms() + auth.offset_ms;
    auth.sessions.insert(
        Sha256::digest(token.as_bytes()).to_vec(),
        Session {
            info: SessionInfo {
                id: id.clone(),
                kind: "window".into(),
                label: body.label.unwrap_or_else(|| "Window".into()),
                created_ms: now,
                last_used_ms: now,
                current: false,
            },
            revoked: false,
            sockets: 0,
            closed_ms: now,
        },
    );
    Ok(axum::Json(SessionCreated { id, token }))
}
async fn sessions(
    State(ctx): State<Ctx>,
    axum::Extension(client): axum::Extension<ClientInfo>,
) -> axum::Json<Vec<SessionInfo>> {
    axum::Json(
        ctx.auth
            .lock()
            .unwrap()
            .sessions
            .values()
            .filter(|s| !s.revoked)
            .map(|s| {
                let mut info = s.info.clone();
                info.current = info.id == client.id;
                info
            })
            .collect(),
    )
}
async fn revoke(
    State(ctx): State<Ctx>,
    Path(id): Path<String>,
    axum::Extension(client): axum::Extension<ClientInfo>,
) -> axum::http::StatusCode {
    let id = if id == "self" { client.id } else { id };
    if let Some(s) = ctx
        .auth
        .lock()
        .unwrap()
        .sessions
        .values_mut()
        .find(|s| s.info.id == id)
    {
        s.revoked = true;
    }
    ctx.events.notify();
    axum::http::StatusCode::NO_CONTENT
}
async fn reset(State(ctx): State<Ctx>) -> Response {
    let _runtime_lock = ctx.runtime_lock.lock().unwrap();
    {
        let mut auth = ctx.auth.lock().unwrap();
        auth.pairing = token("tdp_");
        for s in auth.sessions.values_mut() {
            s.revoked = true;
        }
    }
    ctx.events.notify();
    if let Some(path) = &ctx.runtime
        && super::process::write_runtime(path, &ctx.runtime_value()).is_err()
    {
        let _ = ctx.shutdown.send(true);
        return CommonError::new(
            503,
            "companion.stopping",
            "The runtime file could not be secured.",
        )
        .into_response();
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}

pub(super) fn declarations() -> Vec<String> {
    vec![
        ProofRequest::decl(),
        Proof::decl(),
        SessionRequest::decl(),
        SessionCreated::decl(),
    ]
}
