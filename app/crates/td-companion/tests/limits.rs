use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use td_companion::{Limits, testing::TestCompanion};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

fn port(td: &TestCompanion) -> u16 {
    td.url.rsplit(':').next().unwrap().parse().unwrap()
}
/// Opens a connection and sends `head` plus `body`, leaving the rest of the body unsent.
async fn partial_request(td: &TestCompanion, head: &str, body: &str) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port(td))).await.unwrap();
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
    stream
}
fn slow_proof(td: &TestCompanion) -> String {
    format!(
        "POST /api/v1/auth/proof HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n",
        port(td)
    )
}
async fn read_all(stream: &mut TcpStream, within: Duration) -> String {
    let mut out = Vec::new();
    let _ = tokio::time::timeout(within, stream.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}
async fn pings(td: &TestCompanion, within: Duration) -> bool {
    let client = reqwest::Client::builder().timeout(within).build().unwrap();
    client
        .get(format!("{}/api/v1/system/ping", td.url))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

#[tokio::test]
async fn public_routes_read_at_most_four_kibibytes() {
    let td = TestCompanion::start().await;
    let hits = td.handler_hits();
    let nonce = "a".repeat(5000);
    for path in ["/api/v1/auth/proof", "/api/v1/auth/sessions"] {
        let mut request = td.raw().post(path).json(&json!({ "nonce": nonce }));
        if path.ends_with("sessions") {
            request = request.bearer_auth(&td.pairing_token);
        }
        let response = request.send().await.unwrap();
        assert_eq!(response.status(), 413, "{path}");
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"]["code"],
            "request.too_large"
        );
    }
    assert_eq!(td.handler_hits(), hits);
}

#[tokio::test]
async fn connections_beyond_the_cap_wait_for_a_free_slot() {
    let td = TestCompanion::start_with(Limits {
        connections: 4,
        ..Limits::default()
    })
    .await;
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(TcpStream::connect(("127.0.0.1", port(&td))).await.unwrap());
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!pings(&td, Duration::from_millis(500)).await);
    drop(held);
    assert!(pings(&td, Duration::from_secs(2)).await);
}

#[tokio::test]
async fn slow_headers_are_dropped() {
    let td = TestCompanion::start_with(Limits {
        header_read: Duration::from_millis(300),
        ..Limits::default()
    })
    .await;
    let mut stream = partial_request(&td, "GET /api/v1/system/ping HTTP/1.1\r\n", "").await;
    let started = Instant::now();
    let reply = read_all(&mut stream, Duration::from_secs(3)).await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!reply.contains("200 OK"));
}

#[tokio::test]
async fn slow_bodies_get_request_timeout() {
    let td = TestCompanion::start_with(Limits {
        body_read: Duration::from_millis(300),
        ..Limits::default()
    })
    .await;
    let hits = td.handler_hits();
    let mut stream = partial_request(&td, &slow_proof(&td), "{\"nonce\"").await;
    let reply = read_all(&mut stream, Duration::from_secs(3)).await;
    assert!(reply.starts_with("HTTP/1.1 408"), "{reply}");
    assert!(reply.contains("request.timeout"));
    assert_eq!(td.handler_hits(), hits);
}

#[tokio::test]
async fn whole_requests_are_capped() {
    // The body limit is long, so only the whole-request limit can end this one.
    let td = TestCompanion::start_with(Limits {
        body_read: Duration::from_secs(30),
        request: Duration::from_millis(300),
        ..Limits::default()
    })
    .await;
    let mut stream = partial_request(&td, &slow_proof(&td), "{\"nonce\"").await;
    let started = Instant::now();
    let reply = read_all(&mut stream, Duration::from_secs(5)).await;
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(reply.starts_with("HTTP/1.1 408"), "{reply}");
    assert!(reply.contains("request.timeout"));
}

#[tokio::test]
async fn sockets_outlive_the_request_limit() {
    let td = TestCompanion::start_with(Limits {
        request: Duration::from_millis(200),
        ..Limits::default()
    })
    .await;
    let mut socket = td.events().await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    td.plug_device("mock0001").await;
    let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(frame["type"], "device.added");
}

#[tokio::test]
async fn shutdown_drops_stuck_requests_after_the_drain() {
    let mut td = TestCompanion::start_with(Limits {
        body_read: Duration::from_secs(30),
        drain: Duration::from_millis(300),
        ..Limits::default()
    })
    .await;
    let _stuck = partial_request(&td, &slow_proof(&td), "{").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(td.stop(Duration::from_secs(2)).await);
    assert!(!pings(&td, Duration::from_millis(500)).await);
}
