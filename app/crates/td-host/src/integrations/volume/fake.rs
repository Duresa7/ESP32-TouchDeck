use super::*;

struct FakeState {
    snapshot: Option<Snapshot>,
    notifier: Option<Notifier>,
    active: usize,
    sets: u64,
    fail_reads: bool,
    reads: u64,
}
#[derive(Clone)]
pub struct FakeVolume(Arc<Mutex<FakeState>>);
impl Default for FakeVolume {
    fn default() -> Self {
        Self::new(Snapshot {
            level: 0.5,
            muted: false,
        })
    }
}
impl FakeVolume {
    pub fn new(snapshot: Snapshot) -> Self {
        Self(Arc::new(Mutex::new(FakeState {
            snapshot: snapshot.valid().then_some(snapshot),
            notifier: None,
            active: 0,
            sets: 0,
            fail_reads: false,
            reads: 0,
        })))
    }
    pub fn factory(&self) -> ProviderFactory {
        let fake = self.clone();
        Arc::new(move |notifier| {
            let mut state = fake.0.lock().unwrap();
            state.notifier = Some(notifier);
            state.active += 1;
            Ok(Box::new(Provider(fake.clone())))
        })
    }
    pub fn publish(&self, snapshot: Option<Snapshot>) -> Result<(), ControlError> {
        if snapshot.is_some_and(|snapshot| !snapshot.valid()) {
            return Err(ControlError::OutOfRange);
        }
        let notifier = {
            let mut state = self.0.lock().unwrap();
            state.snapshot = snapshot;
            state.notifier.clone()
        };
        if let Some(notifier) = notifier {
            notifier.endpoint_changed();
        }
        Ok(())
    }
    pub fn fail_reads(&self, fail: bool) {
        self.0.lock().unwrap().fail_reads = fail;
        self.changed();
    }
    pub fn changed(&self) {
        let notifier = self.0.lock().unwrap().notifier.clone();
        if let Some(notifier) = notifier {
            notifier.changed();
        }
    }
    pub fn reads(&self) -> u64 {
        self.0.lock().unwrap().reads
    }
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.0.lock().unwrap().snapshot
    }
    pub fn sets(&self) -> u64 {
        self.0.lock().unwrap().sets
    }
    pub fn active(&self) -> usize {
        self.0.lock().unwrap().active
    }
}
struct Provider(FakeVolume);
impl VolumeProvider for Provider {
    fn read(&mut self, _: bool) -> Result<Snapshot, Failure> {
        let mut state = self.0.0.lock().unwrap();
        state.reads += 1;
        if state.fail_reads {
            Err(Failure::Unavailable)
        } else {
            state.snapshot.ok_or(Failure::NoDevice)
        }
    }
    fn set_level(&mut self, level: f64) -> Result<(), Failure> {
        let notifier = {
            let mut state = self.0.0.lock().unwrap();
            state.snapshot.as_mut().ok_or(Failure::NoDevice)?.level = level;
            state.sets = state.sets.saturating_add(1);
            state.notifier.clone()
        };
        if let Some(notifier) = notifier {
            notifier.changed();
        }
        Ok(())
    }
    fn set_mute(&mut self, muted: bool) -> Result<(), Failure> {
        let notifier = {
            let mut state = self.0.0.lock().unwrap();
            state.snapshot.as_mut().ok_or(Failure::NoDevice)?.muted = muted;
            state.notifier.clone()
        };
        if let Some(notifier) = notifier {
            notifier.changed();
        }
        Ok(())
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap();
        state.active -= 1;
        state.notifier = None;
    }
}
