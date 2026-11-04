use serde_json::{Value, json};
use std::time::{Duration, Instant};
use td_companion::{
    api::NoQuery,
    devices::DeviceId,
    features::{
        integrations::{ConfigPut, Get, IntegrationPath},
        layouts::{LayoutBody, LayoutPath, Push},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_core::{InputPhase, Layout};
use td_link::MockDeck;

const MEDIA: &[u8] = include_bytes!("../../../../schema/examples/media.json");

#[tokio::test]
async fn volume_hooks_script_and_read_the_fake_and_preserve_its_state_on_bad_input() {
    let td = TestCompanion::start().await;
    let response = td
        .raw()
        .post("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .json(&json!({"level":0.37,"muted":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    let state: serde_json::Value = td
        .raw()
        .get("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state, json!({"level":0.37,"muted":true,"sets":0}));
    let response = td
        .raw()
        .post("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .json(&json!({"level":10,"muted":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let state: serde_json::Value = td
        .raw()
        .get("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(state["level"], 0.37);
    let view: serde_json::Value = td
        .raw()
        .get("/api/v1/integrations/volume")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(view["summary"]["presets"][0]["input"], true);
    assert_eq!(view["config"], json!({"enabled":true}));
}

#[tokio::test]
async fn release_router_has_no_volume_hook_or_control_routes() {
    let td = TestCompanion::start_release_router().await;
    for path in [
        "/api/v1/test-hooks/volume",
        "/api/v1/integrations/volume/set",
        "/api/v1/integrations/volume/act",
    ] {
        assert!(
            [404, 405].contains(
                &td.raw()
                    .post(path)
                    .bearer_auth(&td.token)
                    .json(&json!({"level":0.9,"muted":false}))
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .as_u16()
            )
        );
        assert_eq!(
            td.raw()
                .get(path)
                .bearer_auth(&td.token)
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
}

fn volume() -> IntegrationPath {
    IntegrationPath {
        integration: "volume".into(),
    }
}

async fn turn(td: &TestCompanion, enabled: bool) {
    td.call::<ConfigPut>(volume(), NoQuery, json!({"enabled": enabled}))
        .await
        .unwrap();
}

async fn pc_volume(td: &TestCompanion, level: f64) {
    let response = td
        .raw()
        .post("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .json(&json!({"level": level, "muted": false}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
}

async fn pc_sets(td: &TestCompanion) -> u64 {
    let state: Value = td
        .raw()
        .get("/api/v1/test-hooks/volume")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    state["sets"].as_u64().unwrap()
}

async fn show_media_layout(td: &TestCompanion, device: &DeviceId) {
    let layout = Layout::parse(MEDIA).unwrap();
    let job = td
        .call::<Push>(
            LayoutPath {
                device: device.as_str().into(),
                orientation: layout.orientation,
            },
            NoQuery,
            LayoutBody::new(&layout),
        )
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
}

async fn until(what: &str, within: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn bar(deck: &MockDeck, stream: &str) -> Option<(bool, Option<f64>)> {
    deck.live("media", "volume", stream)
        .map(|live| (live.stale, live.samples.last().copied()))
}

fn volume_sets(deck: &MockDeck) -> Vec<Value> {
    deck.live_sets()
        .into_iter()
        .filter(|set| set["tile"] == "volume")
        .collect()
}

#[tokio::test]
async fn turning_volume_off_tells_the_device_its_values_are_stale_and_turning_it_on_refreshes_them()
{
    let td = TestCompanion::start().await;
    let device = td.plug_media_device("mock0001").await;
    show_media_layout(&td, &device).await;
    let deck = td.deck("mock0001");

    pc_volume(&td, 0.4).await;
    until("both bars show 0.4", Duration::from_secs(5), || {
        bar(&deck, "value") == Some((false, Some(0.4)))
            && bar(&deck, "muted") == Some((false, Some(0.0)))
    })
    .await;

    turn(&td, false).await;
    until("both bars go stale", Duration::from_secs(5), || {
        bar(&deck, "value") == Some((true, Some(0.4)))
            && bar(&deck, "muted") == Some((true, Some(0.0)))
    })
    .await;
    let told = volume_sets(&deck);
    let last_two = &told[told.len() - 2..];
    assert!(
        last_two.iter().all(|set| set["ttl_ms"] == 1_000),
        "{last_two:?}"
    );

    pc_volume(&td, 0.5).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(volume_sets(&deck), told, "nothing more is sent while off");
    assert_eq!(bar(&deck, "value"), Some((true, Some(0.4))));

    turn(&td, true).await;
    until("both bars show 0.5", Duration::from_secs(5), || {
        bar(&deck, "value") == Some((false, Some(0.5)))
            && bar(&deck, "muted") == Some((false, Some(0.0)))
    })
    .await;
}

#[tokio::test]
async fn a_bar_drag_in_progress_is_cancelled_when_volume_is_turned_off() {
    let td = TestCompanion::start().await;
    let device = td.plug_media_device("mock0001").await;
    show_media_layout(&td, &device).await;
    let deck = td.deck("mock0001");
    pc_volume(&td, 0.4).await;
    until("the bar shows 0.4", Duration::from_secs(5), || {
        bar(&deck, "value") == Some((false, Some(0.4)))
    })
    .await;

    deck.emit_input("media", "volume", "value", InputPhase::Move, 0.3);
    until("the drag sets the volume", Duration::from_secs(5), || {
        bar(&deck, "value").is_some_and(|(_, value)| value == Some(0.3))
    })
    .await;
    let acted_on = pc_sets(&td).await;
    assert!(acted_on >= 1);

    turn(&td, false).await;
    for value in [0.6, 0.7] {
        deck.emit_input("media", "volume", "value", InputPhase::Move, value);
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    deck.emit_input("media", "volume", "value", InputPhase::End, 0.8);
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(
        pc_sets(&td).await,
        acted_on,
        "the Companion acted on a cancelled drag"
    );
    let log: Value = td
        .raw()
        .get("/api/v1/actions/log")
        .bearer_auth(&td.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(log, json!([]), "a cancelled drag logs nothing");
}

#[tokio::test]
async fn a_turned_off_volume_reports_what_this_pc_reports_when_it_is_off() {
    let td = TestCompanion::start().await;
    let mut off = Vec::new();
    for id in ["this_pc", "volume"] {
        let path = || IntegrationPath {
            integration: id.into(),
        };
        td.call::<ConfigPut>(path(), NoQuery, json!({"enabled": false}))
            .await
            .unwrap();
        off.push(td.call::<Get>(path(), NoQuery, ()).await.unwrap().summary);
    }
    assert!(off.iter().all(|summary| !summary.enabled));
    assert_eq!(off[0].status.state, off[1].status.state);
    assert_eq!(off[1].status.message, None);
    assert!(off[1].status.delivery.iter().all(|d| d.error.is_none()));
}
