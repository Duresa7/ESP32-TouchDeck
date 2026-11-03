use serde_json::json;
use td_companion::testing::TestCompanion;

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
