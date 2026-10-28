use futures_util::StreamExt;
use serde_json::{Value, json};
use td_companion::{
    api::NoQuery,
    features::{
        jobs::{Cancel, JobPath, List},
        layouts::{Get, LayoutBody, LayoutPath, Push, PushReport},
    },
    jobs::{Job, JobState},
    testing::TestCompanion,
};
use td_core::{Layout, Orientation};

fn layout(name: &str, orientation: Orientation, icon: Option<&str>) -> Layout {
    serde_json::from_value(json!({
        "schema": 1, "name": name, "orientation": orientation,
        "pages": [{"id": "main", "title": "Main", "kind": "grid", "rows": 2, "cols": 2, "tiles": [
            {"id": "a", "label": "A", "row": 0, "col": 0, "rowSpan": 1, "colSpan": 1,
             "icon": icon, "action": {"type": "none"}}
        ]}]
    }))
    .unwrap()
}
fn path(device: &str, orientation: Orientation) -> LayoutPath {
    LayoutPath {
        device: device.into(),
        orientation,
    }
}
async fn push(td: &TestCompanion, device: &str, layout: &Layout) -> Job {
    td.call::<Push>(
        path(device, layout.orientation),
        NoQuery,
        LayoutBody::new(layout),
    )
    .await
    .unwrap()
}
/// The status and error body of a request that fails.
async fn failure(request: reqwest::RequestBuilder) -> (u16, Value) {
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (
        status,
        response.json::<Value>().await.unwrap()["error"].clone(),
    )
}
fn report(job: &Job) -> PushReport {
    assert_eq!(job.state, JobState::Succeeded, "{job:?}");
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

#[tokio::test]
async fn two_devices_keep_their_own_layouts() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    let b = td.plug_device("mock0002").await;
    let first = layout("First", Orientation::Landscape, None);
    let second = layout("Second", Orientation::Landscape, None);
    let upright = layout("Upright", Orientation::Portrait, None);
    for (device, layout) in [(&a, &first), (&b, &second), (&a, &upright)] {
        let job = push(&td, device, layout).await;
        assert_eq!(job.kind, "layouts.push");
        let pushed = report(&td.wait(&job).await);
        let crc = td_link::xfer::crc32(&layout.to_canonical_json());
        assert_eq!(pushed.revision, format!("{crc:08x}"));
        assert_eq!(pushed.bytes, layout.to_canonical_json().len() as u64);
    }
    for (device, orientation, expected) in [
        (&a, Orientation::Landscape, &first),
        (&b, Orientation::Landscape, &second),
        (&a, Orientation::Portrait, &upright),
    ] {
        let response = td
            .request::<Get>(path(device, orientation), NoQuery, &())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let etag = response.headers()["etag"].to_str().unwrap().to_owned();
        let read: Value = response.json().await.unwrap();
        let stored = td.deck(device).layout(orientation).unwrap();
        let revision = format!("{:08x}", td_link::xfer::crc32(&stored));
        assert_eq!(read["revision"], revision);
        assert_eq!(etag, format!("\"{revision}\""));
        assert_eq!(
            serde_json::from_value::<Layout>(read["layout"].clone()).unwrap(),
            *expected
        );
    }
    // The second Device never had a portrait Layout.
    let (status, error) =
        failure(td.request::<Get>(path(&b, Orientation::Portrait), NoQuery, &())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("layout.not_found"))
    );
}

#[tokio::test]
async fn revisions_orientation_and_validation_are_checked() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    let first = layout("First", Orientation::Landscape, None);
    td.wait(&push(&td, &a, &first).await).await;
    let read = td
        .call::<Get>(path(&a, Orientation::Landscape), NoQuery, ())
        .await
        .unwrap();
    let put = |layout: &Layout, if_match: &str| {
        td.request::<Push>(
            path(&a, layout.orientation),
            NoQuery,
            &LayoutBody::new(layout),
        )
        .header("if-match", if_match)
    };

    // A stale revision is refused before any Job starts.
    let second = layout("Second", Orientation::Landscape, None);
    let (status, error) = failure(put(&second, "\"00000000\"")).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (412, Some("layout.changed"))
    );
    assert_eq!(error["details"]["revision"], read.revision.as_str());
    assert!(td.call::<List>((), NoQuery, ()).await.unwrap().len() == 1);

    // The current revision passes, and then it's stale.
    let response = put(&second, &format!("\"{}\"", read.revision))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let pushed = report(&td.wait(&response.json().await.unwrap()).await);
    assert_ne!(pushed.revision, read.revision);
    let (status, _) = failure(put(&first, &read.revision)).await;
    assert_eq!(status, 412);
    let response = put(&first, &pushed.revision).send().await.unwrap();
    assert_eq!(response.status(), 202);

    // The path and the body must name the same orientation.
    let raw = |orientation: &str, body: Value| {
        td.raw()
            .request(
                reqwest::Method::PUT,
                &format!("/api/v1/devices/{a}/layouts/{orientation}"),
            )
            .bearer_auth(&td.token)
            .json(&body)
    };
    let landscape = serde_json::to_value(&first).unwrap();
    let (status, error) = failure(raw("portrait", landscape.clone())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (422, Some("layout.orientation_mismatch"))
    );

    // A Layout that fails validation lists its issues.
    let mut overlapping = landscape.clone();
    let mut tile = overlapping["pages"][0]["tiles"][0].clone();
    tile["id"] = json!("b");
    overlapping["pages"][0]["tiles"]
        .as_array_mut()
        .unwrap()
        .push(tile);
    let (status, error) = failure(raw("landscape", overlapping)).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (422, Some("layout.invalid"))
    );
    let issues = error["details"]["issues"].as_array().unwrap();
    assert!(
        issues.iter().any(|i| i["rule"] == "tile.overlap"),
        "{issues:?}"
    );
    let mut unknown = landscape;
    unknown["colour"] = json!("red");
    let (status, error) = failure(raw("landscape", unknown)).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (422, Some("layout.invalid"))
    );
    assert_eq!(error["details"]["issues"][0]["rule"], "shape.unknown-field");

    // An unknown Device.
    let (status, error) =
        failure(td.request::<Get>(path("mock0009", Orientation::Landscape), NoQuery, &())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("device.not_found"))
    );
}

#[tokio::test]
async fn a_second_push_gets_the_busy_job() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    let b = td.plug_device("mock0002").await;
    let mut events = td.events().await;
    td.hold_jobs();
    let first = push(&td, &a, &layout("First", Orientation::Landscape, None)).await;
    let (status, error) = failure(td.request::<Push>(
        path(&a, Orientation::Landscape),
        NoQuery,
        &LayoutBody::new(&layout("Second", Orientation::Landscape, None)),
    ))
    .await;
    assert_eq!((status, error["code"].as_str()), (409, Some("device.busy")));
    assert_eq!(error["details"]["job"]["id"], first.id.as_str());
    // A Job on one Device doesn't block another.
    let other = push(&td, &b, &layout("Other", Orientation::Landscape, None)).await;
    td.release_jobs();
    let done = td.wait(&first).await;
    assert_eq!(report(&done).state.name, "First");
    report(&td.wait(&other).await);
    assert_eq!(
        td.deck(&a).layout(Orientation::Landscape).unwrap(),
        layout("First", Orientation::Landscape, None).to_canonical_json()
    );

    // `job.updated` carries the full Job with a growing `rev`, ending in `succeeded`.
    let mut revs = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let frame: Value =
                serde_json::from_str(events.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            if frame["type"] == "job.updated" && frame["data"]["id"] == first.id.as_str() {
                assert_eq!(frame["device"], a.as_str());
                revs.push(frame["data"]["rev"].as_u64().unwrap());
                if frame["data"]["state"] == "succeeded" {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(revs.windows(2).all(|w| w[0] < w[1]), "{revs:?}");
    assert_eq!(*revs.last().unwrap(), done.rev);
}

#[tokio::test]
async fn a_cancelled_push_leaves_the_device_alone() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    td.hold_jobs();
    let job = push(&td, &a, &layout("First", Orientation::Landscape, None)).await;
    let job_path = || JobPath {
        job: job.id.clone(),
    };
    let cancelled = td.call::<Cancel>(job_path(), NoQuery, ()).await.unwrap();
    assert_eq!(cancelled.id, job.id);
    td.release_jobs();
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Cancelled);
    assert_eq!(done.error.unwrap().code, "job.cancelled");
    assert_eq!(td.deck(&a).layout(Orientation::Landscape), None);
    let (status, error) = failure(td.request::<Cancel>(job_path(), NoQuery, &())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (409, Some("job.not_cancellable"))
    );
    let missing = JobPath {
        job: "job_missing".into(),
    };
    let (status, error) = failure(td.request::<Cancel>(missing, NoQuery, &())).await;
    assert_eq!(
        (status, error["code"].as_str()),
        (404, Some("job.not_found"))
    );
}

#[tokio::test]
async fn a_push_sends_the_assets_the_device_lacks() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mock0001").await;
    let builtin = td_host::icons::builtin()[0].id.clone();
    let with_icon = layout(
        "Icons",
        Orientation::Landscape,
        Some(&format!("file:{builtin}")),
    );
    let pushed = report(&td.wait(&push(&td, &a, &with_icon).await).await);
    assert_eq!(pushed.assets.sent, std::slice::from_ref(&builtin));
    assert!(td.deck(&a).icons().contains(&builtin));
    // A second push sends nothing, and an id with no file is reported.
    let pushed = report(&td.wait(&push(&td, &a, &with_icon).await).await);
    assert!(pushed.assets.sent.is_empty());
    let missing = layout(
        "Missing",
        Orientation::Landscape,
        Some("file:0123456789abcdef"),
    );
    let pushed = report(&td.wait(&push(&td, &a, &missing).await).await);
    assert_eq!(pushed.assets.unavailable, ["0123456789abcdef"]);
}
