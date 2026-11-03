use super::*;
use crate::live::fake::FakeControl;
use crate::photos::encode::tests::noise_png;
use std::collections::BTreeMap;

#[test]
fn the_cover_thread_makes_only_covers_for_current_samples_then_wakes_a_round() {
    let shared = Arc::new(Shared::default());
    let fake = Arc::new(FakeControl::default());
    fake.add_picture("sha256:old", noise_png(100, 100, 1));
    fake.add_picture("sha256:new", noise_png(100, 100, 2));
    fake.publish(&[("cover", LiveValue::Image(Some("sha256:old".into())))]);
    let old = fake.samples().remove(0);
    fake.publish(&[("cover", LiveValue::Image(Some("sha256:new".into())))]);
    let new = fake.samples().remove(0);
    let source = StreamKey {
        integration: "media".into(),
        account: String::new(),
        stream: "cover".into(),
    };
    let job = |sample: &LiveSample| CoverJob {
        source: source.clone(),
        sample: sample.clone(),
        side: 64,
    };
    let controls: Controls = Arc::new(BTreeMap::from([(
        "media".to_owned(),
        fake.clone() as Arc<dyn td_host::integrations::Control>,
    )]));
    let (jobs, queue) = sync_channel(1);
    jobs.send(vec![job(&old), job(&new)]).unwrap();
    drop(jobs);
    make_covers(Arc::downgrade(&shared), controls, queue);
    let covers = shared.covers.lock().clone();
    assert_eq!(covers.len(), 1);
    let made = covers[&(source, new.sample, 64)].as_ref().unwrap();
    assert_eq!(image::load_from_memory(&made.bytes).unwrap().width(), 64);
    assert!(*shared.wake.lock());
}

#[test]
fn review_corrupt_picture_is_requested_once_across_rounds() {
    let shared = Arc::new(Shared::default());
    let fake = Arc::new(FakeControl::default());
    fake.add_picture("bad", b"broken".to_vec());
    fake.publish(&[("cover", LiveValue::Image(Some("bad".into())))]);
    let job = CoverJob {
        source: StreamKey {
            integration: "media".into(),
            account: String::new(),
            stream: "cover".into(),
        },
        sample: fake.samples().remove(0),
        side: 64,
    };
    let controls: Controls = Arc::new(BTreeMap::from([(
        "media".into(),
        fake.clone() as Arc<dyn td_host::integrations::Control>,
    )]));
    let (jobs, queue) = sync_channel(20);
    for _ in 0..20 {
        jobs.send(vec![job.clone()]).unwrap();
    }
    drop(jobs);
    make_covers(Arc::downgrade(&shared), controls, queue);
    assert_eq!(*fake.picture_calls.lock(), 1);
}

#[test]
fn a_tile_comes_from_the_shown_layout_of_the_same_session_and_never_from_a_replaced_one() {
    let live = Live(Arc::default());
    let layout =
        Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    let store = crate::store::LayoutStore::new(data.path().to_owned());
    store.put("d", &layout).unwrap();
    live.trust(store);
    live.0
        .shown
        .lock()
        .insert("d".into(), (7, Arc::new(layout)));
    let tile = live.tile("d", 7, "media", "volume").unwrap();
    assert!(tile.input);
    assert_eq!(live.tile("d", 7, "media", "nope"), None);
    assert_eq!(live.tile("d", 7, "other", "volume"), None);
    assert_eq!(live.tile("d", 8, "media", "volume"), None);
    assert_eq!(live.tile("e", 7, "media", "volume"), None);

    let busy = live.0.router.lock();
    live.layout_changed("d");
    assert_eq!(live.tile("d", 7, "media", "volume"), None);
    drop(busy);
}

#[test]
fn a_round_cannot_restore_a_layout_invalidated_during_delivery() {
    let live = Live(Arc::default());
    let layout =
        Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
    let data = tempfile::tempdir().unwrap();
    let store = crate::store::LayoutStore::new(data.path().to_owned());
    store.put("d", &layout).unwrap();
    live.trust(store);
    let old_round = HashMap::from([("d".into(), (7, Arc::new(layout)))]);
    live.layout_changed("d");
    publish_shown(&live.0, old_round);
    live.0.changed.lock().clear();
    assert!(live.tile("d", 7, "media", "volume").is_none());
}

#[test]
fn control_layout_assessment_tracks_authority_and_orientation() {
    use crate::devices::ControlLayout;
    for orientation in [
        td_core::Orientation::Landscape,
        td_core::Orientation::Portrait,
    ] {
        let live = Live(Arc::default());
        let mut layout =
            Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
        layout.orientation = orientation;
        let data = tempfile::tempdir().unwrap();
        let store = crate::store::LayoutStore::new(data.path().to_owned());
        live.trust(store.clone());
        live.0
            .shown
            .lock()
            .insert("d".into(), (7, Arc::new(layout.clone())));
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::NoPushedCopy {
                has_controls: true,
                ..
            }
        ));
        assert!(live.tile("d", 7, "media", "volume").is_none());
        store.put("d", &layout).unwrap();
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::Matches { .. }
        ));
        let path = data
            .path()
            .join("d")
            .join(format!("{}.json", crate::store::name(orientation)));
        std::fs::write(&path, serde_json::to_vec_pretty(&layout).unwrap()).unwrap();
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::Matches { .. }
        ));
        layout.name = "Changed".into();
        live.0
            .shown
            .lock()
            .insert("d".into(), (7, Arc::new(layout)));
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::Differs { .. }
        ));
        std::fs::write(&path, b"broken").unwrap();
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::NoPushedCopy { .. }
        ));
        std::fs::remove_file(path).unwrap();
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::NoPushedCopy { .. }
        ));
        assert!(matches!(
            live.0.assess("d", 8).0,
            ControlLayout::Unavailable { .. }
        ));
        live.layout_changed("d");
        assert!(matches!(
            live.0.assess("d", 7).0,
            ControlLayout::Unavailable { .. }
        ));
    }
}

#[test]
fn control_layout_without_controls_including_builtin() {
    let live = Live(Arc::default());
    let builtin = Layout::parse(include_bytes!(
        "../../../../../schema/examples/builtin.json"
    ))
    .unwrap();
    live.0
        .shown
        .lock()
        .insert("d".into(), (1, Arc::new(builtin)));
    assert!(matches!(
        live.0.assess("d", 1).0,
        ControlLayout::NoPushedCopy {
            has_controls: false,
            ..
        }
    ));
}

#[test]
fn control_layout_publication_rejects_a_round_before_invalidation() {
    use crate::devices::{BoardKind, Config, DeviceId};
    let devices = DeviceManager::new(crate::api::Events::default());
    let config = Config::mock(None);
    config
        .mock
        .as_ref()
        .unwrap()
        .0
        .plug("mock0001", BoardKind::Device);
    devices.start(config);
    let id = DeviceId::parse("mock0001").unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let session = loop {
        if let Some(d) = devices.get(&id)
            && let LinkState::Connected { since_ms, .. } = d.link
        {
            break since_ms;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    };
    let live = Live(Arc::default());
    live.0.devices.set(devices.clone()).ok().unwrap();
    let data = tempfile::tempdir().unwrap();
    let store = crate::store::LayoutStore::new(data.path().to_owned());
    let layout =
        Layout::parse(include_bytes!("../../../../../schema/examples/media.json")).unwrap();
    store.put(&id, &layout).unwrap();
    live.trust(store);
    live.0
        .shown
        .lock()
        .insert(id.to_string(), (session, Arc::new(layout)));
    let old = live.generation();
    publish_status(&live.0, &devices, old);
    assert!(matches!(
        devices.get(&id).unwrap().control_layout,
        ControlLayout::Matches { .. }
    ));
    let rev = devices.get(&id).unwrap().rev;
    publish_status(&live.0, &devices, old);
    assert_eq!(devices.get(&id).unwrap().rev, rev);
    live.layout_changed(&id);
    publish_status(&live.0, &devices, old);
    assert_eq!(
        devices.get(&id).unwrap().control_layout,
        ControlLayout::default()
    );
    devices.disconnect(&id);
    devices.control_layout(
        &id,
        Some(session),
        ControlLayout::Matches {
            orientation: td_core::Orientation::Landscape,
            has_controls: true,
        },
    );
    assert_eq!(
        devices.get(&id).unwrap().control_layout,
        ControlLayout::default()
    );
    devices.stop();
}
