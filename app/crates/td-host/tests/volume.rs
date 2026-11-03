use serde_json::json;
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use td_core::live::LiveValue;
use td_host::integrations::{
    Control, ControlError, Integration,
    volume::{Snapshot, Volume, fake::FakeVolume},
};

fn setup() -> (Volume, FakeVolume, Arc<dyn Control>) {
    let fake = FakeVolume::new(Snapshot {
        level: 0.25,
        muted: true,
    });
    let volume = Volume::new(fake.factory(), None).unwrap();
    let control = volume.control().unwrap();
    wait(|| !control.live().is_empty());
    (volume, fake, control)
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "Volume did not update");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn value(control: &dyn Control, stream: &str) -> Option<f64> {
    control.live().into_iter().find_map(|sample| {
        if sample.stream == stream
            && let LiveValue::Number(value) = sample.value
        {
            return Some(value);
        }
        None
    })
}

#[test]
fn volume_descriptor_offers_a_draggable_bar_and_only_level_is_settable() {
    let (volume, _, _) = setup();
    let summary = volume.summary();
    assert_eq!(summary.id, "volume");
    assert_eq!(summary.actions, ["mute"]);
    assert_eq!(
        summary
            .streams
            .iter()
            .filter(|s| s.settable)
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["level"]
    );
    let preset = serde_json::to_value(&summary.presets[0]).unwrap();
    assert_eq!(
        preset,
        json!({"id":"volume_bar","name":"Volume bar","action":{"type":"widget","source":"volume","account":"","field":"level"},"render":"bar","streams":{"value":"number","muted":"number"},"input":true})
    );
}

#[test]
fn external_notifications_publish_both_streams_and_wake_without_a_registry_tick() {
    let (_volume, fake, control) = setup();
    let (tx, rx) = mpsc::channel();
    control.subscribe(Arc::new(move || {
        let _ = tx.send(());
    }));
    fake.publish(Some(Snapshot {
        level: 0.7,
        muted: false,
    }))
    .unwrap();
    rx.recv_timeout(Duration::from_secs(1)).unwrap();
    wait(|| value(&*control, "level") == Some(0.7));
    assert_eq!(value(&*control, "muted"), Some(0.0));
    assert!(
        control
            .live()
            .iter()
            .all(|s| s.account.is_empty() && s.ttl_ms == 30_000)
    );
}

#[test]
fn level_above_zero_unmutes_zero_preserves_mute_and_mute_toggles() {
    let (_volume, fake, control) = setup();
    control.set("", "level", 0.0).unwrap();
    wait(|| fake.snapshot().unwrap().level == 0.0);
    assert!(fake.snapshot().unwrap().muted);
    control.set("", "level", 0.8).unwrap();
    wait(|| value(&*control, "level") == Some(0.8));
    assert!(!fake.snapshot().unwrap().muted);
    control.act("", "mute", Duration::from_secs(1)).unwrap();
    assert!(fake.snapshot().unwrap().muted);
    control.act("", "mute", Duration::from_secs(1)).unwrap();
    assert!(!fake.snapshot().unwrap().muted);
}

#[test]
fn hostile_controls_never_reach_the_provider() {
    let (_volume, fake, control) = setup();
    for level in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
        assert_eq!(
            control.set("", "level", level),
            Err(ControlError::OutOfRange)
        );
    }
    assert_eq!(
        control.set("other", "level", 0.4),
        Err(ControlError::UnknownAccount)
    );
    assert_eq!(
        control.set("", "muted", 0.4),
        Err(ControlError::NotSettable)
    );
    assert_eq!(
        control.act("other", "mute", Duration::from_secs(1)),
        Err(ControlError::UnknownAccount)
    );
    assert_eq!(
        control.act("", "next", Duration::from_secs(1)),
        Err(ControlError::UnknownAction)
    );
    assert!(control.act("", "mute", Duration::ZERO).is_err());
    assert_eq!(fake.sets(), 0);
    assert_eq!(
        fake.snapshot().unwrap(),
        Snapshot {
            level: 0.25,
            muted: true
        }
    );
}

#[test]
fn missing_endpoint_clears_samples_and_a_new_default_recovers() {
    let (volume, fake, control) = setup();
    fake.publish(None).unwrap();
    wait(|| control.live().is_empty());
    assert_eq!(
        volume.summary().status.message.as_deref(),
        Some("No playback device.")
    );
    assert!(control.set("", "level", 0.5).is_err());
    fake.publish(Some(Snapshot {
        level: 0.4,
        muted: false,
    }))
    .unwrap();
    wait(|| value(&*control, "level") == Some(0.4));
    assert_eq!(volume.summary().status.state, "ok");
}

#[test]
fn disabling_releases_provider_clears_samples_and_reenabling_reads_current_state() {
    let (mut volume, fake, control) = setup();
    assert_eq!(fake.active(), 1);
    volume.configure(json!({"enabled": false})).unwrap();
    assert_eq!(fake.active(), 0);
    assert!(control.live().is_empty());
    assert!(control.set("", "level", 0.8).is_err());
    fake.publish(Some(Snapshot {
        level: 0.6,
        muted: false,
    }))
    .unwrap();
    assert!(control.live().is_empty());
    volume.configure(json!({"enabled": true})).unwrap();
    wait(|| value(&*control, "level") == Some(0.6));
    drop(volume);
    assert_eq!(fake.active(), 0);
    assert!(control.live().is_empty());
}

#[test]
fn settings_persist_and_invalid_config_does_not_disable_volume() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("volume.json");
    let fake = FakeVolume::default();
    let mut volume = Volume::new(fake.factory(), Some(file.clone())).unwrap();
    assert!(
        volume
            .configure(json!({"enabled":false,"endpoint":"other"}))
            .is_err()
    );
    assert!(volume.summary().enabled);
    volume.configure(json!({"enabled":false})).unwrap();
    drop(volume);
    let volume = Volume::new(fake.factory(), Some(file)).unwrap();
    assert!(!volume.summary().enabled);
    assert!(volume.live().is_empty());
    assert_eq!(fake.active(), 0);
}

struct ReadGate {
    needs_rebind: std::sync::atomic::AtomicBool,
    stale_sets: std::sync::atomic::AtomicUsize,
    block: std::sync::atomic::AtomicBool,
    entered: mpsc::SyncSender<()>,
    release: std::sync::Mutex<mpsc::Receiver<()>>,
}
struct GatedProvider {
    inner: Box<dyn td_host::integrations::volume::VolumeProvider>,
    gate: Arc<ReadGate>,
}
impl td_host::integrations::volume::VolumeProvider for GatedProvider {
    fn read(&mut self, rebind: bool) -> Result<Snapshot, td_host::integrations::volume::Failure> {
        if rebind {
            self.gate
                .needs_rebind
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        if self
            .gate
            .block
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.gate.entered.send(()).unwrap();
            self.gate
                .release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .unwrap();
        }
        self.inner.read(rebind)
    }
    fn set_level(&mut self, level: f64) -> Result<(), td_host::integrations::volume::Failure> {
        if self
            .gate
            .needs_rebind
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.gate
                .stale_sets
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        self.inner.set_level(level)
    }
    fn set_mute(&mut self, muted: bool) -> Result<(), td_host::integrations::volume::Failure> {
        self.inner.set_mute(muted)
    }
}

type GatedVolume = (
    Volume,
    FakeVolume,
    Arc<dyn Control>,
    Arc<ReadGate>,
    mpsc::Receiver<()>,
    mpsc::SyncSender<()>,
);
fn gated() -> GatedVolume {
    let fake = FakeVolume::default();
    let factory = fake.factory();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let gate = Arc::new(ReadGate {
        needs_rebind: false.into(),
        stale_sets: 0.into(),
        block: false.into(),
        entered: entered_tx,
        release: std::sync::Mutex::new(release_rx),
    });
    let worker_gate = gate.clone();
    let volume = Volume::new(
        Arc::new(move |notifier| {
            Ok(Box::new(GatedProvider {
                inner: factory(notifier)?,
                gate: worker_gate.clone(),
            }))
        }),
        None,
    )
    .unwrap();
    let control = volume.control().unwrap();
    wait(|| !control.live().is_empty());
    (volume, fake, control, gate, entered_rx, release_tx)
}

#[test]
fn a_mute_that_expires_during_readback_does_not_run_later() {
    let (_volume, fake, control, gate, entered, release) = gated();
    gate.block.store(true, std::sync::atomic::Ordering::SeqCst);
    let caller = std::thread::spawn(move || control.act("", "mute", Duration::from_millis(100)));
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let result = caller.join().unwrap();
    release.send(()).unwrap();
    assert!(result.is_err());
    std::thread::sleep(Duration::from_millis(30));
    assert!(!fake.snapshot().unwrap().muted);
}

#[test]
fn a_drag_flood_is_coalesced_and_a_default_change_discards_queued_input() {
    let (_volume, fake, control, gate, entered, release) = gated();
    gate.block.store(true, std::sync::atomic::Ordering::SeqCst);
    fake.publish(Some(Snapshot {
        level: 0.4,
        muted: false,
    }))
    .unwrap();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    for n in 0..10_000 {
        control.set("", "level", f64::from(n) / 10_000.0).unwrap();
    }
    release.send(()).unwrap();
    wait(|| fake.sets() == 1);
    assert_eq!(fake.snapshot().unwrap().level, 0.9999);
    gate.block.store(true, std::sync::atomic::Ordering::SeqCst);
    fake.publish(Some(Snapshot {
        level: 0.4,
        muted: false,
    }))
    .unwrap();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    control.set("", "level", 0.9).unwrap();
    fake.publish(None).unwrap();
    fake.publish(Some(Snapshot {
        level: 0.2,
        muted: true,
    }))
    .unwrap();
    release.send(()).unwrap();
    wait(|| value(&*control, "level") == Some(0.2));
    assert_eq!(fake.sets(), 1);
    assert!(fake.snapshot().unwrap().muted);
}

#[test]
fn quiet_volume_heartbeats_without_registry_ticks() {
    let (_volume, _fake, control) = setup();
    let first = control.live()[0].sample;
    let (tx, rx) = mpsc::channel();
    control.subscribe(Arc::new(move || {
        let _ = tx.send(());
    }));
    rx.recv_timeout(Duration::from_secs(12)).unwrap();
    assert!(control.live()[0].sample > first);
    assert_eq!(value(&*control, "level"), Some(0.25));
}

#[test]
fn input_after_a_default_change_waits_for_the_new_endpoint_binding() {
    let (_volume, fake, control, gate, entered, release) = gated();
    gate.block.store(true, std::sync::atomic::Ordering::SeqCst);
    fake.publish(Some(Snapshot {
        level: 0.4,
        muted: false,
    }))
    .unwrap();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    gate.needs_rebind
        .store(true, std::sync::atomic::Ordering::SeqCst);
    fake.publish(Some(Snapshot {
        level: 0.2,
        muted: false,
    }))
    .unwrap();
    control.set("", "level", 0.7).unwrap();
    release.send(()).unwrap();
    wait(|| fake.sets() == 1);
    assert_eq!(gate.stale_sets.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(fake.snapshot().unwrap().level, 0.7);
}

#[test]
fn a_default_change_during_a_mute_read_binds_before_the_next_level() {
    let (_volume, fake, control, gate, entered, release) = gated();
    gate.block.store(true, std::sync::atomic::Ordering::SeqCst);
    let caller_control = control.clone();
    let caller = std::thread::spawn(move || caller_control.act("", "mute", Duration::from_secs(2)));
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    gate.needs_rebind
        .store(true, std::sync::atomic::Ordering::SeqCst);
    fake.publish(Some(Snapshot {
        level: 0.2,
        muted: true,
    }))
    .unwrap();
    control.set("", "level", 0.7).unwrap();
    release.send(()).unwrap();
    assert!(caller.join().unwrap().is_err());
    wait(|| fake.sets() == 1);
    assert_eq!(gate.stale_sets.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(fake.snapshot().unwrap().level, 0.7);
}

#[test]
fn repeated_read_failures_back_off_and_bound_subscriber_wakes() {
    let (_volume, fake, control) = setup();
    let (tx, rx) = mpsc::channel();
    control.subscribe(Arc::new(move || {
        let _ = tx.send(());
    }));
    let before = fake.reads();
    fake.fail_reads(true);
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        fake.changed();
        std::thread::sleep(Duration::from_millis(20));
    }
    let attempts = fake.reads() - before;
    let wakes = rx.try_iter().count();
    assert!(
        (2..=3).contains(&attempts),
        "{attempts} read attempts in 5 seconds"
    );
    assert!(wakes <= 3, "{wakes} subscriber wakes in 5 seconds");
    fake.fail_reads(false);
    fake.publish(Some(Snapshot {
        level: 0.6,
        muted: false,
    }))
    .unwrap();
    wait(|| value(&*control, "level") == Some(0.6));
}
