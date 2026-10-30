use futures_util::StreamExt;
use serde_json::{Value, json};
use td_companion::{api::NoQuery, features::integrations::*, testing::TestCompanion};
fn path() -> IntegrationPath {
    IntegrationPath {
        integration: "cliproxyapi".into(),
    }
}
fn secret() -> SecretPath {
    SecretPath {
        integration: "cliproxyapi".into(),
        field: "management_key".into(),
    }
}
fn config(url: &str) -> Value {
    json!({"enabled":true,"url":url,"interval_s":120})
}

#[tokio::test]
async fn routes_preserve_same_origin_and_clear_changed_origins_and_emit_safe_status() {
    let td = TestCompanion::start().await;
    let list = td.call::<List>((), NoQuery, ()).await.unwrap();
    assert_eq!(list[0].id, "cliproxyapi");
    assert_eq!(list[0].status.state, "not_configured");
    assert!(!list[0].presets.is_empty());
    for next in [
        "http://hub.example",
        "https://other.example",
        "https://hub.example:8443",
    ] {
        td.call::<ConfigPut>(path(), NoQuery, config("https://hub.example"))
            .await
            .unwrap();
        let mut events = td.events().await;
        let hint = td
            .call::<SecretPut>(
                secret(),
                NoQuery,
                SecretInput {
                    value: "fixture-private-value".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(hint.hint, "Set");
        let message = tokio::time::timeout(std::time::Duration::from_secs(2), events.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = message.to_text().unwrap();
        assert!(!text.contains("fixture-private-value"));
        let event: Value = serde_json::from_str(text).unwrap();
        assert_eq!(event["type"], "integration.status");
        assert_eq!(event["data"]["id"], "cliproxyapi");
        let same = td
            .call::<ConfigPut>(path(), NoQuery, config("https://hub.example:443/path"))
            .await
            .unwrap();
        assert!(same.secrets["management_key"].set);
        assert!(
            !serde_json::to_string(&same)
                .unwrap()
                .contains("fixture-private-value")
        );
        let (refresh, changed) = tokio::join!(
            td.call::<Refresh>(path(), NoQuery, ()),
            td.call::<ConfigPut>(path(), NoQuery, config(next))
        );
        refresh.unwrap();
        assert!(!changed.unwrap().secrets["management_key"].set);
        assert!(!td.call::<Get>(path(), NoQuery, ()).await.unwrap().secrets["management_key"].set);
    }
    td.call::<SecretPut>(secret(), NoQuery, SecretInput { value: "ab".into() })
        .await
        .unwrap();
    td.call::<SecretDelete>(secret(), NoQuery, ())
        .await
        .unwrap();
    assert!(!td.call::<Get>(path(), NoQuery, ()).await.unwrap().secrets["management_key"].set);
}
#[tokio::test]
async fn sync_contains_safe_integration_summaries() {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let td = TestCompanion::start().await;
    td.call::<SecretPut>(
        secret(),
        NoQuery,
        SecretInput {
            value: "fixture-private-value".into(),
        },
    )
    .await
    .unwrap();
    let mut request = format!("{}/api/v1/events", td.url.replace("http:", "ws:"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        format!("touchdeck.v1, touchdeck.bearer.{}", td.token)
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket.next().await.unwrap().unwrap();
    let message = socket.next().await.unwrap().unwrap();
    let text = message.to_text().unwrap();
    assert!(!text.contains("fixture-private-value"));
    let frame: Value = serde_json::from_str(text).unwrap();
    assert_eq!(frame["type"], "sync");
    assert_eq!(frame["data"]["integrations"][0]["id"], "cliproxyapi");
}
#[tokio::test]
async fn invalid_config_unknown_fields_and_unauthenticated_calls_are_rejected() {
    let td = TestCompanion::start().await;
    let client = reqwest::Client::new();
    for body in [
        json!({"enabled":true,"url":"https://hub.example","interval_s":120,"management_key":"fixture-private-value"}),
        config("https://user:fixture-private-value@hub.example"),
    ] {
        let response = client
            .put(format!("{}/api/v1/integrations/cliproxyapi/config", td.url))
            .bearer_auth(&td.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 422);
        let text = response.text().await.unwrap();
        assert!(!text.contains("fixture-private-value"));
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap()["error"]["code"],
            "integration.config_invalid"
        );
    }
    assert_eq!(
        td.call::<Get>(
            IntegrationPath {
                integration: "missing".into()
            },
            NoQuery,
            ()
        )
        .await
        .err()
        .unwrap()
        .status()
        .unwrap(),
        404
    );
    for (method, suffix, body) in [
        ("GET", "", Value::Null),
        ("GET", "/cliproxyapi", Value::Null),
        ("PUT", "/cliproxyapi/config", config("https://hub.example")),
        (
            "PUT",
            "/cliproxyapi/secrets/management_key",
            json!({"value":"fixture-private-value"}),
        ),
        ("DELETE", "/cliproxyapi/secrets/management_key", Value::Null),
        ("POST", "/cliproxyapi/refresh", Value::Null),
    ] {
        let response = client
            .request(
                method.parse().unwrap(),
                format!("{}/api/v1/integrations{suffix}", td.url),
            )
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
    }
}
