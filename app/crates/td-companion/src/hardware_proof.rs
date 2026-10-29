use crate::{
    api::Ctx,
    devices::{Config, LinkState},
    features::firmware::{Confirmation, Prompter},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
struct Scripted;
impl Prompter for Scripted {
    fn pick(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(
            std::fs::read(std::env::var("PROOF_CRASH_BUNDLE").unwrap()).unwrap(),
        ))
    }
    fn confirm(&self, c: &Confirmation) -> bool {
        println!(
            "SCRIPTED confirmation target={} port={} current={} version={} build={}",
            c.target, c.port, c.current, c.version, c.build
        );
        true
    }
}
#[tokio::test]
#[ignore]
async fn hardware_api_update_and_rollback() {
    assert_eq!(std::env::var("PROOF_HARDWARE_LOCK").unwrap(), "77");
    let data = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let ctx = Ctx::new(port, false, None, crate::Limits::default(), data.path());
    ctx.firmware.set_prompter(Arc::new(Scripted));
    ctx.devices.start(Config::real(data.path()));
    let router = crate::api::router(ctx.clone());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{port}/api/v1");
    let session: Value = client
        .post(format!("{url}/auth/sessions"))
        .bearer_auth(ctx.pairing_token())
        .json(&json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = session["token"].as_str().unwrap();
    let mut request = format!("ws://127.0.0.1:{port}/api/v1/events")
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "sec-websocket-protocol",
        format!("touchdeck.v1, touchdeck.bearer.{token}")
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        if let Some(d) = ctx
            .devices
            .summaries()
            .into_iter()
            .find(|d| matches!(&d.link, LinkState::Connected { port, .. } if port == "COM3"))
        {
            break d.id;
        }
        assert!(Instant::now() < deadline, "Device detection timeout");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    socket
        .send(Message::text(
            json!({"op":"subscribe","topic":"console","device":id}).to_string(),
        ))
        .await
        .unwrap();
    let capture = tokio::spawn(async move {
        let mut records = Vec::new();
        while let Some(Ok(message)) = socket.next().await {
            if let Ok(text) = message.to_text() {
                records.push(text.to_owned());
            }
        }
        records
    });
    for rollback in [false, true] {
        let selection = if rollback {
            let result: Value = client
                .post(format!("{url}/firmware/bundles/pick"))
                .bearer_auth(token)
                .json(&json!({"device":id}))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            result["bundle"]["id"].as_str().unwrap().to_owned()
        } else {
            "bundled".into()
        };
        let response = client
            .post(format!("{url}/devices/{id}/firmware/update"))
            .bearer_auth(token)
            .json(&json!({"bundle":selection}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
        let job: Value = response.json().await.unwrap();
        let job_id = job["id"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let job: Value = client
                .get(format!("{url}/jobs/{job_id}"))
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            println!("{}", json!({"rollback":rollback,"job":job}));
            if ["failed", "succeeded", "cancelled"].contains(&job["state"].as_str().unwrap()) {
                if rollback {
                    assert_eq!(job["error"]["code"], "firmware.rolled_back");
                } else {
                    assert_eq!(job["state"], "succeeded", "{job}");
                }
                break;
            }
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ctx
            .devices
            .get(&id)
            .is_some_and(|d| matches!(d.link, LinkState::Connected { .. }))
        {
            assert!(Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!(
            "RECONNECTED {}",
            serde_json::to_string(&ctx.devices.get(&id)).unwrap()
        );
    }
    client
        .delete(format!("{url}/auth/sessions/self"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    let records = tokio::time::timeout(Duration::from_secs(5), capture)
        .await
        .unwrap()
        .unwrap();
    std::fs::write(std::env::var("PROOF_EVENTS").unwrap(), records.join("\n")).unwrap();
    ctx.devices.stop();
    task.abort();
}
