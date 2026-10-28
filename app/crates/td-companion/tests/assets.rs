use serde_json::{Value, json};
use td_companion::{
    api::NoQuery,
    features::{
        assets::{
            AssetPath, BuiltinList, Create, CreateAsset, File, Prune, PruneRequest, Suggest,
            SuggestRequest,
        },
        devices::DevicePath,
        layouts::{LayoutBody, LayoutPath, Push},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_core::{Layout, Orientation};

async fn failure(request: reqwest::RequestBuilder) -> (u16, Value) {
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (
        status,
        response.json::<Value>().await.unwrap()["error"].clone(),
    )
}

#[tokio::test]
async fn builtin_icons_suggestions_and_files() {
    let td = TestCompanion::start().await;
    let builtin = td.call::<BuiltinList>((), NoQuery, ()).await.unwrap();
    assert!(builtin.len() >= 6);
    assert!(builtin.iter().all(|icon| icon.id.len() == 16));

    let suggest = |action: Value| SuggestRequest {
        action: serde_json::from_value(action).unwrap(),
    };
    let command = json!({"type": "command", "program": "x", "args": [], "cwd": "", "shell": false});
    let terminal = td
        .call::<Suggest>((), NoQuery, suggest(command))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal.name, "terminal");
    assert!(builtin.iter().any(|icon| icon.id == terminal.id));
    let web = json!({"type": "open", "target": "https://example.com"});
    let browser = td.call::<Suggest>((), NoQuery, suggest(web)).await.unwrap();
    assert_eq!(browser.unwrap().name, "browser");
    let none = td
        .call::<Suggest>((), NoQuery, suggest(json!({"type": "none"})))
        .await
        .unwrap();
    assert!(none.is_none());

    let response = td
        .request::<File>(AssetPath { asset: terminal.id }, NoQuery, &())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/vnd.touchdeck.tdi1"
    );
    let bytes = response.bytes().await.unwrap();
    assert_eq!(bytes.len(), td_host::icon_pack::FILE_BYTES);
    assert!(bytes.starts_with(b"TDI1"));
    let missing = AssetPath {
        asset: "0123456789abcdef".into(),
    };
    let (status, error) = failure(td.request::<File>(missing, NoQuery, &())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("asset.not_found"))
    );
}

#[tokio::test]
async fn prune_keeps_what_stored_layouts_and_the_caller_use() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    let used = td_host::icons::builtin()[0].id.clone();
    let layout: Layout = serde_json::from_value(json!({
        "schema": 1, "name": "Icons", "orientation": "landscape",
        "pages": [{"id": "main", "title": "Main", "kind": "grid", "rows": 1, "cols": 1, "tiles": [
            {"id": "a", "label": "A", "row": 0, "col": 0, "rowSpan": 1, "colSpan": 1,
             "icon": format!("file:{used}"), "action": {"type": "none"}}
        ]}]
    }))
    .unwrap();
    let path = LayoutPath {
        device: a.clone(),
        orientation: Orientation::Landscape,
    };
    let job = td
        .call::<Push>(path, NoQuery, LayoutBody::new(&layout))
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
    let ids = |s: &[&str]| s.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    td.deck(&a).add_icons(ids(&[
        "00000000000000aa",
        "00000000000000bb",
        "00000000000000cc",
    ]));

    let job = td
        .call::<Prune>(
            DevicePath { device: a.clone() },
            NoQuery,
            PruneRequest {
                also_keep: ids(&["00000000000000bb"]),
            },
        )
        .await
        .unwrap();
    assert_eq!(job.kind, "assets.prune");
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    assert_eq!(
        done.result.unwrap()["removed"],
        json!(["00000000000000aa", "00000000000000cc"])
    );
    assert_eq!(td.deck(&a).icons().into_iter().collect::<Vec<_>>(), {
        let mut kept = vec!["00000000000000bb".to_string(), used];
        kept.sort();
        kept
    });
    let missing = DevicePath {
        device: "mock0009".into(),
    };
    let (status, error) =
        failure(td.request::<Prune>(missing, NoQuery, &PruneRequest::default())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("device.not_found"))
    );
}

#[tokio::test]
async fn at_most_two_renders_run_at_once() {
    let td = TestCompanion::start().await;
    let create = || CreateAsset {
        source: serde_json::from_value(json!({"kind": "app", "path": "/no/such/program"})).unwrap(),
        fit: None,
    };
    td.hold_jobs();
    let first = td.call::<Create>((), NoQuery, create()).await.unwrap();
    let second = td.call::<Create>((), NoQuery, create()).await.unwrap();
    assert_eq!(first.kind, "assets.render");
    let (status, error) = failure(td.request::<Create>((), NoQuery, &create())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (429, Some("limits.exceeded"))
    );
    td.release_jobs();
    for job in [first, second] {
        let done = td.wait(&job).await;
        assert_eq!(done.state, JobState::Failed);
        assert_eq!(done.error.unwrap().code, "asset.render_failed");
    }
    // The finished renders free their slots.
    let third = td.call::<Create>((), NoQuery, create()).await.unwrap();
    td.wait(&third).await;
}
