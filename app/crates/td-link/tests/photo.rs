use std::time::Duration;
use td_link::photo::{
    IdleSettings, PHOTO_BYTES_MAX, PhotoError, clock_set, idle_get, idle_set, put_photo,
};
use td_link::{AssetReason, MockDeck, Session, SessionConfig};

const TIMEOUT: Duration = Duration::from_secs(1);

fn open(device: &MockDeck) -> Session {
    let session = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap()
    .0;
    assert_eq!(session.identity(TIMEOUT).unwrap().protocol, 2);
    session
}

fn jpeg(seed: u8, bytes: usize) -> Vec<u8> {
    let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0];
    data.extend((4..bytes).map(|i| (i as u8).wrapping_mul(17).wrapping_add(seed)));
    data
}

fn enable(session: &Session) {
    let on = IdleSettings {
        enabled: true,
        ..IdleSettings::default()
    };
    assert_eq!(idle_set(session, on).unwrap(), on);
}

fn refused(result: Result<impl std::fmt::Debug, PhotoError>) -> (AssetReason, Option<u64>) {
    match result.unwrap_err() {
        PhotoError::Refused {
            reason, retry_ms, ..
        } => (reason, retry_ms),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_picture_is_shown_only_while_the_device_is_idle() {
    let device = MockDeck::v2();
    let session = open(&device);
    let picture = jpeg(1, 5000);
    assert_eq!(
        refused(put_photo(&session, &picture, PHOTO_BYTES_MAX)).0,
        AssetReason::Idle
    );
    enable(&session);
    assert_eq!(
        refused(put_photo(&session, &picture, PHOTO_BYTES_MAX)).0,
        AssetReason::Idle,
        "enabled but awake"
    );
    device.set_idle(true);
    let shown = put_photo(&session, &picture, PHOTO_BYTES_MAX).unwrap();
    assert_eq!((shown.bytes, shown.chunks), (5000, 5));
    assert_eq!(device.photos(), vec![picture]);
}

#[test]
fn a_second_picture_inside_the_interval_is_paced_with_retry_ms() {
    let device = MockDeck::v2();
    let session = open(&device);
    enable(&session);
    device.set_idle(true);
    device.set_photo_interval(Duration::from_millis(400));
    put_photo(&session, &jpeg(1, 2000), PHOTO_BYTES_MAX).unwrap();
    let (reason, retry_ms) = refused(put_photo(&session, &jpeg(2, 2000), PHOTO_BYTES_MAX));
    assert_eq!(reason, AssetReason::Rate);
    assert!(
        retry_ms.is_some_and(|ms| (1..=400).contains(&ms)),
        "{retry_ms:?}"
    );
    std::thread::sleep(Duration::from_millis(retry_ms.unwrap()));
    put_photo(&session, &jpeg(2, 2000), PHOTO_BYTES_MAX).unwrap();
    assert_eq!(device.photos().len(), 2);
}

#[test]
fn a_picture_over_the_limit_never_leaves_the_computer() {
    let device = MockDeck::v2();
    let session = open(&device);
    let before = device.received().len();
    for (data, limit) in [
        (jpeg(1, 2001), 2000),
        (jpeg(1, PHOTO_BYTES_MAX + 1), usize::MAX),
        (Vec::new(), PHOTO_BYTES_MAX),
    ] {
        assert!(matches!(
            put_photo(&session, &data, limit),
            Err(PhotoError::Size { .. })
        ));
    }
    assert_eq!(device.received().len(), before);
    assert_eq!(device.chunk_bytes(), 0);
}

#[test]
fn a_busy_slot_and_a_refused_end_report_their_reasons() {
    let device = MockDeck::v2();
    let session = open(&device);
    enable(&session);
    device.set_idle(true);
    device.refuse_photos([("busy", None), ("rate", Some(250))]);
    assert_eq!(
        refused(put_photo(&session, &jpeg(1, 100), PHOTO_BYTES_MAX)),
        (AssetReason::Busy, None)
    );
    assert_eq!(
        refused(put_photo(&session, &jpeg(1, 100), PHOTO_BYTES_MAX)),
        (AssetReason::Rate, Some(250))
    );
    let not_a_jpeg = vec![7u8; 100];
    assert_eq!(
        refused(put_photo(&session, &not_a_jpeg, PHOTO_BYTES_MAX)).0,
        AssetReason::Format
    );
    assert!(device.photos().is_empty());
}

#[test]
fn idle_settings_round_trip_and_reject_out_of_range_values() {
    let device = MockDeck::v2();
    let session = open(&device);
    assert_eq!(idle_get(&session).unwrap(), IdleSettings::default());
    let chosen = IdleSettings {
        enabled: true,
        timeout_s: 300,
        interval_s: 30,
    };
    assert_eq!(idle_set(&session, chosen).unwrap(), chosen);
    assert_eq!(idle_get(&session).unwrap(), chosen);
    assert_eq!(device.idle_settings(), chosen);
    for bad in [
        IdleSettings {
            timeout_s: 9,
            ..chosen
        },
        IdleSettings {
            interval_s: 3601,
            ..chosen
        },
    ] {
        assert!(matches!(
            idle_set(&session, bad),
            Err(PhotoError::Invalid(_))
        ));
    }
    assert_eq!(device.idle_settings(), chosen);
}

#[test]
fn the_clock_is_set_within_the_protocol_range() {
    let device = MockDeck::v2();
    let session = open(&device);
    clock_set(&session, 1_700_000_000, -300).unwrap();
    assert_eq!(device.clock(), Some((1_700_000_000, -300)));
    assert!(clock_set(&session, 4_102_444_800, 0).is_err());
    assert!(clock_set(&session, 0, 841).is_err());
    assert_eq!(device.clock(), Some((1_700_000_000, -300)));
}

#[test]
fn a_v1_device_refuses_photo_requests() {
    let device = MockDeck::new();
    let session = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap()
    .0;
    assert!(idle_get(&session).is_err());
    assert!(put_photo(&session, &jpeg(1, 100), PHOTO_BYTES_MAX).is_err());
}
