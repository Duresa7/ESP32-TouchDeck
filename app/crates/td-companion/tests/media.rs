use serde_json::json;
use td_companion::{
    api::NoQuery,
    features::integrations::{Get, IntegrationPath},
    testing::TestCompanion,
};

#[tokio::test]
async fn media_hook_scripts_the_fake_without_exposing_metadata_in_product_api() {
    let td = TestCompanion::start().await;
    let response = td.raw().post("/api/v1/test-hooks/media").bearer_auth(&td.token)
        .json(&json!({"session":{"title":"Private fixture track","artist":"Fixture artist","state":2,"cover":"aW1hZ2U="}})).send().await.unwrap();
    assert_eq!(response.status(), 204);
    let view = td
        .call::<Get>(
            IntegrationPath {
                integration: "media".into(),
            },
            NoQuery,
            (),
        )
        .await
        .unwrap();
    assert_eq!(view.summary.status.state, "ok");
    assert_eq!(view.summary.presets[0].id, "now_playing");
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("Private fixture track")
    );
    let response = td
        .raw()
        .post("/api/v1/test-hooks/media")
        .bearer_auth(&td.token)
        .json(&json!({"session":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
}

#[tokio::test]
async fn media_hook_rejects_hostile_input_and_release_router_has_no_hook() {
    let td = TestCompanion::start().await;
    for body in [
        json!({"session":{"title":"track","artist":"artist","state":4}}),
        json!({"session":{"title":"track","artist":"artist","state":2,"cover":"https://example.invalid/cover.png"}}),
        json!({"session":{"title":"x".repeat(4097),"artist":"artist","state":2}}),
        json!({"session":{"title":"track","artist":"artist","state":2,"extra":true}}),
    ] {
        let response = td
            .raw()
            .post("/api/v1/test-hooks/media")
            .bearer_auth(&td.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    let td = TestCompanion::start_release_router().await;
    let response = td
        .raw()
        .post("/api/v1/test-hooks/media")
        .bearer_auth(&td.token)
        .json(&json!({"session":null}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}
