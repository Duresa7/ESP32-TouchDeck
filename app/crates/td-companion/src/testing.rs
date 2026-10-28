use crate::{
    api::{Ctx, Endpoint},
    devices::{MockPorts, PortSource},
};
use futures_util::StreamExt;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::client::IntoClientRequest};
pub struct TestCompanion {
    ctx: Ctx,
    ports: MockPorts,
    task: tokio::task::JoinHandle<()>,
    pub token: String,
    pub pairing_token: String,
    pub url: String,
    client: reqwest::Client,
}
impl TestCompanion {
    pub async fn start() -> Self {
        Self::start_with(crate::Limits::default()).await
    }
    /// Starts with other connection and time limits, so limit tests run in milliseconds.
    pub async fn start_with(limits: crate::Limits) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let ctx = Ctx::new(port, true, None, limits);
        let router = crate::api::router(ctx.clone());
        let stop = ctx.shutdown.subscribe();
        let task = tokio::spawn(crate::serve::serve(listener, router, limits, stop));
        let url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        let pairing_token = ctx.pairing_token();
        let session: serde_json::Value = client
            .post(format!("{url}/api/v1/auth/sessions"))
            .bearer_auth(&pairing_token)
            .json(&serde_json::json!({"kind":"window"}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        Self {
            ctx,
            ports: MockPorts::default(),
            task,
            token: session["token"].as_str().unwrap().into(),
            pairing_token,
            url,
            client,
        }
    }
    pub async fn plug_device(&self, id: &str) -> String {
        self.ports.plug(id);
        self.refresh().await;
        id.into()
    }
    pub async fn unplug(&self, id: &str) {
        self.ports.unplug(id);
        self.refresh().await;
    }
    async fn refresh(&self) {
        let ports = self.ports.clone();
        let snapshot = tokio::task::spawn_blocking(move || ports.scan())
            .await
            .unwrap();
        self.ctx.devices.refresh(snapshot);
    }
    /// How many requests reached a handler. A rejected request leaves it unchanged.
    pub fn handler_hits(&self) -> usize {
        self.ctx
            .handler_hits
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Asks the Companion to stop and reports whether its server finished within `within`.
    pub async fn stop(&mut self, within: std::time::Duration) -> bool {
        let _ = self.ctx.shutdown.send(true);
        tokio::time::timeout(within, &mut self.task).await.is_ok()
    }
    /// Runs one 60 s maintenance step now.
    pub async fn maintain(&self) {
        assert!(crate::process::maintain(&self.ctx).await);
    }
    /// Whether anything asked the Companion to stop.
    pub fn shutdown_requested(&self) -> bool {
        *self.ctx.shutdown.borrow()
    }
    pub fn clock(&self) -> TestClock {
        TestClock(self.ctx.clone())
    }
    pub fn flood_state(&self, frames: usize, bytes: usize) {
        for _ in 0..frames {
            self.ctx.events.publish_frame(
                serde_json::json!({"type": "device.updated", "data": "x".repeat(bytes)}),
            );
        }
    }
    pub fn raw(&self) -> RawClient {
        RawClient {
            client: self.client.clone(),
            url: self.url.clone(),
        }
    }
    pub async fn call<E: Endpoint>(
        &self,
        path: E::Path,
        query: E::Query,
        body: E::Body,
    ) -> Result<E::Ok, reqwest::Error> {
        let mut route = E::PATH.to_owned();
        if let Some(fields) = serde_json::to_value(path).unwrap().as_object() {
            for (k, v) in fields {
                route = route.replace(&format!("{{{k}}}"), v.as_str().unwrap());
            }
        }
        let mut req = self
            .client
            .request(E::METHOD, format!("{}/api/v1{route}", self.url))
            .bearer_auth(&self.token);
        if !serde_json::to_value(&query).unwrap().is_null() {
            req = req.query(&query);
        }
        if !serde_json::to_value(&body).unwrap().is_null() {
            req = req.json(&body);
        }
        let response = req.send().await?.error_for_status()?;
        if response.status() == 204 {
            return Ok(serde_json::from_value(serde_json::Value::Null).unwrap());
        }
        response.json().await
    }
    pub async fn events(&self) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
        let mut request = format!("{}/api/v1/events", self.url.replace("http:", "ws:"))
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "sec-websocket-protocol",
            format!("touchdeck.v1, touchdeck.bearer.{}", self.token)
                .parse()
                .unwrap(),
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        for expected in ["hello", "sync"] {
            let frame = socket.next().await.unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            assert_eq!(value["type"], expected);
        }
        socket
    }
}
impl Drop for TestCompanion {
    fn drop(&mut self) {
        self.task.abort();
    }
}
pub struct RawClient {
    client: reqwest::Client,
    url: String,
}
impl RawClient {
    pub fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.get(format!("{}{path}", self.url))
    }
    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.post(format!("{}{path}", self.url))
    }
    pub fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.client.request(method, format!("{}{path}", self.url))
    }
}

pub struct TestClock(Ctx);
impl TestClock {
    /// Moves the Companion's monotonic clock, which session expiry reads.
    pub fn advance(&self, duration: std::time::Duration) {
        self.0.clock.advance(duration);
    }
}
