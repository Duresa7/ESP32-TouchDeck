//! An app icon rendered through `assets.create`. On Linux the test points the desktop entry
//! lookup at a fixture, so it runs in its own test binary.
use serde_json::json;
use td_companion::{
    api::NoQuery,
    features::assets::{AssetPath, AssetRef, Create, CreateAsset, File},
    jobs::JobState,
    testing::TestCompanion,
};

/// A program the runner has, with its icon.
fn program() -> String {
    #[cfg(windows)]
    return r"C:\Windows\notepad.exe".into();
    #[cfg(not(windows))]
    {
        let dir = std::env::temp_dir().join(format!("td-app-icon-{}", std::process::id()));
        let apps = dir.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        let icon = dir.join("fixture.svg");
        std::fs::write(
            &icon,
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect width="16" height="16" fill="#2a6"/></svg>"##,
        )
        .unwrap();
        let program = dir.join("fixture-app");
        std::fs::write(
            apps.join("fixture.desktop"),
            format!(
                "[Desktop Entry]\nName=Fixture\nExec={} %U\nIcon={}\n",
                program.display(),
                icon.display()
            ),
        )
        .unwrap();
        // SAFETY: this test binary runs this one test, and sets the variable before any thread reads it.
        unsafe { std::env::set_var("XDG_DATA_HOME", &dir) };
        program.display().to_string()
    }
}

#[tokio::test]
async fn an_app_icon_becomes_an_asset() {
    let program = program();
    let td = TestCompanion::start().await;
    for fit in ["contain", "cover"] {
        let body: CreateAsset = serde_json::from_value(json!({
            "source": {"kind": "app", "path": program}, "fit": fit
        }))
        .unwrap();
        let job = td.call::<Create>((), NoQuery, body).await.unwrap();
        let done = td.wait(&job).await;
        assert_eq!(done.state, JobState::Succeeded, "{done:?}");
        assert_eq!(done.progress, json!({"frames_done": 1, "frames_total": 1}));
        let asset: AssetRef = serde_json::from_value(done.result.unwrap()).unwrap();
        assert_eq!(
            (asset.kind.as_str(), asset.width, asset.height),
            ("tdi1", 64, 64)
        );
        let response = td
            .request::<File>(AssetPath { asset: asset.id }, NoQuery, &())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.bytes().await.unwrap();
        assert_eq!(bytes.len() as u64, asset.bytes);
        let alpha = &bytes[td_host::icon_pack::HEADER_BYTES + td_host::icon_pack::COLOR_BYTES..];
        assert!(alpha.iter().filter(|a| **a == 255).count() >= 500);
    }
}
