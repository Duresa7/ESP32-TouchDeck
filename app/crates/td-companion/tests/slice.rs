use futures_util::StreamExt;
use serde_json::{Value, json};
use td_companion::{
    api::NoQuery,
    features::devices::{DevicePath, Get, List},
    testing::TestCompanion,
};
#[tokio::test]
async fn pairing_listing_and_removal() {
    let td = TestCompanion::start().await;
    let mut events = td.events().await;
    td.plug_device("mock0001").await;
    let devices = td.call::<List>((), NoQuery, ()).await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, "mock0001");
    assert_eq!(
        td.call::<Get>(
            DevicePath {
                device: "mock0001".into()
            },
            NoQuery,
            ()
        )
        .await
        .unwrap()
        .id,
        "mock0001"
    );
    td.plug_device("mock0002").await;
    assert_eq!(td.call::<List>((), NoQuery, ()).await.unwrap().len(), 2);
    td.unplug("mock0001").await;
    let removed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let message = events.next().await.unwrap().unwrap();
            if let Ok(text) = message.to_text() {
                let frame: Value = serde_json::from_str(text).unwrap();
                if frame["type"] == "device.removed" {
                    break frame;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(removed["data"]["id"], "mock0001");
    assert_eq!(td.call::<List>((), NoQuery, ()).await.unwrap().len(), 1);
}
#[tokio::test]
async fn requests_are_rejected_before_shutdown() {
    let td = TestCompanion::start().await;
    let cases = [
        ("authorization", "", 401, "auth.missing"),
        ("authorization", "Bearer wrong", 401, "auth.invalid"),
        ("host", "evil.example:1", 421, "security.host"),
        ("host", "", 421, "security.host"),
        ("host", "127.0.0.1", 421, "security.host"),
        ("host", "localhost", 421, "security.host"),
        ("host", "localhost:1", 421, "security.host"),
        ("origin", "http://evil.example", 403, "security.origin"),
        ("origin", "null", 403, "security.origin"),
        ("sec-fetch-site", "cross-site", 403, "security.origin"),
        ("content-type", "text/plain", 415, "request.media_type"),
    ];
    for (header, value, status, code) in cases {
        let mut req = td.raw().post("/api/v1/system/shutdown");
        if header != "authorization" {
            req = req.bearer_auth(&td.token);
        }
        if !value.is_empty() || header == "host" {
            req = req.header(header, value);
        }
        let response = req.send().await.unwrap();
        assert_eq!(response.status().as_u16(), status, "{header}");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["error"]["code"], code);
        assert!(
            td.raw()
                .get("/api/v1/system/ping")
                .send()
                .await
                .unwrap()
                .status()
                .is_success(),
            "rejection must not shut down the Companion"
        );
    }
    // A cross-site form post carries a foreign Origin and no token.
    let response = td
        .raw()
        .post("/api/v1/system/shutdown")
        .header("origin", "http://evil.example")
        .header("content-type", "text/plain")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    // A body without a Content-Type is refused even with a valid token.
    let response = td
        .raw()
        .post("/api/v1/system/shutdown")
        .bearer_auth(&td.token)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 415);
    assert!(
        td.raw()
            .get("/api/v1/system/ping")
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    let response = td
        .raw()
        .request(reqwest::Method::OPTIONS, "/api/v1/system/shutdown")
        .header("origin", "http://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert!(
        !response
            .headers()
            .keys()
            .any(|h| h.as_str().starts_with("access-control-"))
    );
    let port = td.url.rsplit(':').next().unwrap();
    assert!(
        td.raw()
            .get("/api/v1/devices")
            .bearer_auth(&td.token)
            .header("host", format!("localhost:{port}"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
}
#[tokio::test]
async fn reset_revokes_sockets_and_pairing_token() {
    let td = TestCompanion::start().await;
    let mut socket = td.events().await;
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
    let message = socket.next().await.unwrap().unwrap();
    let frame: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(frame["type"], "auth.revoked");
    let close = socket.next().await.unwrap().unwrap();
    assert!(
        matches!(close,tokio_tungstenite::tungstenite::Message::Close(Some(c)) if u16::from(c.code)==4401)
    );
    let response = td
        .raw()
        .post("/api/v1/system/shutdown")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "auth.revoked"
    );
    assert_eq!(
        td.raw()
            .post("/api/v1/auth/sessions")
            .bearer_auth(&td.pairing_token)
            .json(&json!({"kind":"window"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert!(
        td.raw()
            .get("/api/v1/system/ping")
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
}
#[tokio::test]
async fn proof_requires_knowledge_of_pairing_token() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let td = TestCompanion::start().await;
    let nonce = [42u8; 32];
    let proof = td
        .raw()
        .post("/api/v1/auth/proof")
        .json(&json!({"nonce":URL_SAFE_NO_PAD.encode(nonce)}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let bytes = URL_SAFE_NO_PAD
        .decode(proof["proof"].as_str().unwrap())
        .unwrap();
    for (key, valid) in [(td.pairing_token.as_str(), true), ("wrong", false)] {
        let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).unwrap();
        mac.update(b"touchdeck-companion-proof\0");
        mac.update(&nonce);
        assert_eq!(mac.verify_slice(&bytes).is_ok(), valid);
    }
}
#[tokio::test]
async fn websocket_attacks_are_rejected() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let td = TestCompanion::start().await;
    for origin in [None, Some("http://evil.example")] {
        let mut req = format!("{}/api/v1/events", td.url.replace("http:", "ws:"))
            .into_client_request()
            .unwrap();
        if let Some(origin) = origin {
            req.headers_mut().insert("origin", origin.parse().unwrap());
            req.headers_mut().insert(
                "sec-websocket-protocol",
                format!("touchdeck.v1, touchdeck.bearer.{}", td.token)
                    .parse()
                    .unwrap(),
            );
        }
        let error = tokio_tungstenite::connect_async(req).await.unwrap_err();
        let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
            panic!("expected HTTP rejection")
        };
        assert_eq!(
            response.status().as_u16(),
            if origin.is_some() { 403 } else { 401 }
        );
    }
    // Rejected upgrades consume no socket slots: all four permitted sockets remain available.
    let mut sockets = Vec::new();
    for _ in 0..4 {
        sockets.push(td.events().await);
    }
}

#[tokio::test]
async fn window_session_ends_after_sixty_seconds_without_socket() {
    let td = TestCompanion::start().await;
    td.clock().advance(std::time::Duration::from_secs(59));
    assert!(td.call::<List>((), NoQuery, ()).await.is_ok());
    td.clock().advance(std::time::Duration::from_secs(2));
    assert_eq!(
        td.raw()
            .get("/api/v1/devices")
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}
#[tokio::test]
async fn state_overflow_closes_and_releases_socket_capacity() {
    for (frames, bytes) in [(1025, 0), (1, 4 * 1024 * 1024)] {
        let td = TestCompanion::start().await;
        let mut socket = td.events().await;
        td.flood_state(frames, bytes);
        let message = tokio::time::timeout(std::time::Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            matches!(message,tokio_tungstenite::tungstenite::Message::Close(Some(c)) if u16::from(c.code)==4008)
        );
        assert!(td.call::<List>((), NoQuery, ()).await.is_ok());
        let _replacement = td.events().await;
    }
}

async fn window_session(td: &TestCompanion) -> Value {
    td.raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind": "window", "label": "Second window"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}
#[tokio::test]
async fn sessions_list_and_revoke() {
    let td = TestCompanion::start().await;
    let second = window_session(&td).await;
    let second_token = second["token"].as_str().unwrap();
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
    assert_eq!(list.len(), 2);
    let own = list.iter().find(|s| s["current"] == true).unwrap();
    assert_ne!(own["id"], second["id"]);
    assert!(list.iter().any(|s| s["label"] == "Second window"));
    assert!(
        list.iter()
            .all(|s| s.as_object().unwrap().keys().all(|k| k != "token"))
    );
    let revoke = td
        .raw()
        .request(
            reqwest::Method::DELETE,
            &format!("/api/v1/auth/sessions/{}", second["id"].as_str().unwrap()),
        )
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(revoke.status(), 204);
    let response = td
        .raw()
        .post("/api/v1/system/shutdown")
        .bearer_auth(second_token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "auth.revoked"
    );
    assert!(td.call::<List>((), NoQuery, ()).await.is_ok());
    let own = td
        .raw()
        .request(reqwest::Method::DELETE, "/api/v1/auth/sessions/self")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap();
    assert_eq!(own.status(), 204);
    assert!(td.call::<List>((), NoQuery, ()).await.is_err());
    // Revoking sessions leaves pairing alone, so a window can mint again.
    assert!(window_session(&td).await["token"].is_string());
}
#[tokio::test]
async fn unknown_and_malformed_device_ids() {
    let td = TestCompanion::start().await;
    for (id, status, code) in [
        ("mock0009", 404, "device.not_found"),
        ("MOCK0001", 400, "request.invalid"),
        ("a%2Fb", 400, "request.invalid"),
    ] {
        let response = td
            .raw()
            .get(&format!("/api/v1/devices/{id}"))
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{id}");
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["code"],
            code
        );
    }
}
#[tokio::test]
async fn rejections_use_the_one_error_shape() {
    let td = TestCompanion::start().await;
    let token = format!("touchdeck.v1, touchdeck.bearer.{}", td.token);
    let cases = [
        (
            reqwest::Method::GET,
            "/api/v1/nowhere",
            404,
            "route.not_found",
        ),
        (
            reqwest::Method::PUT,
            "/api/v1/devices",
            405,
            "route.method_not_allowed",
        ),
    ];
    for (method, path, status, code) in cases {
        let response = td
            .raw()
            .request(method, path)
            .bearer_auth(&td.token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{path}");
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["code"],
            code
        );
    }
    // A plain GET on the events route is an axum upgrade rejection underneath.
    let response = td
        .raw()
        .get("/api/v1/events")
        .header("sec-websocket-protocol", token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "request.invalid"
    );
}
#[tokio::test]
async fn session_create_checks_the_pairing_token_before_the_body() {
    let td = TestCompanion::start().await;
    let hits = td.handler_hits();
    let response = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth("tdp_wrong")
        .header("content-type", "application/json")
        .body("{not json")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "auth.invalid"
    );
    // A session token isn't the pairing token.
    let response = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.token)
        .json(&json!({"kind": "window"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(td.handler_hits(), hits);
}
#[tokio::test]
async fn session_labels_drop_control_characters() {
    let td = TestCompanion::start().await;
    let label = format!("Bad\u{1b}[31m\nlabel{}", "x".repeat(100));
    td.raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind": "window", "label": label}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
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
    let stored = list
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .find(|l| l.starts_with("Bad"))
        .unwrap();
    assert!(stored.starts_with("Bad[31mlabelx"));
    assert!(!stored.chars().any(char::is_control));
    assert_eq!(stored.chars().count(), 64);
    assert!(list.iter().all(|s| s["kind"] == "window"));
}
