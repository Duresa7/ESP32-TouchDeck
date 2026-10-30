//! The photo frame through the API: its settings on the Device and on this PC, the Device's
//! clock, and pictures streamed from the chosen folder while the Device is idle.
use serde_json::{Value, json};
use std::{io::Cursor, time::Duration};
use td_companion::{
    api::NoQuery,
    devices::DeviceId,
    features::devices::{
        DevicePath, DeviceSettingsPatch, PhotoFramePatch, PhotoFrameSettings, SettingsGet,
        SettingsPatch,
    },
    testing::TestCompanion,
};
use td_link::photo::IdleSettings;

fn path(id: &DeviceId) -> DevicePath {
    DevicePath { device: id.clone() }
}

/// A generated picture, so no photo of anyone is in the repository.
fn picture(seed: u8) -> Vec<u8> {
    let picture = image::RgbImage::from_fn(96, 64, |x, y| {
        image::Rgb([(x as u8) ^ seed, (y as u8).wrapping_mul(3), seed])
    });
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn frame(patch: PhotoFramePatch) -> DeviceSettingsPatch {
    DeviceSettingsPatch {
        photo_frame: Some(patch),
        ..Default::default()
    }
}

async fn patch_error(td: &TestCompanion, device: &str, body: Value) -> (u16, Value) {
    let response = td
        .raw()
        .request(
            reqwest::Method::PATCH,
            &format!("/api/v1/devices/{device}/settings"),
        )
        .bearer_auth(&td.token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body: Value = response.json().await.unwrap();
    (status, body["error"].clone())
}

#[tokio::test]
async fn photo_frame_settings_round_trip_through_the_device_and_this_pc() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    let settings = td
        .call::<SettingsGet>(path(&id), NoQuery, ())
        .await
        .unwrap();
    assert_eq!(
        settings.photo_frame,
        Some(PhotoFrameSettings {
            enabled: false,
            timeout_s: 60,
            interval_s: 15,
            folder: None,
        })
    );

    let folder = tempfile::tempdir().unwrap();
    let chosen = folder.path().to_string_lossy().into_owned();
    let patched = td
        .call::<SettingsPatch>(
            path(&id),
            NoQuery,
            frame(PhotoFramePatch {
                enabled: Some(true),
                timeout_s: Some(300),
                folder: Some(chosen.clone()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    let expected = PhotoFrameSettings {
        enabled: true,
        timeout_s: 300,
        interval_s: 15,
        folder: Some(chosen.clone()),
    };
    assert_eq!(patched.settings.photo_frame.as_ref(), Some(&expected));
    assert!(!patched.restart_required);
    assert_eq!(
        td.deck("mock0001").idle_settings(),
        IdleSettings {
            enabled: true,
            timeout_s: 300,
            interval_s: 15,
        }
    );
    let read = td
        .call::<SettingsGet>(path(&id), NoQuery, ())
        .await
        .unwrap();
    assert_eq!(read.photo_frame, Some(expected));

    let cleared = td
        .call::<SettingsPatch>(
            path(&id),
            NoQuery,
            frame(PhotoFramePatch {
                interval_s: Some(30),
                folder: Some(String::new()),
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .settings
        .photo_frame
        .unwrap();
    assert_eq!((cleared.interval_s, cleared.folder), (30, None));

    let missing = folder.path().join("gone").to_string_lossy().into_owned();
    for body in [
        json!({"photo_frame": {"timeout_s": 9}}),
        json!({"photo_frame": {"timeout_s": 86_401}}),
        json!({"photo_frame": {"interval_s": 4}}),
        json!({"photo_frame": {"interval_s": 3601}}),
        json!({"photo_frame": {"folder": "Pictures"}}),
        json!({"photo_frame": {"folder": missing}}),
    ] {
        let (status, error) = patch_error(&td, "mock0001", body.clone()).await;
        assert_eq!(
            (status, error["code"].as_str()),
            (400, Some("request.invalid")),
            "{body}"
        );
    }
    assert_eq!(td.deck("mock0001").idle_settings().interval_s, 30);
}

#[tokio::test]
async fn v1_firmware_has_no_photo_frame() {
    let td = TestCompanion::start().await;
    let id = td.plug_device_v1("mock0003").await;
    let settings = td
        .call::<SettingsGet>(path(&id), NoQuery, ())
        .await
        .unwrap();
    assert_eq!(settings.photo_frame, None);
    let (status, error) =
        patch_error(&td, "mock0003", json!({"photo_frame": {"enabled": true}})).await;
    assert_eq!(status, 409);
    assert_eq!(error["code"], "device.needs_firmware");
    assert_eq!(error["details"]["field"], "photo_frame");
}

#[tokio::test]
async fn the_folder_streams_to_the_device_only_while_it_is_idle() {
    let td = TestCompanion::start().await;
    let id = td.plug_device("mock0001").await;
    let deck = td.deck("mock0001");
    td.until(|_| deck.clock().is_some()).await;
    let (unix_s, _) = deck.clock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(now.abs_diff(unix_s) < 60, "the clock is this PC's time");

    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("a.png"), picture(1)).unwrap();
    std::fs::write(folder.path().join("notes.txt"), b"not a picture").unwrap();
    let turn = |enabled: bool| {
        td.call::<SettingsPatch>(
            path(&id),
            NoQuery,
            frame(PhotoFramePatch {
                enabled: Some(enabled),
                folder: Some(folder.path().to_string_lossy().into_owned()),
                ..Default::default()
            }),
        )
    };
    let puts = || {
        deck.received()
            .iter()
            .filter(|m| m["m"] == "v2.photo.put")
            .count()
    };
    turn(true).await.unwrap();
    td.until(|_| puts() >= 2).await;
    assert!(deck.photos().is_empty(), "an awake Device gets no picture");
    assert_eq!(deck.chunk_bytes(), 0);

    turn(false).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = puts();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        puts(),
        before,
        "turning the photo frame off stops the stream"
    );

    turn(true).await.unwrap();
    deck.set_idle(true);
    td.until(|_| !deck.photos().is_empty()).await;
    let shown =
        image::load_from_memory_with_format(&deck.photos()[0], image::ImageFormat::Jpeg).unwrap();
    assert_eq!((shown.width(), shown.height()), (96, 64));
}
