//! The commands behind the Sources page: CLIProxyAPI and the
//! management key. The sources themselves live in `td-host`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use td_core::app_dirs::Dirs;
use td_core::{Layout, Request};
use td_host::sources::{Config, Hub, KeyStore, MemoryKeys, Sink, Status, key_hint};
use td_link::Session;

const SEND_TIMEOUT: Duration = Duration::from_secs(1);
const SERVICE: &str = "touchdeck";
const ACCOUNT: &str = "cliproxyapi-management-key";

/// The management key in the operating system's credential store.
struct OsKeys;

impl OsKeys {
    fn entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| error.to_string())
    }
}

impl KeyStore for OsKeys {
    fn get(&self) -> Result<Option<String>, String> {
        match OsKeys::entry()?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set(&self, key: &str) -> Result<(), String> {
        OsKeys::entry()?
            .set_password(key)
            .map_err(|error| error.to_string())
    }

    fn clear(&self) -> Result<(), String> {
        match OsKeys::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

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
    keys: Arc<dyn KeyStore>,
    config_file: PathBuf,
    config: Mutex<Config>,
    mock: bool,
}

impl SourcesState {
    /// The mock deck keeps source config in a temporary folder and its key in memory.
    pub fn new(mock: bool) -> SourcesState {
        let (config_file, config, keys): (PathBuf, Config, Arc<dyn KeyStore>) = if mock {
            let base = std::env::temp_dir().join(format!("touchdeck-mock-{}", std::process::id()));
            (
                base.join("config/sources.json"),
                Config::default(),
                Arc::new(MemoryKeys::default()),
            )
        } else {
            let dirs = Dirs::current();
            for problem in dirs.migrate() {
                eprintln!("touchdeck: cannot move old source data: {problem}");
            }
            let file = dirs.sources_config();
            (file.clone(), Config::load(&file), Arc::new(OsKeys))
        };
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
        config
            .save(&state.config_file)
            .map_err(|error| format!("{}: {error}", state.config_file.display()))?;
        *state.config.lock().unwrap() = config.clone();
        state.hub.configure(config);
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

/// Feeds the tiles of a layout, such as the one the editor just pushed.
#[tauri::command]
pub fn sources_use_layout(app: AppHandle, json: String) -> Result<(), String> {
    let layout = Layout::parse(json.as_bytes()).map_err(|errors| {
        errors
            .first()
            .map_or_else(|| "invalid layout".to_owned(), ToString::to_string)
    })?;
    app.state::<SourcesState>().hub.use_layout(layout);
    Ok(())
}
