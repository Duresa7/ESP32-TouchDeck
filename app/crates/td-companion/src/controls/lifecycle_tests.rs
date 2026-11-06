use super::*;
use crate::{devices::DeviceManager, live::fake::FakeControl, store::LayoutStore};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use td_host::integrations::Control;

struct Fixture {
    module: DeviceControls,
    authority: LayoutAuthority,
    control: Arc<FakeControl>,
    jobs: Arc<Mutex<Vec<Worker>>>,
    logs: Arc<Mutex<Vec<String>>>,
    id: DeviceId,
    _data: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let events = Events::default();
        let store = LayoutStore::new(data.path().to_owned());
        let layout =
            td_core::Layout::parse(include_bytes!("../../../../../schema/examples/media.json"))
                .unwrap();
        store.put("mock0001", &layout).unwrap();
        let authority = LayoutAuthority::new(
            store,
            Arc::new(|_| true),
            DeviceManager::new(events.clone()),
        );
        authority.publish(
            0,
            HashMap::from([("mock0001".into(), (7, Arc::new(layout)))]),
            vec![],
            BTreeSet::new(),
        );
        let control = Arc::new(FakeControl::default());
        let controls: Controls = Arc::new(BTreeMap::from([(
            "volume".into(),
            control.clone() as Arc<dyn Control>,
        )]));
        let logs = Arc::new(Mutex::new(Vec::new()));
        let logged = logs.clone();
        let mut module = DeviceControls::new(
            authority.clone(),
            controls,
            Arc::new(|_| true),
            Arc::new(|_| true),
            events,
            Arc::new(move |_, _, detail| logged.lock().push(detail)),
        );
        let jobs = Arc::new(Mutex::new(Vec::new()));
        let pending = jobs.clone();
        module.spawn = Arc::new(move |job| pending.lock().push(job));
        Self {
            module,
            authority,
            control,
            jobs,
            logs,
            id: DeviceId::parse("mock0001").unwrap(),
            _data: data,
        }
    }
    fn input(&self, seq: u32, phase: &str, value: f64) {
        self.module.event(&self.id, 7, &json!({"m":"v2.input","page":"media","tile":"volume","stream":"value","seq":seq,"phase":phase,"value":value,"t":1}));
    }
    fn finish(&self) {
        self.module.closed(&self.id);
        for job in std::mem::take(&mut *self.jobs.lock()) {
            job();
        }
    }
}
#[test]
fn delayed_worker_loses_no_part_tap() {
    let f = Fixture::new();
    f.module.event(&f.id, 7, &json!({"m":"press","page":"media","tile":"volume","kind":"widget","part":"mute","seq":1,"t":1}));
    assert!(f.control.acts.lock().is_empty());
    f.finish();
    assert_eq!(f.control.acts.lock().len(), 1);
    assert_eq!(*f.logs.lock(), ["Mute changed."]);
}
#[test]
fn layout_replacement_cancels_a_pending_gesture_without_logging_failure() {
    let f = Fixture::new();
    f.input(1, "move", 0.2);
    f.input(2, "end", 0.4);
    f.authority.replace(&f.id, || Ok::<_, ()>(None)).unwrap();
    f.finish();
    assert!(f.control.sets.lock().is_empty());
    assert!(f.logs.lock().is_empty());
}
#[test]
fn replacement_during_a_control_call_drops_the_remaining_batch_and_its_failure() {
    struct Replacing {
        authority: LayoutAuthority,
        control: Arc<FakeControl>,
    }
    impl Control for Replacing {
        fn live(&self) -> Vec<td_host::integrations::LiveSample> {
            vec![]
        }
        fn subscribe(&self, _: Arc<dyn Fn() + Send + Sync>) {}
        fn act(&self, _: &str, _: &str, _: Duration) -> Result<(), ControlError> {
            unreachable!()
        }
        fn set(&self, account: &str, stream: &str, value: f64) -> Result<(), ControlError> {
            self.control.set(account, stream, value)?;
            self.authority
                .replace("mock0001", || Ok::<_, ()>(None))
                .unwrap();
            Err(ControlError::Unavailable("old failure".into()))
        }
    }
    let mut f = Fixture::new();
    Arc::get_mut(&mut f.module.execution).unwrap().controls = Arc::new(BTreeMap::from([(
        "volume".into(),
        Arc::new(Replacing {
            authority: f.authority.clone(),
            control: f.control.clone(),
        }) as Arc<dyn Control>,
    )]));
    f.input(1, "end", 0.2);
    f.input(2, "end", 0.4);
    f.finish();
    assert_eq!(
        f.control
            .sets
            .lock()
            .iter()
            .map(|s| s.2)
            .collect::<Vec<_>>(),
        [0.2]
    );
    assert!(f.logs.lock().is_empty());
}
#[test]
fn overflow_cancels_pending_gestures_and_the_next_gesture_works() {
    let f = Fixture::new();
    f.input(1, "move", 0.2);
    f.input(2, "end", 0.4);
    f.module.overflow(&f.id);
    f.input(3, "move", 0.6);
    f.input(4, "end", 0.8);
    f.finish();
    assert_eq!(
        f.control
            .sets
            .lock()
            .iter()
            .map(|s| s.2)
            .collect::<Vec<_>>(),
        [0.8]
    );
    assert_eq!(*f.logs.lock(), [GESTURE_DONE]);
}
