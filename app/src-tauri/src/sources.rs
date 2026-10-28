//! The commands behind the Sources page: CLIProxyAPI and the
//! management key. The sources themselves live in `td-host`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use td_core::app_dirs::Dirs;
use td_core::{Layout, Request};
use td_host::sources::{Config, Hub, KeyStore, Sink, Status, key_hint};
use td_link::Session;

const SEND_TIMEOUT: Duration = Duration::from_secs(1);
use td_host::integrations::secrets::{BoundKeys, MemorySecrets, OsSecrets, SecretStore};

struct SessionSink(Arc<Session>);

impl Sink for SessionSink {
    fn send(&self, request: &Request) -> Result<Option<String>, String> {
        let reply = self
            .0
            .request(request, SEND_TIMEOUT)
            .map_err(|error| error.to_string())?;
        Ok((!reply.ok).then(|| {
            let reason = reply.fields.get("error").and_then(|e| e.as_str());
            format!(
                "{} refused: {}",
                reply.m,
                reason.unwrap_or("no reason given")
            )
        }))
    }
}

pub struct SourcesState {
    hub: Hub,
    keys: Arc<BoundKeys>,
    config_file: PathBuf,
    config: Mutex<Config>,
    mock: bool,
}

impl SourcesState {
    /// The mock deck keeps source config in a temporary folder and its key in memory.
    pub fn new(mock: bool) -> SourcesState {
        let (config_file, config, store): (PathBuf, Config, Arc<dyn SecretStore>) = if mock {
            let base = std::env::temp_dir().join(format!("touchdeck-mock-{}", std::process::id()));
            (
                base.join("config/sources.json"),
                Config::default(),
                Arc::new(MemorySecrets::default()),
            )
        } else {
            let file = Dirs::current().sources_config();
            (file.clone(), Config::load(&file), Arc::new(OsSecrets))
        };
        let keys = Arc::new(BoundKeys::from_config(store, &config.cliproxy_url));
        SourcesState {
            hub: Hub::start(config.clone(), keys.clone()),
            keys,
            config_file,
            config: Mutex::new(config),
            mock,
        }
    }

    pub fn connected(&self, session: Arc<Session>) {
        self.hub.set_sink(Some(Arc::new(SessionSink(session))));
    }

    pub fn disconnected(&self) {
        self.hub.set_sink(None);
    }

    pub fn use_layout(&self, layout: Layout) {
        self.hub.use_layout(layout);
    }

    pub fn status(&self) -> Status {
        self.hub.status()
    }

    pub fn pressed(&self, tile: &str) {
        self.hub.pressed(tile);
    }

    fn key_hint(&self) -> Option<String> {
        self.keys.get().ok().flatten().map(|key| key_hint(&key))
    }
}

#[derive(Serialize)]
pub struct SourcesView {
    config: Config,
    key_hint: Option<String>,
    config_file: String,
    mock: bool,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

fn view(state: &SourcesState) -> SourcesView {
    let config = state.config.lock().unwrap().clone();
    SourcesView {
        config_file: state.config_file.display().to_string(),
        key_hint: state.key_hint(),
        mock: state.mock,
        config,
    }
}

#[tauri::command]
pub fn sources_view(app: AppHandle) -> SourcesView {
    view(&app.state::<SourcesState>())
}

#[tauri::command]
pub fn sources_status(app: AppHandle) -> Status {
    app.state::<SourcesState>().status()
}

#[tauri::command]
pub async fn sources_save(app: AppHandle, config: Config) -> Result<SourcesView, String> {
    blocking(move || {
        let state = app.state::<SourcesState>();
        let mut config = config;
        config.cliproxy_url = config.cliproxy_url.trim().to_owned();
        config.cliproxy_interval_s = config.cliproxy_interval_s.max(60);
        let mut saved = state.config.lock().unwrap();
        state.keys.rebind(&config.cliproxy_url)?;
        if let Err(error) = config.save(&state.config_file) {
            state.keys.rebind(&saved.cliproxy_url)?;
            return Err(format!("{}: {error}", state.config_file.display()));
        }
        *saved = config.clone();
        state.hub.configure(config);
        drop(saved);
        Ok(view(&state))
    })
    .await?
}

#[tauri::command]
pub async fn sources_set_key(app: AppHandle, key: String) -> Result<Option<String>, String> {
    blocking(move || {
        let state = app.state::<SourcesState>();
        let key = key.trim();
        if key.is_empty() {
            return Err("the management key is empty".to_owned());
        }
        let _config = state.config.lock().unwrap();
        state.keys.set(key)?;
        state.hub.refresh(td_host::sources::CLIPROXYAPI);
        Ok(state.key_hint())
    })
    .await?
}

#[tauri::command]
pub async fn sources_clear_key(app: AppHandle) -> Result<(), String> {
    blocking(move || app.state::<SourcesState>().keys.clear()).await?
}

#[tauri::command]
pub fn sources_refresh(app: AppHandle, source: String) {
    app.state::<SourcesState>().hub.refresh(&source);
}
