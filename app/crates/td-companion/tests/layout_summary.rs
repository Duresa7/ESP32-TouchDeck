use serde_json::{Value, json};
use td_companion::{
    api::NoQuery,
    devices::{DeviceId, LinkState},
    features::{
        devices::{DevicePath, DeviceSettingsPatch, Get, Restart, SettingsPatch},
        layouts::{LayoutBody, LayoutPath, Push},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_core::Layout;

const BUILTIN: &[u8] = include_bytes!("../../../../schema/examples/builtin.json");

fn two_pages(name: &str) -> Layout {
    let page = |id: &str| {
        json!({"id": id, "title": id, "kind": "grid", "rows": 1, "cols": 1, "tiles": [
            {"id": format!("{id}_tile"), "label": "A", "row": 0, "col": 0, "rowSpan": 1, "colSpan": 1, "icon": null, "action": {"type": "none"}}
        ]})
    };
    serde_json::from_value(json!({
        "schema": 1, "name": name, "orientation": "landscape",
        "pages": [page("first"), page("second")]
    }))
    .unwrap()
}

async fn device(td: &TestCompanion, id: &DeviceId) -> Value {
    let device = td
        .call::<Get>(DevicePath { device: id.clone() }, NoQuery, ())
        .await
        .unwrap();
    serde_json::to_value(device).unwrap()
}

async fn summary(td: &TestCompanion, id: &DeviceId) -> Value {
    device(td, id).await["layout"].clone()
}

async fn until_layout(td: &TestCompanion, id: &DeviceId, want: impl Fn(&Value) -> bool) -> Value {
    td.until(|td| want(&serde_json::to_value(td.device(id).unwrap()).unwrap()["layout"]))
        .await;
    summary(td, id).await
}

async fn push(td: &TestCompanion, id: &DeviceId, layout: &Layout) {
    let path = LayoutPath {
        device: id.as_str().into(),
        orientation: layout.orientation,
    };
    let job = td
        .call::<Push>(path, NoQuery, LayoutBody::new(layout))
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
}

#[tokio::test]
async fn after_connect_the_summary_names_the_built_in_layout_and_its_page() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    let builtin = Layout::parse(BUILTIN).unwrap();
    let shown = until_layout(&td, &id, |layout| layout["page"].is_number()).await;
    assert_eq!(
        shown,
        json!({
            "source": "builtin",
            "name": builtin.name,
            "orientation": "landscape",
            "pages": builtin.pages.len(),
            "page": 0,
        })
    );
}

#[tokio::test]
async fn the_summary_follows_a_pushed_layout_and_the_page_on_screen() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    until_layout(&td, &id, |layout| layout["source"] == "builtin").await;
    push(&td, &id, &two_pages("Desk")).await;
    let shown = until_layout(&td, &id, |layout| layout["name"] == "Desk").await;
    assert_eq!(
        shown,
        json!({"source": "storage", "name": "Desk", "orientation": "landscape", "pages": 2, "page": 0})
    );
    td.deck("mock0001").show_page(1);
    let shown = until_layout(&td, &id, |layout| layout["page"] == 1).await;
    assert_eq!(shown["name"], "Desk");
    td.deck("mock0001").show_page(0);
    until_layout(&td, &id, |layout| layout["page"] == 0).await;
}

#[tokio::test]
async fn firmware_that_does_not_report_pages_leaves_the_page_null() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    push(&td, &id, &two_pages("Desk")).await;
    until_layout(&td, &id, |layout| {
        layout["name"] == "Desk" && layout["page"] == 0
    })
    .await;
    let deck = td.deck("mock0001");
    deck.set_page_reports(false);
    deck.show_page(1);
    td.unplug("mock0001").await;
    assert_eq!(summary(&td, &id).await, Value::Null);
    td.plug_device("mock0001").await;
    let shown = until_layout(&td, &id, |layout| layout["name"] == "Desk").await;
    assert_eq!(
        shown,
        json!({"source": "storage", "name": "Desk", "orientation": "landscape", "pages": 2, "page": null})
    );
    td.let_live_rounds_run(4).await;
    assert_eq!(summary(&td, &id).await["page"], Value::Null);
}

#[tokio::test]
async fn protocol_1_firmware_has_no_summary() {
    let td = TestCompanion::start().await;
    let id = td.plug_device_v1("mock0001").await;
    td.let_live_rounds_run(4).await;
    assert_eq!(summary(&td, &id).await, Value::Null);
}

#[tokio::test]
async fn a_push_starts_the_new_layout_on_its_first_page() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    push(&td, &id, &two_pages("Desk")).await;
    td.deck("mock0001").show_page(1);
    until_layout(&td, &id, |layout| {
        layout["name"] == "Desk" && layout["page"] == 1
    })
    .await;
    push(&td, &id, &two_pages("Studio")).await;
    let shown = until_layout(&td, &id, |layout| layout["name"] == "Studio").await;
    assert_eq!(shown["page"], 0);
}

#[tokio::test]
async fn the_device_orientation_follows_the_layout_it_shows() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    until_layout(&td, &id, |layout| layout["orientation"] == "landscape").await;
    assert_eq!(device(&td, &id).await["orientation"], "landscape");

    let mut upright = two_pages("Upright");
    upright.orientation = td_core::Orientation::Portrait;
    push(&td, &id, &upright).await;
    let patch = DeviceSettingsPatch {
        orientation: Some("portrait".into()),
        ..Default::default()
    };
    let patched = td
        .call::<SettingsPatch>(DevicePath { device: id.clone() }, NoQuery, patch)
        .await
        .unwrap();
    assert!(patched.restart_required);
    td.call::<Restart>(DevicePath { device: id.clone() }, NoQuery, ())
        .await
        .unwrap();
    // The restart closes the link; reconnecting takes up to one settle on its own.
    td.until(|td| !matches!(td.device(&id).unwrap().link, LinkState::Connected { .. }))
        .await;
    td.until(|td| matches!(td.device(&id).unwrap().link, LinkState::Connected { .. }))
        .await;
    until_layout(&td, &id, |layout| layout["orientation"] == "portrait").await;
    assert_eq!(device(&td, &id).await["orientation"], "portrait");

    td.unplug("mock0001").await;
    let offline = device(&td, &id).await;
    assert_eq!(offline["layout"], Value::Null);
    assert_eq!(offline["orientation"], Value::Null);
}

#[tokio::test]
async fn protocol_1_firmware_reports_no_orientation() {
    let td = TestCompanion::start().await;
    let id = td.plug_device_v1("mock0001").await;
    td.let_live_rounds_run(4).await;
    assert_eq!(device(&td, &id).await["orientation"], Value::Null);
}
