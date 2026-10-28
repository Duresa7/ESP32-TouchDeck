use futures_util::StreamExt;
use serde_json::{Value, json};
use td_companion::{
    features::devices::{DevicePath, Get, List},
    testing::TestCompanion,
};
#[tokio::test]
async fn pairing_listing_and_removal() {
    let td = TestCompanion::start().await;
    let mut events = td.events().await;
    td.plug_device("mock0001").await;
    let devices = td.call::<List>((), (), ()).await.unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, "mock0001");
    assert_eq!(
        td.call::<Get>(
            DevicePath {
                device: "mock0001".into()
            },
            (),
            ()
        )
        .await
        .unwrap()
        .id,
        "mock0001"
    );
    td.plug_device("mock0002").await;
    assert_eq!(td.call::<List>((), (), ()).await.unwrap().len(), 2);
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
    assert_eq!(td.call::<List>((), (), ()).await.unwrap().len(), 1);
}
#[tokio::test]
async fn requests_are_rejected_before_shutdown() {
    let td = TestCompanion::start().await;
    let cases = [
        ("authorization", "", 401, "auth.missing"),
        ("authorization", "Bearer wrong", 401, "auth.invalid"),
        ("host", "evil.example:1", 421, "security.host"),
        ("host", "", 421, "security.host"),
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
    assert!(td.call::<List>((), (), ()).await.is_ok());
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
        assert!(td.call::<List>((), (), ()).await.is_ok());
        let _replacement = td.events().await;
    }
}
