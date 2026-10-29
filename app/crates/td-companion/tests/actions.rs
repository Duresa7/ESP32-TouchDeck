use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::time::Duration;
use td_companion::{
    api::NoQuery,
    features::layouts::{LayoutBody, LayoutPath, Push},
    testing::TestCompanion,
};
use td_core::{Layout, Orientation};

async fn get(td: &TestCompanion, path: &str) -> Value {
    td.raw()
        .get(&format!("/api/v1/{path}"))
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn wait_count(td: &TestCompanion, path: &str, count: usize) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value = get(td, path).await;
            if value.as_array().unwrap().len() == count {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("expected API state")
}
async fn script(td: &TestCompanion, answer: &str) {
    td.raw()
        .post("/api/v1/test/approvals/script")
        .bearer_auth(&td.token)
        .json(&json!({"answers":[answer]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}
async fn layout(td: &TestCompanion, device: &str, count: usize, code: i32) {
    let program = if code == -1 && cfg!(windows) {
        "powershell.exe"
    } else if cfg!(windows) {
        "cmd"
    } else {
        "sh"
    };
    let flag = if code == -1 && cfg!(windows) {
        "-Command"
    } else if cfg!(windows) {
        "/C"
    } else {
        "-c"
    };
    let command = if code == -1 {
        if cfg!(windows) {
            "Start-Sleep -Seconds 3; exit 0".into()
        } else {
            "sleep 3; exit 0".into()
        }
    } else {
        format!("exit {code}")
    };
    let tiles: Vec<_> = (0..count).map(|n| json!({"id": format!("t{n}"), "label": format!("Tile {n}"), "row": n / 6, "col": n % 6, "rowSpan":1, "colSpan":1,
        "action":{"type":"command", "program":program, "args": [flag, command], "cwd":"", "shell":false}})).collect();
    let layout: Layout = serde_json::from_value(json!({"schema":1,"name":"Actions","orientation":"landscape","pages":[{"id":"main","title":"Main","kind":"grid","rows":3,"cols":6,"tiles":tiles}]})).unwrap();
    let job = td
        .call::<Push>(
            LayoutPath {
                device: device.into(),
                orientation: Orientation::Landscape,
            },
            NoQuery,
            LayoutBody::new(&layout),
        )
        .await
        .unwrap();
    assert_eq!(
        td.wait(&job).await.state,
        td_companion::jobs::JobState::Succeeded
    );
}
fn press(td: &TestCompanion, device: &str, tile: usize) {
    td.deck(device)
        .emit_press("main", &format!("t{tile}"), "command");
}
async fn frame(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    name: &str,
) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let value: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            if value["type"] == name {
                return value;
            }
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn approval_answers_repeats_changed_arguments_and_revocation() {
    let td = TestCompanion::start().await;
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", 1, 0).await;
    let mut socket = td.events().await;
    press(&td, "mock0001", 0);
    frame(&mut socket, "approval.requested").await;
    let pending = wait_count(&td, "approvals", 1).await;
    assert_eq!(get(&td, "actions/log").await, json!([]));
    press(&td, "mock0001", 0);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(get(&td, "approvals").await, pending);
    script(&td, "once").await;
    assert_eq!(
        frame(&mut socket, "approval.resolved").await["data"]["answer"],
        "once"
    );
    assert_eq!(wait_count(&td, "actions/log", 1).await[0]["exit_code"], 0);
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "deny").await;
    assert_eq!(
        wait_count(&td, "actions/log", 2).await[1]["status"],
        "denied"
    );
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "always").await;
    wait_count(&td, "actions/log", 3).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "actions/log", 4).await;
    assert_eq!(get(&td, "approvals").await, json!([]));
    layout(&td, "mock0001", 1, 3).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "deny").await;
    wait_count(&td, "actions/log", 5).await;
    let hash = get(&td, "actions/allowed").await[0]["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    td.raw()
        .request(
            reqwest::Method::DELETE,
            &format!("/api/v1/actions/allowed/{hash}"),
        )
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    layout(&td, "mock0001", 1, 0).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    td.clock().advance(Duration::from_secs(301));
    assert_eq!(
        wait_count(&td, "actions/log", 6).await[5]["status"],
        "expired"
    );
}
#[tokio::test]
async fn flooding_is_bounded_and_all_device_keys_are_suppressed() {
    let td = TestCompanion::start().await;
    td.plug_device("mock0001").await;
    td.plug_device("mock0002").await;
    layout(&td, "mock0001", 17, 0).await;
    let mut socket = td.events().await;
    for n in 0..17 {
        press(&td, "mock0001", n);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    wait_count(&td, "approvals", 16).await;
    assert_eq!(
        wait_count(&td, "actions/log", 1).await[0]["detail"],
        "limits.exceeded"
    );
    for device in ["mock0001", "mock0002"] {
        socket
            .send(tokio_tungstenite::tungstenite::Message::text(
                json!({"op":"subscribe","topic":"key-mirror","device":device}).to_string(),
            ))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        td.deck(device)
            .emit_key(&json!({"type":"kbd","mods":0,"keys":[4]}));
        assert_eq!(
            frame(&mut socket, "device.key-mirror").await["data"]["outcome"],
            "suppressed"
        );
    }
    assert!(td.input_log().is_empty());
    td.clock().advance(Duration::from_secs(301));
    wait_count(&td, "approvals", 0).await;
    wait_count(&td, "actions/log", 17).await;
    assert!(td.input_log().is_empty());
}
#[tokio::test]
async fn two_devices_resolve_their_own_actions() {
    let td = TestCompanion::start().await;
    for (device, code) in [("mock0001", 3), ("mock0002", 7)] {
        td.plug_device(device).await;
        layout(&td, device, 1, code).await;
    }
    for device in ["mock0001", "mock0002"] {
        press(&td, device, 0);
    }
    wait_count(&td, "approvals", 2).await;
    script(&td, "once").await;
    wait_count(&td, "actions/log", 1).await;
    script(&td, "once").await;
    let logs = wait_count(&td, "actions/log", 2).await;
    for (device, code) in [("mock0001", 3), ("mock0002", 7)] {
        assert!(
            logs.as_array()
                .unwrap()
                .iter()
                .any(|l| l["device"] == device && l["exit_code"] == code)
        );
    }
}

#[tokio::test]
async fn release_router_answers_no_approvals() {
    let td = TestCompanion::start_release_router().await;
    for path in [
        "/test/approvals/script",
        "/approvals/p1/answer",
        "/approvals/p1",
        "/actions/allowed",
    ] {
        let response = td
            .raw()
            .post(&format!("/api/v1{path}"))
            .bearer_auth(&td.token)
            .json(&json!({"answer":"always","answers":["always"]}))
            .send()
            .await
            .unwrap();
        assert!(matches!(response.status().as_u16(), 404 | 405));
    }
    let routes: Vec<_> = td_companion::api::registry()
        .into_iter()
        .filter(|f| f.name == "approvals" || f.name == "actions")
        .flat_map(|f| f.endpoints)
        .filter(|e| e.method != reqwest::Method::GET)
        .map(|e| e.name)
        .collect();
    assert_eq!(routes, ["actions.allowed_remove", "approvals.show"]);
}

#[tokio::test]
async fn a_slow_action_does_not_block_another_device() {
    let td = TestCompanion::start().await;
    td.plug_device("mock0001").await;
    td.plug_device("mock0002").await;
    layout(&td, "mock0001", 1, -1).await;
    layout(&td, "mock0002", 1, 7).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "once").await;
    wait_count(&td, "approvals", 0).await;
    press(&td, "mock0002", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "once").await;
    let logs = wait_count(&td, "actions/log", 1).await;
    assert_eq!(logs[0]["device"], "mock0002");
    assert_eq!(
        wait_count(&td, "actions/log", 2).await[1]["device"],
        "mock0001"
    );
}
