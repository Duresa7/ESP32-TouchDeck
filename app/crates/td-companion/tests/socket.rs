use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use td_companion::testing::TestCompanion;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Error, Message, client::IntoClientRequest, http::Response},
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type Opened = Result<(Socket, Response<Option<Vec<u8>>>), Error>;
type Headers<'a> = &'a [(&'a str, &'a str)];

/// Opens the events socket with `token` and extra headers, without reading anything.
async fn open(td: &TestCompanion, token: &str, headers: &[(&str, &str)]) -> Opened {
    let mut request = format!("{}/api/v1/events", td.url.replace("http:", "ws:"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        format!("touchdeck.v1, touchdeck.bearer.{token}")
            .parse()
            .unwrap(),
    );
    for (name, value) in headers {
        let name: tokio_tungstenite::tungstenite::http::HeaderName = name.parse().unwrap();
        request.headers_mut().insert(name, value.parse().unwrap());
    }
    connect_async(request).await
}
fn rejected_with(result: Opened) -> (u16, Value) {
    let Err(Error::Http(response)) = result else {
        panic!("expected an HTTP rejection")
    };
    let body = serde_json::from_slice(response.body().as_deref().unwrap_or(b"null")).unwrap();
    (response.status().as_u16(), body)
}
async fn window_session(td: &TestCompanion) -> String {
    let created: Value = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind": "window"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    created["token"].as_str().unwrap().into()
}
/// Reads frames until a close frame or the end of the stream.
async fn close_code(socket: &mut Socket, within: Duration) -> Option<u16> {
    tokio::time::timeout(within, async {
        while let Some(message) = socket.next().await {
            match message {
                Ok(Message::Close(frame)) => return frame.map(|f| u16::from(f.code)),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .expect("the socket should end")
}

#[tokio::test]
async fn stalled_readers_lose_their_slot() {
    let td = TestCompanion::start().await;
    let mut stalled = Vec::new();
    for _ in 0..4 {
        stalled.push(open(&td, &td.token, &[]).await.unwrap().0);
    }
    assert_eq!(rejected_with(open(&td, &td.token, &[]).await).0, 429);
    // Nobody reads, so the server's sends and queues back up.
    td.flood_state(16, 1 << 20);
    let reopened = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Ok(socket) = open(&td, &td.token, &[]).await {
                break socket;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(reopened.is_ok(), "a stalled socket kept its slot");
}

#[tokio::test]
async fn oversized_client_frames_end_the_socket() {
    let td = TestCompanion::start().await;
    let mut socket = td.events().await;
    let _ = socket.send(Message::text("x".repeat(10_000))).await;
    let _ = close_code(&mut socket, Duration::from_secs(3)).await;
    // The slot is free again and the Companion still answers.
    let mut sockets = Vec::new();
    for _ in 0..4 {
        sockets.push(td.events().await);
    }
}

#[tokio::test]
async fn socket_caps_per_session_and_overall() {
    let td = TestCompanion::start().await;
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(open(&td, &td.token, &[]).await.unwrap().0);
    }
    let (status, body) = rejected_with(open(&td, &td.token, &[]).await);
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (429, Some("limits.exceeded"))
    );
    for _ in 0..3 {
        let token = window_session(&td).await;
        for _ in 0..4 {
            held.push(open(&td, &token, &[]).await.unwrap().0);
        }
    }
    assert_eq!(held.len(), 16);
    let fresh = window_session(&td).await;
    let (status, body) = rejected_with(open(&td, &fresh, &[]).await);
    assert_eq!(
        (status, body["error"]["code"].as_str()),
        (429, Some("limits.exceeded"))
    );
}

#[tokio::test]
async fn upgrade_attacks_never_reach_the_handler() {
    let td = TestCompanion::start().await;
    let hits = td.handler_hits();
    let port = td.url.rsplit(':').next().unwrap().to_owned();
    let foreign = format!("evil.example:{port}");
    let cases: [(Headers, u16, &str); 4] = [
        (&[("host", &foreign)], 421, "security.host"),
        (&[("origin", "null")], 403, "security.origin"),
        (&[("origin", "http://evil.example")], 403, "security.origin"),
        (&[("sec-fetch-site", "cross-site")], 403, "security.origin"),
    ];
    for (headers, status, code) in cases {
        let (got, body) = rejected_with(open(&td, &td.token, headers).await);
        assert_eq!(got, status, "{headers:?}");
        assert_eq!(body["error"]["code"], code, "{headers:?}");
    }
    assert_eq!(td.handler_hits(), hits);
    assert!(!td.shutdown_requested());
}

#[tokio::test]
async fn the_token_is_never_echoed() {
    let td = TestCompanion::start().await;
    let (_socket, response) = open(&td, &td.token, &[]).await.unwrap();
    let selected: Vec<_> = response
        .headers()
        .get_all("sec-websocket-protocol")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(selected, ["touchdeck.v1"]);
}

#[tokio::test]
async fn revoking_one_session_closes_only_its_socket() {
    let td = TestCompanion::start().await;
    let other = window_session(&td).await;
    let mut mine = td.events().await;
    let (mut theirs, _) = open(&td, &other, &[]).await.unwrap();
    let list: Vec<Value> = td
        .raw()
        .get("/api/v1/auth/sessions")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = list.iter().find(|s| s["current"] == false).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    td.raw()
        .request(
            reqwest::Method::DELETE,
            &format!("/api/v1/auth/sessions/{id}"),
        )
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        close_code(&mut theirs, Duration::from_secs(2)).await,
        Some(4401)
    );
    td.plug_device("mock0001").await;
    let frame = tokio::time::timeout(Duration::from_secs(2), mine.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(frame["type"], "device.added");
}

#[tokio::test]
async fn a_window_session_ends_sixty_seconds_after_its_socket_closes() {
    let td = TestCompanion::start().await;
    let list = || async {
        td.raw()
            .get("/api/v1/devices")
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    };
    let mut socket = td.events().await;
    // An open socket keeps the session alive however long it lasts.
    td.clock().advance(Duration::from_secs(120));
    assert_eq!(list().await, 200);
    socket.close(None).await.unwrap();
    let _ = close_code(&mut socket, Duration::from_secs(2)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    td.clock().advance(Duration::from_secs(59));
    assert_eq!(list().await, 200);
    td.clock().advance(Duration::from_secs(2));
    assert_eq!(list().await, 401);
}

#[tokio::test]
async fn shutdown_says_stopping_then_closes_with_1001() {
    let mut td = TestCompanion::start().await;
    let mut socket = td.events().await;
    assert!(td.stop(Duration::from_secs(3)).await);
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(frame["type"], "companion.stopping");
    assert_eq!(frame["data"]["reason"], "shutdown");
    assert_eq!(
        close_code(&mut socket, Duration::from_secs(2)).await,
        Some(1001)
    );
}

#[tokio::test]
async fn maintenance_forgets_ended_sessions() {
    let td = TestCompanion::start().await;
    let code = |token: String| {
        let td = &td;
        async move {
            let response = td
                .raw()
                .get("/api/v1/devices")
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            let body: Value = response.json().await.unwrap_or(Value::Null);
            body["error"]["code"].as_str().unwrap_or("ok").to_owned()
        }
    };
    let revoked = window_session(&td).await;
    let idle = window_session(&td).await;
    let mut socket = td.events().await;
    let list: Vec<Value> = td
        .raw()
        .get("/api/v1/auth/sessions")
        .bearer_auth(&revoked)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = list.iter().find(|s| s["current"] == true).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    td.raw()
        .request(
            reqwest::Method::DELETE,
            &format!("/api/v1/auth/sessions/{id}"),
        )
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(code(revoked.clone()).await, "auth.revoked");
    td.clock().advance(Duration::from_secs(61));
    td.maintain().await;
    assert_eq!(code(revoked).await, "auth.invalid");
    assert_eq!(code(idle).await, "auth.invalid");
    // A session with an open socket is kept, however long it lasts.
    assert_eq!(code(td.token.clone()).await, "ok");
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn subscribe_is_accepted_and_ignored() {
    let td = TestCompanion::start().await;
    let mut socket = td.events().await;
    for op in ["subscribe", "unsubscribe"] {
        socket
            .send(Message::text(
                json!({"op": op, "topic": "console", "device": "mock0001"}).to_string(),
            ))
            .await
            .unwrap();
    }
    td.plug_device("mock0001").await;
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(frame["type"], "device.added");
}
