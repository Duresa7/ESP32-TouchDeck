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
    pub acts: Mutex<Vec<(String, String, Duration)>>,
    pub set_times: Mutex<Vec<std::time::Instant>>,
    pub sets: Mutex<Vec<(String, String, f64)>>,
    pub act_delay: Mutex<Duration>,
    pub act_result: Mutex<Option<ControlError>>,
    pub set_result: Mutex<Option<ControlError>>,
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
    fn act(&self, account: &str, action: &str, timeout: Duration) -> Result<(), ControlError> {
        self.acts
            .lock()
            .push((account.to_owned(), action.to_owned(), timeout));
        std::thread::sleep(*self.act_delay.lock());
        self.act_result.lock().clone().map_or(Ok(()), Err)
    }
    fn set(&self, account: &str, stream: &str, value: f64) -> Result<(), ControlError> {
        self.set_times.lock().push(std::time::Instant::now());
        self.sets
            .lock()
            .push((account.to_owned(), stream.to_owned(), value));
        self.set_result.lock().clone().map_or(Ok(()), Err)
    }
    fn picture(&self, id: &str) -> Option<Arc<[u8]>> {
        *self.picture_calls.lock() += 1;
        self.pictures.lock().get(id).cloned()
    }
}
