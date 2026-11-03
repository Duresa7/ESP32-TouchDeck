use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use td_core::live::LiveValue;
use td_host::integrations::{
    ControlError, Integration,
    media::{MAX_THUMBNAIL_BYTES, Media, Playback, fake::FakeMedia},
};

fn setup() -> (Media, FakeMedia) {
    let fake = FakeMedia::default();
    (Media::new(Arc::new(fake.clone())), fake)
}
fn value(media: &Media, stream: &str) -> LiveValue {
    media
        .control()
        .unwrap()
        .live()
        .into_iter()
        .find(|s| s.stream == stream)
        .unwrap()
        .value
}

#[test]
fn idle_has_all_four_streams_and_is_healthy() {
    let (media, _) = setup();
    assert_eq!(media.live().len(), 4);
    assert_eq!(value(&media, "state"), LiveValue::Number(0.0));
    assert_eq!(value(&media, "title"), LiveValue::Text(String::new()));
    assert_eq!(value(&media, "artist"), LiveValue::Text(String::new()));
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
    assert_eq!(media.summary().status.state, "ok");
}

#[test]
fn metadata_wakes_delivery_clears_cover_and_rejects_old_completions() {
    let (media, fake) = setup();
    let control = media.control().unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    control.subscribe(Arc::new(move || {
        count.fetch_add(1, Ordering::SeqCst);
    }));
    let first = fake.begin();
    fake.metadata(first, "First", "Artist", Playback::Playing);
    fake.cover(first, b"first cover");
    let LiveValue::Image(Some(id)) = value(&media, "cover") else {
        panic!("missing art")
    };
    assert_eq!(&*control.picture(&id).unwrap(), b"first cover");
    let second = fake.begin();
    fake.metadata(second, "Second", "Other", Playback::Paused);
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
    fake.metadata(first, "Late first", "Old", Playback::Playing);
    fake.cover(first, b"late art");
    assert_eq!(value(&media, "title"), LiveValue::Text("Second".into()));
    assert_eq!(value(&media, "state"), LiveValue::Number(1.0));
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
    fake.cover(second, b"first cover");
    assert_eq!(value(&media, "cover"), LiveValue::Image(Some(id)));
    assert!(wakes.load(Ordering::SeqCst) >= 4);
    fake.idle();
    assert_eq!(value(&media, "state"), LiveValue::Number(0.0));
    assert_eq!(value(&media, "title"), LiveValue::Text(String::new()));
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
}

#[test]
fn actions_validate_account_and_name_and_report_false_and_timeout() {
    let (media, fake) = setup();
    let control = media.control().unwrap();
    let wait = Duration::from_millis(20);
    assert_eq!(
        control.act("other", "next", wait),
        Err(ControlError::UnknownAccount)
    );
    assert_eq!(
        control.act("", "launch", wait),
        Err(ControlError::UnknownAction)
    );
    assert_eq!(
        control.set("", "state", 1.0),
        Err(ControlError::NotSettable)
    );
    assert_eq!(
        control.act("", "next", wait),
        Err(ControlError::Unavailable("Nothing is playing.".into()))
    );
    let generation = fake.begin();
    fake.metadata(generation, "Track", "Artist", Playback::Playing);
    for action in ["previous", "play_pause", "next"] {
        control.act("", action, wait).unwrap();
    }
    assert_eq!(fake.commands(), ["previous", "play_pause", "next"]);
    fake.respond(Some(false));
    assert_eq!(
        control.act("", "next", wait),
        Err(ControlError::Unavailable(
            "The player didn't respond.".into()
        ))
    );
    fake.respond(None);
    let start = Instant::now();
    assert!(control.act("", "next", wait).is_err());
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[test]
fn disable_stops_provider_and_old_generation_cannot_return_after_enable() {
    let (mut media, fake) = setup();
    let old = fake.feed();
    let generation = old.invalidate();
    media.configure(json!({"enabled":false})).unwrap();
    assert!(media.live().is_empty());
    assert!(!fake.running());
    media.configure(json!({"enabled":true})).unwrap();
    old.metadata(generation, "Stale", "", Playback::Playing);
    assert_eq!(value(&media, "title"), LiveValue::Text(String::new()));
    assert!(fake.running());
}

#[test]
fn metadata_and_cover_sizes_are_bounded_and_picture_cache_is_limited() {
    let (media, fake) = setup();
    let generation = fake.begin();
    fake.metadata(generation, &"é".repeat(10_000), "Artist", Playback::Playing);
    let LiveValue::Text(title) = value(&media, "title") else {
        panic!()
    };
    assert!(title.len() <= 4096);
    fake.cover(generation, &vec![0; MAX_THUMBNAIL_BYTES + 1]);
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
    fake.cover(generation, b"oldest");
    let LiveValue::Image(Some(oldest)) = value(&media, "cover") else {
        panic!()
    };
    for n in 0..8 {
        fake.cover(generation, &[n]);
    }
    assert!(media.control().unwrap().picture(&oldest).is_none());
}

#[test]
fn heartbeat_advances_samples_only_when_due_and_preset_matches_contract() {
    let (media, fake) = setup();
    let now = Instant::now();
    fake.heartbeat(now);
    let first = media.live();
    fake.heartbeat(now + Duration::from_secs(9));
    assert_eq!(media.live(), first);
    fake.heartbeat(now + Duration::from_secs(10));
    assert!(
        media
            .live()
            .iter()
            .all(|s| s.sample > first[0].sample && s.ttl_ms == 30_000)
    );
    let summary = media.summary();
    assert_eq!(summary.id, "media");
    assert!(summary.streams.iter().all(|s| !s.settable));
    let preset = serde_json::to_value(&summary.presets[0]).unwrap();
    assert_eq!(preset["id"], "now_playing");
    assert_eq!(preset["render"], "now-playing");
    assert_eq!(
        preset["streams"],
        json!({"title":"text","artist":"text","cover":"image","state":"number"})
    );
    assert_eq!(
        preset["action"],
        json!({"type":"widget","source":"media","field":"title","account":""})
    );
}

#[test]
fn enabling_media_keeps_sample_sequence_ahead_of_previous_delivery() {
    let (mut media, fake) = setup();
    let generation = fake.begin();
    for _ in 0..10 {
        fake.metadata(generation, "Track", "Artist", Playback::Playing);
    }
    let before = media.live()[0].sample;
    media.configure(json!({"enabled":false})).unwrap();
    media.configure(json!({"enabled":true})).unwrap();
    assert!(media.live().iter().all(|sample| sample.sample > before));
}

#[test]
fn new_generation_with_identical_metadata_does_not_keep_previous_art() {
    let (media, fake) = setup();
    let first = fake.begin();
    fake.metadata(first, "Track", "Artist", Playback::Playing);
    fake.cover(first, b"first art");
    let second = fake.begin();
    fake.metadata(second, "Track", "Artist", Playback::Playing);
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
    fake.cover(first, b"late first art");
    assert_eq!(value(&media, "cover"), LiveValue::Image(None));
}
