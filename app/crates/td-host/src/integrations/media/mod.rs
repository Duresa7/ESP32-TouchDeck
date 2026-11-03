use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::{
        Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Receiver,
    },
    time::Instant,
};
use td_core::{
    layout::{StreamType, TileRender},
    live::LiveValue,
};

pub mod fake;
#[cfg(windows)]
mod windows;

pub const MAX_THUMBNAIL_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 4096;
const NO_RESPONSE: &str = "The player didn't respond.";
const UNAVAILABLE: &str = "Media isn't available.";
const HEARTBEAT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    Idle,
    Paused,
    Playing,
}
impl Playback {
    fn number(self) -> f64 {
        match self {
            Self::Idle => 0.0,
            Self::Paused => 1.0,
            Self::Playing => 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    Previous,
    PlayPause,
    Next,
}
impl MediaCommand {
    pub fn name(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::PlayPause => "play_pause",
            Self::Next => "next",
        }
    }
}

pub trait MediaProvider: Send + Sync {
    fn start(&self, feed: Feed) -> Result<Arc<dyn MediaSession>, String>;
}
pub trait MediaSession: Send + Sync {
    fn command(&self, command: MediaCommand, deadline: Instant) -> Receiver<Result<bool, String>>;
}

struct State {
    enabled: bool,
    error: Option<String>,
    title: String,
    artist: String,
    playback: Playback,
    metadata_generation: Option<u64>,
    cover: Option<String>,
    pictures: VecDeque<(String, Arc<[u8]>)>,
    sample: u64,
    heartbeat: Instant,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            error: None,
            title: String::new(),
            artist: String::new(),
            playback: Playback::Idle,
            metadata_generation: None,
            cover: None,
            pictures: VecDeque::new(),
            sample: 1,
            heartbeat: Instant::now(),
        }
    }
}
#[derive(Default)]
struct Core {
    state: Mutex<State>,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    session: Mutex<Option<Arc<dyn MediaSession>>>,
}
impl Core {
    fn wake(&self) {
        let wake = self.wake.lock().unwrap().clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}

#[derive(Clone)]
pub struct Feed {
    core: Weak<Core>,
    active: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}
impl Feed {
    /// Invalidates outstanding metadata and thumbnail requests; safe in an OS callback.
    pub fn invalidate(&self) -> u64 {
        self.generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1)
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub fn current(&self, generation: u64) -> bool {
        self.active.load(Ordering::SeqCst) && self.generation() == generation
    }
    fn update(&self, generation: u64, f: impl FnOnce(&mut State)) {
        let Some(core) = self.core.upgrade() else {
            return;
        };
        {
            let mut state = core.state.lock().unwrap();
            if !self.current(generation) || !state.enabled {
                return;
            }
            f(&mut state);
            state.sample = state.sample.wrapping_add(1);
        }
        core.wake();
    }
    pub fn metadata(&self, generation: u64, title: &str, artist: &str, playback: Playback) {
        self.update(generation, |state| {
            let title = bounded_text(title);
            let artist = bounded_text(artist);
            if state.metadata_generation != Some(generation)
                || state.title != title
                || state.artist != artist
                || playback == Playback::Idle
            {
                state.cover = None;
            }
            state.metadata_generation = Some(generation);
            state.title = if playback == Playback::Idle {
                String::new()
            } else {
                title
            };
            state.artist = if playback == Playback::Idle {
                String::new()
            } else {
                artist
            };
            state.playback = playback;
            state.error = None;
        });
    }
    pub fn clear_cover(&self, generation: u64) {
        self.update(generation, |state| state.cover = None);
    }
    pub fn cover(&self, generation: u64, bytes: &[u8]) {
        if bytes.is_empty() || bytes.len() > MAX_THUMBNAIL_BYTES || !self.current(generation) {
            return;
        }
        let id = format!("sha256:{:x}", Sha256::digest(bytes));
        self.update(generation, |state| {
            if state.playback == Playback::Idle {
                return;
            }
            let picture = state
                .pictures
                .iter()
                .position(|(key, _)| key == &id)
                .and_then(|index| state.pictures.remove(index))
                .unwrap_or_else(|| (id.clone(), Arc::from(bytes)));
            if state.pictures.len() == 8 {
                state.pictures.pop_front();
            }
            state.pictures.push_back(picture);
            state.cover = Some(id);
        });
    }
    pub fn idle(&self, generation: u64) {
        self.metadata(generation, "", "", Playback::Idle);
    }
    pub fn error(&self, message: &str) {
        self.update(self.generation(), |state| {
            state.error = Some(bounded_text(message));
            state.cover = None;
        });
    }
    pub fn heartbeat(&self, now: Instant) {
        let Some(core) = self.core.upgrade() else {
            return;
        };
        {
            let mut state = core.state.lock().unwrap();
            if !self.active.load(Ordering::SeqCst)
                || !state.enabled
                || state.error.is_some()
                || now.saturating_duration_since(state.heartbeat) < HEARTBEAT
            {
                return;
            }
            state.heartbeat = now;
            state.sample = state.sample.wrapping_add(1);
        }
        core.wake();
    }
}
fn bounded_text(text: &str) -> String {
    let mut end = text.len().min(MAX_TEXT_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

impl Control for Core {
    fn live(&self) -> Vec<LiveSample> {
        let state = self.state.lock().unwrap();
        if !state.enabled || state.error.is_some() {
            return Vec::new();
        }
        [
            ("title", LiveValue::Text(state.title.clone())),
            ("artist", LiveValue::Text(state.artist.clone())),
            ("cover", LiveValue::Image(state.cover.clone())),
            ("state", LiveValue::Number(state.playback.number())),
        ]
        .into_iter()
        .map(|(stream, value)| LiveSample {
            account: String::new(),
            stream: stream.into(),
            sample: state.sample,
            ttl_ms: 30_000,
            value,
        })
        .collect()
    }
    fn subscribe(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self.wake.lock().unwrap() = Some(wake);
    }
    fn act(&self, account: &str, action: &str, timeout: Duration) -> Result<(), ControlError> {
        if !account.is_empty() {
            return Err(ControlError::UnknownAccount);
        }
        let command = match action {
            "previous" => MediaCommand::Previous,
            "play_pause" => MediaCommand::PlayPause,
            "next" => MediaCommand::Next,
            _ => return Err(ControlError::UnknownAction),
        };
        {
            let state = self.state.lock().unwrap();
            if !state.enabled || state.error.is_some() {
                return Err(ControlError::Unavailable(UNAVAILABLE.into()));
            }
            if state.playback == Playback::Idle {
                return Err(ControlError::Unavailable("Nothing is playing.".into()));
            }
        }
        let deadline = Instant::now() + timeout.min(Duration::from_secs(2));
        let session = self
            .session
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| ControlError::Unavailable(UNAVAILABLE.into()))?;
        let answer = session.command(command, deadline);
        match answer.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(true)) => Ok(()),
            _ => Err(ControlError::Unavailable(NO_RESPONSE.into())),
        }
    }
    fn set(&self, account: &str, _: &str, _: f64) -> Result<(), ControlError> {
        Err(if account.is_empty() {
            ControlError::NotSettable
        } else {
            ControlError::UnknownAccount
        })
    }
    fn picture(&self, id: &str) -> Option<Arc<[u8]>> {
        self.state
            .lock()
            .unwrap()
            .pictures
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, bytes)| bytes.clone())
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
}

pub struct Media {
    provider: Arc<dyn MediaProvider>,
    core: Arc<Core>,
    feed: Option<Feed>,
    file: Option<PathBuf>,
    delivery: BTreeMap<String, Delivery>,
}
impl Media {
    pub fn new(provider: Arc<dyn MediaProvider>) -> Self {
        Self::with_enabled(provider, true)
    }
    fn with_enabled(provider: Arc<dyn MediaProvider>, enabled: bool) -> Self {
        let mut media = Self {
            provider,
            core: Arc::default(),
            feed: None,
            file: None,
            delivery: BTreeMap::new(),
        };
        media.core.state.lock().unwrap().enabled = enabled;
        if enabled {
            media.start();
        }
        media
    }
    pub fn system() -> Self {
        Self::new(system_provider())
    }
    fn start(&mut self) {
        let feed = Feed {
            core: Arc::downgrade(&self.core),
            active: Arc::new(AtomicBool::new(true)),
            generation: Arc::new(AtomicU64::new(1)),
        };
        match self.provider.start(feed.clone()) {
            Ok(session) => *self.core.session.lock().unwrap() = Some(session),
            Err(error) => feed.error(&error),
        }
        self.feed = Some(feed);
    }
    fn stop(&mut self) {
        if let Some(feed) = self.feed.take() {
            feed.active.store(false, Ordering::SeqCst);
        }
        let session = self.core.session.lock().unwrap().take();
        drop(session);
    }
}
impl Drop for Media {
    fn drop(&mut self) {
        self.stop();
    }
}
impl Integration for Media {
    fn summary(&self) -> IntegrationSummary {
        let state = self.core.state.lock().unwrap();
        IntegrationSummary {
            revision: 0,
            id: "media".into(),
            name: "Media".into(),
            icon: "music".into(),
            enabled: state.enabled,
            status: IntegrationStatus {
                state: if state.error.is_some() { "error" } else { "ok" }.into(),
                message: state.error.clone(),
                delivery: self.delivery.values().cloned().collect(),
            },
            config_schema: vec![ConfigField {
                name: "enabled".into(),
                kind: "boolean".into(),
                secret: false,
            }],
            streams: [
                ("title", "text"),
                ("artist", "text"),
                ("cover", "image"),
                ("state", "numeric"),
            ]
            .into_iter()
            .map(|(id, kind)| DataStream {
                id: id.into(),
                kind: kind.into(),
                unit: None,
                settable: false,
            })
            .collect(),
            actions: ["previous", "play_pause", "next"].map(String::from).into(),
            presets: vec![PresetTile {
                id: "now_playing".into(),
                name: "Now playing".into(),
                action: json!({"type":"widget","source":"media","account":"","field":"title"}),
                render: Some(TileRender::NowPlaying),
                streams: Some(BTreeMap::from([
                    ("title".into(), StreamType::Text),
                    ("artist".into(), StreamType::Text),
                    ("cover".into(), StreamType::Image),
                    ("state".into(), StreamType::Number),
                ])),
                input: false,
            }],
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: json!({"enabled": self.core.state.lock().unwrap().enabled}),
            secrets: BTreeMap::new(),
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        if settings.enabled != self.core.state.lock().unwrap().enabled {
            if let Some(file) = &self.file {
                crate::sources::write_atomic(
                    file,
                    &serde_json::to_vec(&settings).map_err(|_| Error::Storage)?,
                )
                .map_err(|_| Error::Storage)?;
            }
            self.stop();
            {
                let mut state = self.core.state.lock().unwrap();
                *state = State {
                    enabled: settings.enabled,
                    sample: state.sample.wrapping_add(1),
                    ..State::default()
                };
            }
            if settings.enabled {
                self.start();
            }
            self.core.wake();
        }
        self.view()
    }
    fn secret_put(&mut self, _: &str, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn secret_delete(&mut self, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn refresh(&mut self) {}
    fn tick(&mut self) {}
    fn readings(&self) -> Vec<crate::sources::Reading> {
        Vec::new()
    }
    fn live(&self) -> Vec<LiveSample> {
        self.core.live()
    }
    fn control(&self) -> Option<Arc<dyn Control>> {
        Some(self.core.clone())
    }
    fn delivered(&mut self, delivery: Delivery) {
        self.delivery.insert(delivery.device.clone(), delivery);
    }
}
#[cfg(not(windows))]
struct Unsupported;
#[cfg(not(windows))]
impl MediaProvider for Unsupported {
    fn start(&self, _: Feed) -> Result<Arc<dyn MediaSession>, String> {
        Err("Media needs Windows.".into())
    }
}
fn system_provider() -> Arc<dyn MediaProvider> {
    #[cfg(windows)]
    {
        Arc::new(windows::WindowsMedia)
    }
    #[cfg(not(windows))]
    {
        Arc::new(Unsupported)
    }
}
fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    let file = env
        .config_file
        .as_ref()
        .map(|file| file.with_file_name("media.json"));
    let settings = match file.as_ref().map(std::fs::read) {
        Some(Ok(bytes)) => {
            serde_json::from_slice::<Settings>(&bytes).map_err(|_| Error::ConfigInvalid)?
        }
        Some(Err(error)) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::Storage);
        }
        _ => Settings { enabled: true },
    };
    let mut media = Media::with_enabled(
        env.media_provider.clone().unwrap_or_else(system_provider),
        settings.enabled,
    );
    media.file = file;
    Ok(Box::new(media))
}
inventory::submit! { Registration(create) }
