use serde_json::{Value, json};
use td_companion::{
    api::NoQuery,
    features::{
        assets::{AssetPath, Create, CreateAsset, File, OnDevice},
        devices::DevicePath,
        layouts::{LayoutBody, LayoutPath, Push},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_core::{Layout, Orientation};

fn png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(128, 64, image::Rgba([255, 0, 0, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}
async fn upload(td: &TestCompanion) -> Value {
    td.raw()
        .post("/api/v1/uploads")
        .bearer_auth(&td.token)
        .header("content-type", "image/png")
        .body(png())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn render(td: &TestCompanion, id: &str, fit: &str) -> td_companion::jobs::Job {
    let body: CreateAsset =
        serde_json::from_value(json!({"source":{"kind":"upload","upload":id},"fit":fit})).unwrap();
    let job = td.call::<Create>((), NoQuery, body).await.unwrap();
    td.wait(&job).await
}
#[tokio::test]
async fn upload_fits_and_is_consumed_once() {
    let td = TestCompanion::start().await;
    for fit in ["contain", "cover"] {
        let uploaded = upload(&td).await;
        let id = uploaded["id"].as_str().unwrap();
        let done = render(&td, id, fit).await;
        assert_eq!(done.state, JobState::Succeeded, "{done:?}");
        let result = done.result.unwrap();
        let hash = result["id"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
        let file = td
            .request::<File>(AssetPath { asset: hash.into() }, NoQuery, &())
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(td_link::asset::asset_id(&file), hash);
        let alpha = &file[td_host::icon_pack::HEADER_BYTES + td_host::icon_pack::COLOR_BYTES..];
        if fit == "cover" {
            assert!(alpha.iter().all(|a| *a == 255));
        } else {
            assert_eq!(alpha.iter().filter(|a| **a == 255).count(), 2048);
        }
        assert_eq!(
            render(&td, id, fit).await.error.unwrap().code,
            "upload.not_found"
        );
    }
}
#[tokio::test]
async fn invalid_uploads_release_capacity() {
    let td = TestCompanion::start().await;
    for _ in 0..10 {
        let response = td
            .raw()
            .post("/api/v1/uploads")
            .bearer_auth(&td.token)
            .header("content-type", "image/png")
            .body("not an image")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    upload(&td).await;
}
#[tokio::test]
async fn second_layout_push_sends_no_asset() {
    let td = TestCompanion::start().await;
    let device = td.plug_device("mock0001").await;
    let uploaded = upload(&td).await;
    let done = render(&td, uploaded["id"].as_str().unwrap(), "cover").await;
    assert_eq!(done.state, JobState::Succeeded, "{done:?}");
    let hash = done.result.unwrap()["id"].as_str().unwrap().to_owned();
    let layout = Layout::parse(&serde_json::to_vec(&json!({"schema":2,"name":"Assets","orientation":"landscape","pages":[{"id":"main","title":"Main","kind":"grid","rows":1,"cols":1,"tiles":[{"id":"a","label":"A","row":0,"col":0,"rowSpan":1,"colSpan":1,"icon":null,"asset":format!("sha256:{hash}"),"action":{"type":"none"}}]}]})).unwrap()).unwrap();
    for count in [1, 0] {
        let before = td.deck("mock0001").chunk_bytes();
        let job = td
            .call::<Push>(
                LayoutPath {
                    device: device.to_string(),
                    orientation: Orientation::Landscape,
                },
                NoQuery,
                LayoutBody::new(&layout),
            )
            .await
            .unwrap();
        let done = td.wait(&job).await;
        assert_eq!(done.state, JobState::Succeeded, "{done:?}");
        assert_eq!(
            td.deck("mock0001").chunk_bytes() - before,
            layout.to_canonical_json().len() + count * td_host::icon_pack::FILE_BYTES
        );
        assert_eq!(
            done.result.unwrap()["assets"]["sent"]
                .as_array()
                .unwrap()
                .len(),
            count
        );
    }
    let stored = td
        .call::<OnDevice>(DevicePath { device }, NoQuery, ())
        .await
        .unwrap();
    assert_eq!(stored.ids, vec![hash]);
    assert!(stored.storage_free > 0);
}

#[tokio::test]
async fn uploads_belong_to_the_creating_session() {
    let mut td = TestCompanion::start().await;
    let uploaded = upload(&td).await;
    let owner = td.token.clone();
    let session: Value = td
        .raw()
        .post("/api/v1/auth/sessions")
        .bearer_auth(&td.pairing_token)
        .json(&json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    td.token = session["token"].as_str().unwrap().into();
    let id = uploaded["id"].as_str().unwrap();
    assert_eq!(
        render(&td, id, "contain").await.error.unwrap().code,
        "upload.not_found"
    );
    td.token = owner;
    assert_eq!(render(&td, id, "contain").await.state, JobState::Succeeded);
}

#[tokio::test]
async fn pending_upload_limit_recovers_after_consumption() {
    let td = TestCompanion::start().await;
    let first = upload(&td).await;
    for _ in 1..8 {
        upload(&td).await;
    }
    let response = td
        .raw()
        .post("/api/v1/uploads")
        .bearer_auth(&td.token)
        .header("content-type", "image/png")
        .body(png())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 429);
    assert_eq!(
        render(&td, first["id"].as_str().unwrap(), "cover")
            .await
            .state,
        JobState::Succeeded
    );
    upload(&td).await;
}
