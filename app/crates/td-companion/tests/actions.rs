use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use td_companion::{
    actions::{ActionSpec, AllowedAction, GrantStore},
    api::NoQuery,
    features::layouts::{LayoutBody, LayoutPath, Push},
    jobs::{Job, JobState},
    testing::TestCompanion,
};
use td_core::{Layout, Orientation};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

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
struct Runs(tempfile::TempDir);
impl Runs {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }
    fn count(&self) -> usize {
        std::fs::read_to_string(self.0.path().join("runs")).map_or(0, |s| s.lines().count())
    }
    fn action(&self, code: i32) -> Value {
        if cfg!(windows) {
            self.command("cmd", "/C", &format!("echo ran>>runs& exit {code}"))
        } else {
            self.command("sh", "-c", &format!("echo ran >> runs; exit {code}"))
        }
    }
    fn uncounted_three_second_action(&self) -> Value {
        if cfg!(windows) {
            self.command(
                "powershell.exe",
                "-Command",
                "Start-Sleep -Seconds 3; exit 0",
            )
        } else {
            self.command("sh", "-c", "sleep 3; exit 0")
        }
    }
    fn command(&self, program: &str, flag: &str, command: &str) -> Value {
        json!({"type":"command", "program":program, "args":[flag, command],
            "cwd":self.0.path().to_string_lossy(), "shell":false})
    }
    fn layout(&self, count: usize, code: i32) -> Layout {
        grid((0..count).map(|_| self.action(code)).collect())
    }
}
fn grid(actions: Vec<Value>) -> Layout {
    let tiles: Vec<_> = actions.into_iter().enumerate().map(|(n, action)| json!({"id": format!("t{n}"), "label": format!("Tile {n}"), "row": n / 6, "col": n % 6, "rowSpan":1, "colSpan":1, "action":action})).collect();
    serde_json::from_value(json!({"schema":1,"name":"Actions","orientation":"landscape","pages":[{"id":"main","title":"Main","kind":"grid","rows":3,"cols":6,"tiles":tiles}]})).unwrap()
}
async fn push_layout(td: &TestCompanion, device: &str, layout: &Layout) -> Job {
    td.call::<Push>(
        LayoutPath {
            device: device.into(),
            orientation: Orientation::Landscape,
        },
        NoQuery,
        LayoutBody::new(layout),
    )
    .await
    .unwrap()
}
async fn layout(td: &TestCompanion, device: &str, layout: &Layout) {
    let job = push_layout(td, device, layout).await;
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
}
fn press(td: &TestCompanion, device: &str, tile: usize) {
    td.deck(device)
        .emit_press("main", &format!("t{tile}"), "command");
}
async fn frame(socket: &mut Socket, name: &str) -> Value {
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
async fn approval_events(socket: &mut Socket) -> Vec<String> {
    let mut seen = Vec::new();
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(Duration::from_millis(400), socket.next()).await
    {
        let Ok(value) = serde_json::from_str::<Value>(message.to_text().unwrap_or_default()) else {
            continue;
        };
        match value["type"].as_str() {
            Some("approval.requested") => seen.push("requested".to_owned()),
            Some("approval.resolved") => seen.push(format!(
                "resolved {}",
                value["data"]["answer"].as_str().unwrap()
            )),
            _ => {}
        }
    }
    seen
}
#[tokio::test]
async fn approval_answers_repeats_changed_arguments_and_revocation() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    let mut socket = td.events().await;
    press(&td, "mock0001", 0);
    let pending = wait_count(&td, "approvals", 1).await;
    press(&td, "mock0001", 0);
    assert_eq!(approval_events(&mut socket).await, ["requested"]);
    assert_eq!(get(&td, "approvals").await, pending);
    assert_eq!(get(&td, "actions/log").await, json!([]));
    assert_eq!(runs.count(), 0);

    script(&td, "once").await;
    assert_eq!(wait_count(&td, "actions/log", 1).await[0]["exit_code"], 0);
    assert_eq!(approval_events(&mut socket).await, ["resolved once"]);
    assert_eq!(runs.count(), 1);

    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "deny").await;
    assert_eq!(
        wait_count(&td, "actions/log", 2).await[1]["status"],
        "denied"
    );
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved deny"]
    );
    assert_eq!(runs.count(), 1);

    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "always").await;
    wait_count(&td, "actions/log", 3).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "actions/log", 4).await;
    assert_eq!(get(&td, "approvals").await, json!([]));
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved always"]
    );
    assert_eq!(runs.count(), 3);

    layout(&td, "mock0001", &runs.layout(1, 3)).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "deny").await;
    wait_count(&td, "actions/log", 5).await;
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved deny"]
    );
    assert_eq!(runs.count(), 3);

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
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    td.clock().advance(Duration::from_secs(301));
    assert_eq!(
        wait_count(&td, "actions/log", 6).await[5]["status"],
        "expired"
    );
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved expired"]
    );
    assert_eq!(runs.count(), 3);
}
#[tokio::test]
async fn flooding_is_bounded_and_all_device_keys_are_suppressed() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    td.plug_device("mock0002").await;
    layout(&td, "mock0001", &runs.layout(17, 0)).await;
    let mut socket = td.events().await;
    for n in 0..17 {
        press(&td, "mock0001", n);
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    wait_count(&td, "approvals", 16).await;
    let overflow = wait_count(&td, "actions/log", 1).await;
    assert_eq!(overflow[0]["status"], "denied");
    assert_eq!(overflow[0]["detail"], "limits.exceeded");
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
    assert_eq!(runs.count(), 0);
}
#[tokio::test]
async fn two_devices_resolve_their_own_actions() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    for (device, code) in [("mock0001", 3), ("mock0002", 7)] {
        td.plug_device(device).await;
        layout(&td, device, &runs.layout(1, code)).await;
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
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    td.plug_device("mock0002").await;
    layout(
        &td,
        "mock0001",
        &grid(vec![runs.uncounted_three_second_action()]),
    )
    .await;
    layout(&td, "mock0002", &runs.layout(1, 7)).await;
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

#[tokio::test]
async fn launches_and_file_opens_require_native_consent() {
    let td = TestCompanion::start().await;
    td.plug_device("mock0001").await;
    for action in [
        json!({"type":"launch", "path":"must-not-run", "args":[], "cwd":""}),
        json!({"type":"open", "target":"file:///must-not-open"}),
        json!({"type":"open", "target":"custom:must-not-open"}),
    ] {
        layout(&td, "mock0001", &grid(vec![action])).await;
        press(&td, "mock0001", 0);
        let pending = wait_count(&td, "approvals", 1).await;
        assert!(
            get(&td, "actions/log")
                .await
                .as_array()
                .unwrap()
                .iter()
                .all(|l| l["status"] == "denied")
        );
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
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
        assert_eq!(
            frame(&mut socket, "sync").await["data"]["approvals"],
            pending
        );
        let id = pending[0]["id"].as_str().unwrap();
        for status in [204, 429] {
            assert_eq!(
                td.raw()
                    .post(&format!("/api/v1/approvals/{id}/show"))
                    .bearer_auth(&td.token)
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .as_u16(),
                status
            );
        }
        script(&td, "deny").await;
        wait_count(&td, "approvals", 0).await;
    }
    let logs = wait_count(&td, "actions/log", 3).await;
    assert!(
        logs.as_array()
            .unwrap()
            .iter()
            .all(|l| l["status"] == "denied")
    );
}

#[tokio::test]
async fn tap_during_layout_push_waits_and_resolves_updated_action() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    let deck = td.deck("mock0001");
    let before = deck.received().len();
    deck.delay_replies(Duration::from_millis(150));
    let job = push_layout(&td, "mock0001", &runs.layout(1, 7)).await;
    td.until(|_| {
        deck.received()[before..]
            .iter()
            .any(|v| v["m"] == "layout.put")
    })
    .await;
    press(&td, "mock0001", 0);
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
    let pending = wait_count(&td, "approvals", 1).await;
    assert!(pending[0]["args"][1].as_str().unwrap().ends_with("exit 7"));
    script(&td, "once").await;
    assert_eq!(wait_count(&td, "actions/log", 1).await[0]["exit_code"], 7);
    assert_eq!(runs.count(), 1);
}

#[tokio::test]
async fn dialog_escapes_invisible_characters_and_grants_the_raw_value() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    let raw = "report\u{202E}gpj.exe\u{202C}";
    let mut action = runs.action(0);
    action["args"].as_array_mut().unwrap().push(json!(raw));
    layout(&td, "mock0001", &grid(vec![action.clone()])).await;
    press(&td, "mock0001", 0);
    let pending = wait_count(&td, "approvals", 1).await;
    script(&td, "always").await;
    wait_count(&td, "actions/log", 1).await;

    let text = td.prompted().pop().unwrap();
    assert!(
        text.contains(r#""report\u{202E}gpj.exe\u{202C}""#),
        "{text}"
    );
    assert!(!text.contains(['\u{202E}', '\u{202C}']), "{text}");

    let spec = ActionSpec {
        kind: "command".into(),
        program: action["program"].as_str().unwrap().into(),
        args: serde_json::from_value(action["args"].clone()).unwrap(),
        cwd: action["cwd"].as_str().unwrap().into(),
        shell: false,
    };
    assert_eq!(spec.args[2], raw);
    let allowed = get(&td, "actions/allowed").await;
    assert_eq!(allowed[0]["hash"], spec.hash());
    assert_eq!(pending[0]["hash"], spec.hash());
    assert_eq!(allowed[0]["args"][2], raw);
}

#[tokio::test]
async fn a_failed_prompt_denies() {
    let td = TestCompanion::start().await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    let mut socket = td.events().await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "fail").await;
    let log = wait_count(&td, "actions/log", 1).await;
    assert_eq!(log[0]["status"], "denied");
    assert!(
        log[0]["detail"]
            .as_str()
            .unwrap()
            .starts_with("The approval prompt failed")
    );
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved deny"]
    );
    assert_eq!(get(&td, "actions/allowed").await, json!([]));
    assert_eq!(runs.count(), 0);
}

struct TestGrants {
    fail: bool,
    entered: Mutex<mpsc::Sender<()>>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl TestGrants {
    fn new(fail: bool) -> (Arc<Self>, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, saving) = mpsc::channel();
        let (finish, release) = mpsc::channel();
        let store = Self {
            fail,
            entered: Mutex::new(entered),
            release: Mutex::new(release),
        };
        (Arc::new(store), saving, finish)
    }
}
impl GrantStore for TestGrants {
    fn load(&self) -> std::io::Result<Vec<AllowedAction>> {
        Ok(Vec::new())
    }
    fn save(&self, _: &[AllowedAction]) -> std::io::Result<()> {
        if self.fail {
            return Err(std::io::Error::other("the disk is full"));
        }
        let _ = self.entered.lock().unwrap().send(());
        let _ = self
            .release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10));
        Ok(())
    }
}

#[tokio::test]
async fn a_failed_grant_write_denies() {
    let (grants, _, _) = TestGrants::new(true);
    let td = TestCompanion::start_with_grants(grants).await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    let mut socket = td.events().await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "always").await;
    let log = wait_count(&td, "actions/log", 1).await;
    assert_eq!(log[0]["status"], "denied");
    assert!(
        log[0]["detail"]
            .as_str()
            .unwrap()
            .starts_with("The allowed action couldn't be saved")
    );
    assert_eq!(
        approval_events(&mut socket).await,
        ["requested", "resolved deny"]
    );
    assert_eq!(get(&td, "actions/allowed").await, json!([]));
    assert_eq!(runs.count(), 0);
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    assert_eq!(runs.count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_answer_while_a_grant_saves() {
    let (grants, saving, finish) = TestGrants::new(false);
    let td = TestCompanion::start_with_grants(grants).await;
    let runs = Runs::new();
    td.plug_device("mock0001").await;
    layout(&td, "mock0001", &runs.layout(1, 0)).await;
    press(&td, "mock0001", 0);
    wait_count(&td, "approvals", 1).await;
    script(&td, "always").await;
    tokio::task::spawn_blocking(move || saving.recv_timeout(Duration::from_secs(10)))
        .await
        .unwrap()
        .expect("the grant starts saving");

    let read = |path| tokio::time::timeout(Duration::from_secs(1), get(&td, path));
    let pending = read("approvals").await.expect("approvals.list answers");
    let allowed = read("actions/allowed")
        .await
        .expect("actions.allowed answers");
    let log = read("actions/log").await.expect("actions.log answers");
    assert_eq!(pending.as_array().unwrap().len(), 1);
    assert_eq!(allowed, json!([]));
    assert_eq!(log, json!([]));
    assert_eq!(runs.count(), 0);

    finish.send(()).unwrap();
    wait_count(&td, "actions/allowed", 1).await;
    assert_eq!(wait_count(&td, "actions/log", 1).await[0]["exit_code"], 0);
    assert_eq!(runs.count(), 1);
}
