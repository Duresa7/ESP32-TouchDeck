use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use td_core::live::LiveValue;
use td_host::integrations::{Control, ControlError, LiveSample};

#[derive(Default)]
pub(crate) struct FakeControl {
    samples: Mutex<Vec<LiveSample>>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    pictures: Mutex<HashMap<String, Arc<[u8]>>>,
    next: Mutex<u64>,
    pub picture_calls: Mutex<usize>,
}

impl FakeControl {
    pub fn publish(&self, values: &[(&str, LiveValue)]) {
        {
            let mut samples = self.samples.lock();
            let mut next = self.next.lock();
            for (stream, value) in values {
                *next += 1;
                samples.retain(|s| s.stream != *stream);
                samples.push(LiveSample {
                    account: String::new(),
                    stream: (*stream).to_owned(),
                    sample: *next,
                    ttl_ms: 30_000,
                    value: value.clone(),
                });
            }
        }
        let wake = self.wake.lock().clone();
        if let Some(wake) = wake {
            wake();
        }
    }

    pub fn number(&self, value: f64) {
        self.publish(&[("level", LiveValue::Number(value))]);
    }

    pub fn add_picture(&self, id: &str, bytes: Vec<u8>) {
        self.pictures.lock().insert(id.to_owned(), bytes.into());
    }

    pub fn samples(&self) -> Vec<LiveSample> {
        self.samples.lock().clone()
    }
}

impl Control for FakeControl {
    fn live(&self) -> Vec<LiveSample> {
        self.samples.lock().clone()
    }
    fn subscribe(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self.wake.lock() = Some(wake);
    }
    fn act(&self, _: &str, _: &str, _: Duration) -> Result<(), ControlError> {
        Err(ControlError::UnknownAction)
    }
    fn set(&self, _: &str, _: &str, _: f64) -> Result<(), ControlError> {
        Err(ControlError::NotSettable)
    }
    fn picture(&self, id: &str) -> Option<Arc<[u8]>> {
        *self.picture_calls.lock() += 1;
        self.pictures.lock().get(id).cloned()
    }
}
