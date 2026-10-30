use serde_json::{Value, json};
use std::time::Duration;
use td_companion::testing::TestCompanion;
async fn mint(td: &TestCompanion) -> String {
    let response = td
        .raw()
        .post("/api/v1/auth/pair-codes")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    let (url, code) = td.pairing_delivery();
    let code = code.unwrap();
    let url = url.unwrap();
    assert!(url.ends_with(&format!("/pair#code={code}")));
    assert!(!body.contains(&code));
    assert!(!body.contains(&url));
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1);
    assert!(body["expires_ms"].as_u64().is_some());
    code
}
async fn pair(td: &TestCompanion, code: &str) -> reqwest::Response {
    td.raw()
        .post("/api/v1/auth/pair")
        .json(&json!({"code":code,"label":"Edge\n on Windows"}))
        .send()
        .await
        .unwrap()
}
async fn error(response: reqwest::Response, status: u16, code: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        code
    );
}
#[tokio::test]
async fn delivery_single_use_browser_permissions_and_reset() {
    let td = TestCompanion::start().await;
    let first = mint(&td).await;
    let second = mint(&td).await;
    error(pair(&td, &first).await, 401, "auth.code_invalid").await;
    let normalized = second
        .to_lowercase()
        .replace('-', " ")
        .replace('0', "o")
        .replace('1', "l");
    let response = pair(&td, &normalized).await;
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(!body.contains(&second));
    let session: Value = serde_json::from_str(&body).unwrap();
    let token = session["token"].as_str().unwrap();
    error(pair(&td, &second).await, 401, "auth.code_invalid").await;
    error(
        td.raw()
            .post("/api/v1/auth/pair-codes")
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        403,
        "auth.forbidden",
    )
    .await;
    let sessions: Value = td
        .raw()
        .get("/api/v1/auth/sessions")
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let browser = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == "browser")
        .unwrap();
    assert_eq!(browser["label"], "Edge on Windows");
    let pending = mint(&td).await;
    assert_eq!(
        td.raw()
            .post("/api/v1/auth/reset")
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    error(pair(&td, &pending).await, 401, "auth.code_invalid").await;
    error(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        401,
        "auth.revoked",
    )
    .await;
}
#[tokio::test]
async fn expiry_lockout_and_new_round() {
    let td = TestCompanion::start().await;
    let code = mint(&td).await;
    td.clock().advance(Duration::from_secs(120));
    error(pair(&td, &code).await, 401, "auth.code_invalid").await;
    let window: Value = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        td.raw()
            .post("/api/v1/auth/pair-codes")
            .bearer_auth(window["token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let (_, code) = td.pairing_delivery();
    for _ in 0..5 {
        error(pair(&td, "wrong").await, 401, "auth.code_invalid").await;
    }
    error(pair(&td, &code.unwrap()).await, 429, "auth.code_locked").await;
    assert_eq!(
        td.pairing_delivery().1.unwrap(),
        "Pairing was blocked after wrong codes. Start pairing again."
    );
    assert_eq!(
        td.raw()
            .post("/api/v1/auth/pair-codes")
            .bearer_auth(window["token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let (_, code) = td.pairing_delivery();
    assert_eq!(pair(&td, &code.unwrap()).await.status(), 200);
}
#[tokio::test]
async fn browser_idle_expiry_uses_last_request() {
    let td = TestCompanion::start().await;
    let code = mint(&td).await;
    let session: Value = pair(&td, &code).await.json().await.unwrap();
    let token = session["token"].as_str().unwrap();
    td.clock().advance(Duration::from_secs(6 * 86400));
    assert_eq!(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    td.clock().advance(Duration::from_secs(6 * 86400));
    assert_eq!(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    td.clock().advance(Duration::from_secs(7 * 86400));
    error(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        401,
        "auth.expired",
    )
    .await;
}
#[tokio::test]
async fn startup_round_trips_without_native_registry() {
    let mut td = TestCompanion::start().await;
    use td_companion::{
        api::NoQuery,
        startup::{Get, Put, StartupSetting},
    };
    assert!(!td.call::<Get>((), NoQuery, ()).await.unwrap().at_sign_in);
    assert!(
        td.call::<Put>((), NoQuery, StartupSetting { at_sign_in: true })
            .await
            .unwrap()
            .at_sign_in
    );
    td.restart().await;
    assert!(td.call::<Get>((), NoQuery, ()).await.unwrap().at_sign_in);
    assert!(
        !td.call::<Put>((), NoQuery, StartupSetting { at_sign_in: false })
            .await
            .unwrap()
            .at_sign_in
    );
}

#[tokio::test]
async fn restart_rejects_browser_session_and_pending_code() {
    let mut td = TestCompanion::start().await;
    let code = mint(&td).await;
    let session: Value = pair(&td, &code).await.json().await.unwrap();
    let pending = mint(&td).await;
    td.restart().await;
    error(pair(&td, &pending).await, 401, "auth.code_invalid").await;
    error(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(session["token"].as_str().unwrap())
            .send()
            .await
            .unwrap(),
        401,
        "auth.invalid",
    )
    .await;
}
type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
async fn browser(td: &TestCompanion) -> String {
    let code = mint(td).await;
    let session: Value = pair(td, &code).await.json().await.unwrap();
    session["token"].as_str().unwrap().into()
}
async fn events(td: &TestCompanion, token: &str) -> Socket {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = format!("{}/api/v1/events", td.url.replace("http:", "ws:"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        format!("touchdeck.v1, touchdeck.bearer.{token}")
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    for expected in ["hello", "sync"] {
        assert_eq!(frame(&mut socket).await.unwrap()["type"], expected);
    }
    socket
}
async fn frame(socket: &mut Socket) -> Option<Value> {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("a frame within 2 s");
        match message {
            Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return None,
            Some(Ok(_)) => {}
        }
    }
}
async fn until_closed(socket: &mut Socket) -> (Vec<String>, Option<u16>) {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let mut types = Vec::new();
    let code = tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(message) = socket.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    types.push(value["type"].as_str().unwrap().to_owned());
                }
                Ok(Message::Close(frame)) => return frame.map(|f| u16::from(f.code)),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    (types, code)
}
#[tokio::test]
async fn an_expired_browser_socket_closes_before_the_next_event() {
    let td = TestCompanion::start().await;
    let mut window = td.events().await;
    let token = browser(&td).await;
    let mut socket = events(&td, &token).await;
    td.clock().advance(Duration::from_secs(7 * 86400));
    td.plug_device("mock0001").await;
    assert_eq!(frame(&mut window).await.unwrap()["type"], "device.added");
    let (types, code) = until_closed(&mut socket).await;
    assert!(!types.iter().any(|t| t == "device.added"), "{types:?}");
    assert_eq!(code, Some(4401));
}
#[tokio::test]
async fn maintenance_closes_an_expired_browser_socket() {
    let td = TestCompanion::start().await;
    let token = browser(&td).await;
    let mut socket = events(&td, &token).await;
    td.clock().advance(Duration::from_secs(7 * 86400));
    td.maintain().await;
    let (types, code) = until_closed(&mut socket).await;
    assert!(!types.iter().any(|t| t == "device.added"), "{types:?}");
    assert_eq!(code, Some(4401));
    error(
        td.raw()
            .get("/api/v1/system")
            .bearer_auth(&token)
            .send()
            .await
            .unwrap(),
        401,
        "auth.expired",
    )
    .await;
}

#[tokio::test]
async fn forget_all_revokes_browser_sockets_and_preserves_windows() {
    let td = TestCompanion::start().await;
    let second: Value = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let windows = [&td.token, second["token"].as_str().unwrap()];
    let mut window_one = events(&td, windows[0]).await;
    let mut window_two = events(&td, windows[1]).await;
    let a = browser(&td).await;
    let b = browser(&td).await;
    let mut socket_a = events(&td, &a).await;
    let mut socket_b = events(&td, &b).await;
    assert_eq!(
        td.raw()
            .request(
                reqwest::Method::DELETE,
                "/api/v1/auth/sessions?kind=browser"
            )
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    for socket in [&mut socket_a, &mut socket_b] {
        let (types, code) = until_closed(socket).await;
        assert_eq!(types, ["auth.revoked"]);
        assert_eq!(code, Some(4401));
    }
    for token in windows {
        assert_eq!(
            td.raw()
                .get("/api/v1/system")
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    td.plug_device("mock0001").await;
    for socket in [&mut window_one, &mut window_two] {
        assert_eq!(frame(socket).await.unwrap()["type"], "device.added");
    }
}

#[tokio::test]
async fn forget_all_rejects_browser_without_revoking_anything() {
    let td = TestCompanion::start().await;
    let a = browser(&td).await;
    let b = browser(&td).await;
    error(
        td.raw()
            .request(
                reqwest::Method::DELETE,
                "/api/v1/auth/sessions?kind=browser",
            )
            .bearer_auth(&a)
            .send()
            .await
            .unwrap(),
        403,
        "auth.forbidden",
    )
    .await;
    for token in [&td.token, &a, &b] {
        assert_eq!(
            td.raw()
                .get("/api/v1/system")
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
}

#[tokio::test]
async fn forget_all_validates_kind_and_request_security() {
    let td = TestCompanion::start().await;
    for query in ["", "?kind=window", "?kind=", "?kind=other"] {
        error(
            td.raw()
                .request(
                    reqwest::Method::DELETE,
                    &format!("/api/v1/auth/sessions{query}"),
                )
                .bearer_auth(&td.token)
                .send()
                .await
                .unwrap(),
            400,
            "request.invalid",
        )
        .await;
    }
    for (header, value, status, code) in [
        ("host", "evil.example", 421, "security.host"),
        ("origin", "http://evil.example", 403, "security.origin"),
        ("sec-fetch-site", "cross-site", 403, "security.origin"),
    ] {
        error(
            td.raw()
                .request(
                    reqwest::Method::DELETE,
                    "/api/v1/auth/sessions?kind=browser",
                )
                .bearer_auth(&td.token)
                .header(header, value)
                .send()
                .await
                .unwrap(),
            status,
            code,
        )
        .await;
    }
}
