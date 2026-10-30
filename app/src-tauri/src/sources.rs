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

struct SavedConfig {
    value: Config,
    configured: bool,
    pending_key: Option<String>,
}

pub struct SourcesState {
    hub: Hub,
    keys: Arc<BoundKeys>,
    config_file: PathBuf,
    config: Mutex<SavedConfig>,
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
        let configured = !mock && config_file.is_file();
        SourcesState {
            hub: Hub::start(config.clone(), keys.clone()),
            keys,
            config_file,
            config: Mutex::new(SavedConfig {
                value: config,
                configured,
                pending_key: None,
            }),
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

    fn save_config(&self, config: Config) -> Result<(), String> {
        let mut config = config;
        config.cliproxy_url = config.cliproxy_url.trim().to_owned();
        config.cliproxy_interval_s = config.cliproxy_interval_s.max(60);
        let mut saved = self.config.lock().unwrap();
        let next_origin = td_host::integrations::secrets::origin(&config.cliproxy_url)?;
        if saved.configured
            && td_host::integrations::secrets::origin(&saved.value.cliproxy_url)
                .ok()
                .as_ref()
                != Some(&next_origin)
        {
            saved.pending_key = None;
        }
        self.keys.rebind(&config.cliproxy_url)?;
        if let Err(error) = config.save(&self.config_file) {
            self.keys.rebind(&saved.value.cliproxy_url)?;
            return Err(format!("{}: {error}", self.config_file.display()));
        }
        saved.value = config.clone();
        saved.configured = true;
        self.hub.configure(config);
        if let Some(key) = &saved.pending_key {
            self.keys.set(key)?;
        }
        saved.pending_key = None;
        drop(saved);
        Ok(())
    }

    fn set_key(&self, key: &str) -> Result<Option<String>, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("the management key is empty".to_owned());
        }
        let mut saved = self.config.lock().unwrap();
        if !saved.configured {
            saved.pending_key = Some(key.into());
            return Ok(Some(key_hint(key)));
        }
        self.keys.set(key)?;
        saved.pending_key = None;
        self.hub.refresh(td_host::sources::CLIPROXYAPI);
        Ok(Some(key_hint(key)))
    }

    fn clear_key(&self) -> Result<(), String> {
        let mut saved = self.config.lock().unwrap();
        saved.pending_key = None;
        self.keys.clear()
    }

    fn key_hint(&self, saved: &SavedConfig) -> Option<String> {
        saved
            .pending_key
            .as_ref()
            .map(|key| key_hint(key))
            .or_else(|| self.keys.get().ok().flatten().map(|key| key_hint(&key)))
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
    let saved = state.config.lock().unwrap();
    SourcesView {
        config_file: state.config_file.display().to_string(),
        key_hint: state.key_hint(&saved),
        mock: state.mock,
        config: saved.value.clone(),
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
        state.save_config(config)?;
        Ok(view(&state))
    })
    .await?
}

#[tauri::command]
pub async fn sources_set_key(app: AppHandle, key: String) -> Result<Option<String>, String> {
    blocking(move || {
        let state = app.state::<SourcesState>();
        state.set_key(&key)
    })
    .await?
}

#[tauri::command]
pub async fn sources_clear_key(app: AppHandle) -> Result<(), String> {
    blocking(move || app.state::<SourcesState>().clear_key()).await?
}

#[tauri::command]
pub fn sources_refresh(app: AppHandle, source: String) {
    app.state::<SourcesState>().hub.refresh(&source);
}

#[cfg(test)]
mod tests {
    use super::*;
    use td_host::sources::{CliProxy, Fetch, Source};

    #[derive(Clone, Default)]
    struct Calls(Arc<Mutex<Vec<String>>>);
    impl Fetch for Calls {
        fn get(&self, _: &str, bearer: &str) -> Result<String, String> {
            self.0.lock().unwrap().push(bearer.into());
            Ok("[]".into())
        }
        fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<String, String> {
            unreachable!()
        }
    }
    fn state() -> SourcesState {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let mut state = SourcesState::new(true);
        state.config_file = std::env::temp_dir().join(format!(
            "td-source-setup-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        state
    }
    fn config(url: &str) -> Config {
        Config {
            cliproxy: false,
            cliproxy_url: url.into(),
            ..Config::default()
        }
    }
    #[test]
    fn setup_polls_with_key_and_url_saved_in_either_order() {
        for key_first in [true, false] {
            let state = state();
            let url = "https://hub.example";
            if key_first {
                state.set_key("fixture-key").unwrap();
            }
            state.save_config(config(url)).unwrap();
            if !key_first {
                state.set_key("fixture-key").unwrap();
            }
            let calls = Calls::default();
            CliProxy::new(url, state.keys.clone(), Box::new(calls.clone())).poll();
            assert_eq!(
                *calls.0.lock().unwrap(),
                vec!["fixture-key"],
                "key_first={key_first}"
            );
            std::fs::remove_file(&state.config_file).unwrap();
        }
    }
    #[test]
    fn setup_key_is_not_available_to_pollers_and_can_be_cleared() {
        let state = state();
        state.set_key("fixture-key").unwrap();
        assert!(view(&state).key_hint.is_some());
        assert!(
            state
                .keys
                .get_for(&Config::default().cliproxy_url)
                .unwrap()
                .is_none()
        );
        assert!(state.keys.get_for("https://hub.example").unwrap().is_none());
        state.clear_key().unwrap();
        assert!(view(&state).key_hint.is_none());
        state.save_config(config("https://hub.example")).unwrap();
        assert!(state.keys.get().unwrap().is_none());
        std::fs::remove_file(&state.config_file).unwrap();
    }

    #[test]
    fn saved_origins_keep_path_changes_but_never_transfer_keys() {
        let state = state();
        state.set_key("fixture-key").unwrap();
        state.save_config(config("https://hub.example")).unwrap();
        state
            .save_config(config("https://hub.example/path"))
            .unwrap();
        assert_eq!(state.keys.get().unwrap().as_deref(), Some("fixture-key"));
        for url in [
            "https://other.example",
            "https://other.example:8443",
            "http://other.example:8443",
        ] {
            state.set_key("replacement-fixture").unwrap();
            state.save_config(config(url)).unwrap();
            assert!(view(&state).key_hint.is_none());
            assert!(state.keys.get_for(url).unwrap().is_none());
        }
        std::fs::remove_file(&state.config_file).unwrap();
    }

    #[test]
    fn failed_first_save_keeps_the_draft_private_until_retry() {
        let mut state = state();
        state.set_key("fixture-key").unwrap();
        let file = state.config_file.clone();
        std::fs::write(&file, "blocked parent").unwrap();
        state.config_file = file.join("sources.json");
        assert!(state.save_config(config("https://hub.example")).is_err());
        assert!(state.keys.get_for("https://hub.example").unwrap().is_none());
        assert!(state.keys.get().unwrap().is_none());
        assert!(view(&state).key_hint.is_some());
        std::fs::remove_file(&file).unwrap();
        state.config_file = file;
        state.save_config(config("https://hub.example")).unwrap();
        assert_eq!(state.keys.get().unwrap().as_deref(), Some("fixture-key"));
        std::fs::remove_file(&state.config_file).unwrap();
    }
    #[derive(Default)]
    struct FailingVault {
        store: MemorySecrets,
        fail: Mutex<bool>,
    }
    impl SecretStore for FailingVault {
        fn get(
            &self,
            i: &str,
            f: &str,
        ) -> Result<Option<td_host::integrations::secrets::Secret>, String> {
            self.store.get(i, f)
        }
        fn put(
            &self,
            i: &str,
            f: &str,
            s: td_host::integrations::secrets::Secret,
        ) -> Result<(), String> {
            if *self.fail.lock().unwrap() {
                Err("vault unavailable".into())
            } else {
                self.store.put(i, f, s)
            }
        }
        fn delete(&self, i: &str, f: &str) -> Result<(), String> {
            self.store.delete(i, f)
        }
    }
    #[test]
    fn failed_promotion_binds_draft_to_the_first_saved_origin() {
        for retry in ["https://a.example/path", "https://b.example"] {
            let mut state = state();
            let vault = Arc::new(FailingVault::default());
            state.keys = Arc::new(BoundKeys::from_config(
                vault.clone(),
                &Config::default().cliproxy_url,
            ));
            state.set_key("fixture-key").unwrap();
            *vault.fail.lock().unwrap() = true;
            assert!(state.save_config(config("https://a.example")).is_err());
            assert_eq!(
                Config::load(&state.config_file).cliproxy_url,
                "https://a.example"
            );
            *vault.fail.lock().unwrap() = false;
            state.save_config(config(retry)).unwrap();
            let same = retry == "https://a.example/path";
            assert_eq!(state.keys.get_for(retry).unwrap().is_some(), same);
            let calls = Calls::default();
            CliProxy::new(retry, state.keys.clone(), Box::new(calls.clone())).poll();
            assert_eq!(calls.0.lock().unwrap().len(), usize::from(same));
            std::fs::remove_file(&state.config_file).unwrap();
        }
    }

    #[test]
    fn replacing_a_failed_draft_does_not_restore_the_old_key() {
        let mut state = state();
        let vault = Arc::new(FailingVault::default());
        state.keys = Arc::new(BoundKeys::from_config(
            vault.clone(),
            &Config::default().cliproxy_url,
        ));
        state.set_key("old-fixture").unwrap();
        *vault.fail.lock().unwrap() = true;
        assert!(state.save_config(config("https://a.example")).is_err());
        *vault.fail.lock().unwrap() = false;
        state.set_key("new-fixture").unwrap();
        state.save_config(config("https://a.example/path")).unwrap();
        assert_eq!(state.keys.get().unwrap().as_deref(), Some("new-fixture"));
        std::fs::remove_file(&state.config_file).unwrap();
    }
}
