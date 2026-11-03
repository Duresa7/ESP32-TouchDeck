use super::*;
use serde_json::json;
use std::sync::Mutex;
use td_core::{
    layout::{StreamType, TileRender},
    live::LiveValue,
};

pub mod fake;
#[cfg(windows)]
mod windows;
mod worker;
pub use worker::Notifier;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub level: f64,
    pub muted: bool,
}
impl Snapshot {
    pub fn valid(self) -> bool {
        self.level.is_finite() && (0.0..=1.0).contains(&self.level)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    NoDevice,
    Unavailable,
    NeedsWindows,
}
impl Failure {
    fn message(self) -> &'static str {
        match self {
            Self::NoDevice => "No playback device.",
            Self::Unavailable => "Volume is unavailable.",
            Self::NeedsWindows => "Volume needs Windows.",
        }
    }
    fn control(self) -> ControlError {
        ControlError::Unavailable(self.message().into())
    }
}

/// Created, called and dropped only on the Volume worker. Reads return OS state.
pub trait VolumeProvider {
    fn read(&mut self, rebind: bool) -> Result<Snapshot, Failure>;
    fn set_level(&mut self, level: f64) -> Result<(), Failure>;
    fn set_mute(&mut self, muted: bool) -> Result<(), Failure>;
}
pub type ProviderFactory =
    Arc<dyn Fn(Notifier) -> Result<Box<dyn VolumeProvider>, Failure> + Send + Sync>;
type Wake = Arc<dyn Fn() + Send + Sync>;

struct State {
    enabled: bool,
    current: Result<Snapshot, Failure>,
    sample: u64,
    wake: Option<Wake>,
}
struct Handle {
    state: Arc<Mutex<State>>,
    queue: Mutex<Option<worker::Queue>>,
}
impl Control for Handle {
    fn live(&self) -> Vec<LiveSample> {
        let Ok(state) = self.state.try_lock() else {
            return Vec::new();
        };
        let Ok(current) = state.current else {
            return Vec::new();
        };
        if !state.enabled {
            return Vec::new();
        }
        [
            ("level", current.level),
            ("muted", f64::from(current.muted)),
        ]
        .into_iter()
        .map(|(stream, value)| LiveSample {
            account: String::new(),
            stream: stream.into(),
            sample: state.sample,
            ttl_ms: 30_000,
            value: LiveValue::Number(value),
        })
        .collect()
    }
    fn subscribe(&self, wake: Wake) {
        self.state.lock().unwrap().wake = Some(wake);
    }
    fn act(&self, account: &str, action: &str, timeout: Duration) -> Result<(), ControlError> {
        if !account.is_empty() {
            return Err(ControlError::UnknownAccount);
        }
        if action != "mute" {
            return Err(ControlError::UnknownAction);
        }
        if timeout.is_zero() {
            return Err(Failure::Unavailable.control());
        }
        let queue = self.available()?;
        queue.mute(timeout)
    }
    fn set(&self, account: &str, stream: &str, value: f64) -> Result<(), ControlError> {
        if !account.is_empty() {
            return Err(ControlError::UnknownAccount);
        }
        if stream != "level" {
            return Err(ControlError::NotSettable);
        }
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(ControlError::OutOfRange);
        }
        self.available()?.set(value);
        Ok(())
    }
}
impl Handle {
    fn available(&self) -> Result<worker::Queue, ControlError> {
        let queue = self
            .queue
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Failure::Unavailable.control())?;
        self.state
            .lock()
            .unwrap()
            .current
            .map_err(Failure::control)?;
        Ok(queue)
    }
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
}

pub struct Volume {
    factory: ProviderFactory,
    handle: Arc<Handle>,
    worker: Option<worker::Worker>,
    settings: Settings,
    file: Option<PathBuf>,
    delivery: BTreeMap<String, Delivery>,
}
impl Volume {
    pub fn new(factory: ProviderFactory, file: Option<PathBuf>) -> Result<Self, Error> {
        let mut settings = Settings { enabled: true };
        if let Some(file) = &file {
            match std::fs::read(file) {
                Ok(bytes) => {
                    settings = serde_json::from_slice(&bytes).map_err(|_| Error::ConfigInvalid)?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Storage),
            }
        }
        let mut volume = Self {
            factory,
            handle: Arc::new(Handle {
                state: Arc::new(Mutex::new(State {
                    enabled: false,
                    current: Err(Failure::Unavailable),
                    sample: 0,
                    wake: None,
                })),
                queue: Mutex::new(None),
            }),
            worker: None,
            settings,
            file,
            delivery: BTreeMap::new(),
        };
        if volume.settings.enabled {
            volume.start()?;
        }
        Ok(volume)
    }
    pub fn system(file: Option<PathBuf>) -> Result<Self, Error> {
        #[cfg(windows)]
        let factory: ProviderFactory = Arc::new(windows::open);
        #[cfg(not(windows))]
        let factory: ProviderFactory = Arc::new(|_| Err(Failure::NeedsWindows));
        Self::new(factory, file)
    }
    fn start(&mut self) -> Result<(), Error> {
        let (worker, queue) =
            worker::Worker::start(self.factory.clone(), self.handle.state.clone())?;
        *self.handle.queue.lock().unwrap() = Some(queue);
        self.worker = Some(worker);
        Ok(())
    }
    fn stop(&mut self) {
        *self.handle.queue.lock().unwrap() = None;
        let wake = {
            let mut state = self.handle.state.lock().unwrap();
            state.enabled = false;
            state.current = Err(Failure::Unavailable);
            state.wake.clone()
        };
        self.worker.take();
        if let Some(wake) = wake {
            wake();
        }
    }
}
impl Drop for Volume {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Integration for Volume {
    fn summary(&self) -> IntegrationSummary {
        let state = self.handle.state.lock().unwrap();
        let message = if self.settings.enabled {
            state.current.err().map(|e| e.message().into())
        } else {
            None
        };
        IntegrationSummary {
            revision: state.sample,
            id: "volume".into(),
            name: "Volume".into(),
            icon: "volume".into(),
            enabled: self.settings.enabled,
            status: IntegrationStatus {
                state: if message.is_some() { "error" } else { "ok" }.into(),
                message,
                delivery: self.delivery.values().cloned().collect(),
            },
            config_schema: vec![ConfigField {
                name: "enabled".into(),
                kind: "boolean".into(),
                secret: false,
            }],
            streams: ["level", "muted"]
                .into_iter()
                .map(|id| DataStream {
                    id: id.into(),
                    kind: "numeric".into(),
                    unit: None,
                    settable: id == "level",
                })
                .collect(),
            actions: vec!["mute".into()],
            presets: vec![PresetTile {
                id: "volume_bar".into(),
                name: "Volume bar".into(),
                action: json!({"type":"widget","source":"volume","account":"","field":"level"}),
                render: Some(TileRender::Bar),
                input: true,
                streams: Some(BTreeMap::from([
                    ("value".into(), StreamType::Number),
                    ("muted".into(), StreamType::Number),
                ])),
            }],
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: json!({"enabled":self.settings.enabled}),
            secrets: BTreeMap::new(),
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        if settings != self.settings {
            if let Some(file) = &self.file {
                crate::sources::write_atomic(
                    file,
                    &serde_json::to_vec(&settings).map_err(|_| Error::Storage)?,
                )
                .map_err(|_| Error::Storage)?;
            }
            if settings.enabled {
                self.start()?;
            } else {
                self.stop();
            }
            self.settings = settings;
        }
        self.view()
    }
    fn secret_put(&mut self, _: &str, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn secret_delete(&mut self, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn refresh(&mut self) {
        if let Some(queue) = self.handle.queue.lock().unwrap().as_ref() {
            queue.refresh();
        }
    }
    fn tick(&mut self) {}
    fn readings(&self) -> Vec<crate::sources::Reading> {
        Vec::new()
    }
    fn live(&self) -> Vec<LiveSample> {
        self.handle.live()
    }
    fn control(&self) -> Option<Arc<dyn Control>> {
        Some(self.handle.clone())
    }
    fn delivered(&mut self, delivery: Delivery) {
        self.delivery.insert(delivery.device.clone(), delivery);
    }
}

fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    let file = env
        .config_file
        .as_ref()
        .map(|file| file.with_file_name("volume.json"));
    let volume = match &env.volume {
        Some(factory) => Volume::new(factory.clone(), file)?,
        None => Volume::system(file)?,
    };
    Ok(Box::new(volume))
}
inventory::submit! { Registration(create) }
