//! The commands behind the Sources page: the status line shim, CLIProxyAPI, and the
//! management key. The sources themselves live in `td-host`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use td_core::app_dirs::{APP_ID, Dirs};
use td_core::{Layout, Request};
use td_host::sources::claude_status::{
    DiffLine, edit_settings, line_diff, shim_command, unwrap_settings,
};
use td_host::sources::{
    Config, Hub, KeyStore, MemoryKeys, Sink, Status, claude_settings_path, key_hint, write_atomic,
};
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
    /// The mock deck never touches the operator's real state: its directories, its
    /// settings file, and its key all live in a temporary folder or in memory.
    pub fn new(mock: bool) -> SourcesState {
        let (config_file, config, keys): (PathBuf, Config, Arc<dyn KeyStore>) = if mock {
            let base = std::env::temp_dir().join(format!("touchdeck-mock-{}", std::process::id()));
            let config = Config {
                claude_dir: Some(
                    base.join("state")
                        .join(APP_ID)
                        .join("claude-status")
                        .display()
                        .to_string(),
                ),
                claude_settings: Some(
                    base.join("home/.claude/settings.json")
                        .display()
                        .to_string(),
                ),
                ..Config::default()
            };
            (
                base.join("config/sources.json"),
                config,
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

    pub fn pressed(&self, tile: &str) {
        self.hub.pressed(tile);
    }

    fn settings_path(&self) -> PathBuf {
        self.config
            .lock()
            .unwrap()
            .claude_settings
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .map_or_else(claude_settings_path, PathBuf::from)
    }

    fn key_hint(&self) -> Option<String> {
        self.keys.get().ok().flatten().map(|key| key_hint(&key))
    }
}

#[derive(Serialize)]
pub struct Shim {
    command: String,
    found: bool,
}

#[derive(Serialize)]
pub struct SourcesView {
    config: Config,
    key_hint: Option<String>,
    claude_dir: String,
    settings_path: String,
    config_file: String,
    shim: Shim,
    mock: bool,
}

#[derive(Serialize)]
pub struct InstallPreview {
    path: String,
    exists: bool,
    before: Option<String>,
    command: String,
    changed: bool,
    installed: bool,
    shim_found: bool,
    diff: Vec<DiffLine>,
}

#[derive(Serialize)]
pub struct RemoveResult {
    path: String,
    exists: bool,
    /// The command the entry ran, if it had one.
    before: Option<String>,
    /// The command put back, or `None` when the entry went.
    restored: Option<String>,
    changed: bool,
    diff: Vec<DiffLine>,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

fn view(state: &SourcesState) -> SourcesView {
    let config = state.config.lock().unwrap().clone();
    let (command, found) = shim_command(exe_dir().as_deref());
    SourcesView {
        claude_dir: config.claude_dir().display().to_string(),
        settings_path: state.settings_path().display().to_string(),
        config_file: state.config_file.display().to_string(),
        key_hint: state.key_hint(),
        shim: Shim { command, found },
        mock: state.mock,
        config,
    }
}

fn read_settings(
    state: &SourcesState,
    settings: Option<String>,
) -> Result<(PathBuf, Option<String>), String> {
    let path = settings
        .filter(|path| !path.trim().is_empty())
        .map_or_else(|| state.settings_path(), PathBuf::from);
    match fs::read_to_string(&path) {
        Ok(text) => Ok((path, Some(text))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok((path, None)),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn preview(
    state: &SourcesState,
    write: bool,
    settings: Option<String>,
) -> Result<InstallPreview, String> {
    let (path, current) = read_settings(state, settings)?;
    let (shim, shim_found) = shim_command(exe_dir().as_deref());
    let edit = edit_settings(current.as_deref(), &shim)?;
    let diff = line_diff(current.as_deref().unwrap_or(""), &edit.text);
    let installed = write && edit.changed;
    if installed {
        if let Some(text) = &current {
            let backup = path.with_file_name("settings.json.touchdeck.bak");
            if !backup.exists() {
                fs::write(&backup, text)
                    .map_err(|error| format!("{}: {error}", backup.display()))?;
            }
        }
        write_atomic(&path, edit.text.as_bytes())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(InstallPreview {
        path: path.display().to_string(),
        exists: current.is_some(),
        before: edit.before,
        command: edit.command,
        changed: edit.changed,
        installed,
        shim_found,
        diff,
    })
}

/// Undoes Install the way `td-statusline --unwrap` does: the `--then` command goes back in,
/// or the entry goes when the shim ran alone.
fn remove(state: &SourcesState, settings: Option<String>) -> Result<RemoveResult, String> {
    let (path, current) = read_settings(state, settings)?;
    let edit = unwrap_settings(current.as_deref())?;
    if edit.changed {
        write_atomic(&path, edit.text.as_bytes())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(RemoveResult {
        path: path.display().to_string(),
        exists: current.is_some(),
        diff: line_diff(current.as_deref().unwrap_or(""), &edit.text),
        restored: (edit.changed && !edit.command.is_empty()).then_some(edit.command),
        before: edit.before,
        changed: edit.changed,
    })
}

#[tauri::command]
pub fn sources_view(app: AppHandle) -> SourcesView {
    view(&app.state::<SourcesState>())
}

#[tauri::command]
pub fn sources_status(app: AppHandle) -> Status {
    app.state::<SourcesState>().hub.status()
}

#[tauri::command]
pub async fn sources_save(app: AppHandle, config: Config) -> Result<SourcesView, String> {
    blocking(move || {
        let state = app.state::<SourcesState>();
        let mut config = config;
        config.cliproxy_url = config.cliproxy_url.trim().to_owned();
        config.cliproxy_interval_s = config.cliproxy_interval_s.max(1);
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

#[tauri::command]
pub async fn statusline_preview(
    app: AppHandle,
    settings: Option<String>,
) -> Result<InstallPreview, String> {
    blocking(move || preview(&app.state::<SourcesState>(), false, settings)).await?
}

#[tauri::command]
pub async fn statusline_install(
    app: AppHandle,
    settings: Option<String>,
) -> Result<InstallPreview, String> {
    blocking(move || preview(&app.state::<SourcesState>(), true, settings)).await?
}

#[tauri::command]
pub async fn statusline_remove(
    app: AppHandle,
    settings: Option<String>,
) -> Result<RemoveResult, String> {
    blocking(move || remove(&app.state::<SourcesState>(), settings)).await?
}
