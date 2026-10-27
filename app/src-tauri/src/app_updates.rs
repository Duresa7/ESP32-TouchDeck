//! Desktop app updates. Firmware updates use a separate command and bundle format.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, ipc::Channel};
use tauri_plugin_updater::{Update, UpdaterBuilder, UpdaterExt};

#[cfg(feature = "e2e")]
pub(crate) mod e2e;

const CHECK_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum UpdateErrorKind {
    Unconfigured,
    Unsupported,
    Network,
    Signature,
    Metadata,
    Busy,
    Installer,
    Internal,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpdateError {
    kind: UpdateErrorKind,
    message: String,
}

impl UpdateError {
    fn new(kind: UpdateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn updater(error: tauri_plugin_updater::Error) -> Self {
        use tauri_plugin_updater::Error;
        let kind = match &error {
            Error::Reqwest(error)
                if error.is_timeout() || (!error.is_builder() && !error.is_decode()) =>
            {
                UpdateErrorKind::Network
            }
            Error::Network(_) | Error::ReleaseNotFound => UpdateErrorKind::Network,
            Error::Minisign(_)
            | Error::Base64(_)
            | Error::SignatureUtf8(_)
            | Error::SignedVersionMismatch { .. }
            | Error::MissingSignedVersion => UpdateErrorKind::Signature,
            Error::UnsupportedArch | Error::UnsupportedOs => UpdateErrorKind::Unsupported,
            _ => UpdateErrorKind::Metadata,
        };
        Self::new(kind, error.to_string())
    }
}

fn updater_builder<R: tauri::Runtime>(app: &AppHandle<R>) -> UpdaterBuilder {
    app.updater_builder()
        .timeout(CHECK_TIMEOUT)
        .restart_after_install(false)
        // The default hook destroys resources and hides windows before ShellExecuteW can fail.
        // The Windows updater exits itself immediately after a successful launch.
        .on_before_exit(|| {})
        .configure_client(configure_client)
}

fn configure_client(client: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    client
        .https_only(true)
        .connect_timeout(CHECK_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
}

async fn check_release(
    updater: tauri_plugin_updater::Updater,
) -> Result<Option<Update>, UpdateError> {
    let mut update = updater.check().await.map_err(UpdateError::updater)?;
    if let Some(update) = &mut update {
        // The metadata deadline is copied into Update. Downloads are bounded by idle reads instead.
        update.timeout = None;
    }
    Ok(update)
}

#[derive(Default)]
struct GateState {
    transfers: usize,
    installing: bool,
}

/// One shared reservation makes transfer start and installer start mutually exclusive.
#[derive(Clone, Default)]
pub(crate) struct AppUpdateGate(Arc<Mutex<GateState>>);

pub(crate) struct TransferGuard(AppUpdateGate);
struct InstallGuard(AppUpdateGate);

impl AppUpdateGate {
    pub(crate) fn begin_transfer(&self) -> Result<TransferGuard, String> {
        let mut state = self.0.lock().unwrap();
        if state.installing {
            return Err(
                "TouchDeck app installation is starting; deck operations are blocked".into(),
            );
        }
        state.transfers += 1;
        Ok(TransferGuard(self.clone()))
    }

    fn begin_install(&self) -> Result<InstallGuard, UpdateError> {
        let mut state = self.0.lock().unwrap();
        if state.installing || state.transfers != 0 {
            return Err(UpdateError::new(
                UpdateErrorKind::Busy,
                "Wait for deck operations to finish before installing the app update",
            ));
        }
        state.installing = true;
        Ok(InstallGuard(self.clone()))
    }
}

impl Drop for TransferGuard {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().transfers -= 1;
    }
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.0.lock().unwrap().installing = false;
    }
}

#[derive(Default)]
enum Pending {
    #[default]
    Empty,
    Available(UpdateSource),
    Downloaded {
        update: UpdateSource,
        bytes: Vec<u8>,
    },
    Busy,
}

#[derive(Default)]
pub(crate) struct AppUpdates {
    pending: Mutex<Pending>,
    #[cfg(feature = "e2e")]
    scripted: e2e::ScriptedUpdater,
}

/// Only the updater I/O varies in e2e builds; pending state and install guards are shared.
enum UpdateSource {
    Release(Box<Update>),
    #[cfg(feature = "e2e")]
    Scripted(e2e::ScriptedUpdate),
}

impl UpdateSource {
    fn metadata(&self) -> UpdateInfo {
        match self {
            Self::Release(update) => metadata(update),
            #[cfg(feature = "e2e")]
            Self::Scripted(update) => update.metadata(),
        }
    }

    async fn download(&self, progress: Channel<DownloadProgress>) -> Result<Vec<u8>, UpdateError> {
        match self {
            Self::Release(update) => {
                let mut downloaded = 0;
                update
                    .download(
                        |chunk, total| {
                            downloaded += chunk as u64;
                            let _ = progress.send(DownloadProgress { downloaded, total });
                        },
                        || {},
                    )
                    .await
                    .map_err(UpdateError::updater)
            }
            #[cfg(feature = "e2e")]
            Self::Scripted(update) => update.download(progress).await,
        }
    }

    fn install(&self, bytes: &[u8]) -> Result<(), UpdateError> {
        match self {
            // On Windows this exits only after the installer launches. Do not request a relaunch.
            Self::Release(update) => update
                .install(bytes)
                .map_err(|error| UpdateError::new(UpdateErrorKind::Installer, error.to_string())),
            #[cfg(feature = "e2e")]
            Self::Scripted(update) => update.install(),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateInfo {
    version: String,
    current_version: String,
    notes: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn metadata(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone().unwrap_or_default(),
    }
}

fn take_pending(state: &AppUpdates) -> Result<Pending, UpdateError> {
    let mut pending = state.pending.lock().unwrap();
    if matches!(*pending, Pending::Busy) {
        return Err(UpdateError::new(
            UpdateErrorKind::Busy,
            "An app update operation is already running",
        ));
    }
    Ok(std::mem::replace(&mut *pending, Pending::Busy))
}

async fn check_source(app: &AppHandle) -> Result<Option<UpdateSource>, UpdateError> {
    #[cfg(feature = "e2e")]
    if let Some(result) = app.state::<AppUpdates>().scripted.check() {
        return result.map(|update| update.map(UpdateSource::Scripted));
    }
    if !cfg!(windows) {
        return Err(UpdateError::new(
            UpdateErrorKind::Unsupported,
            "Desktop app updates are currently packaged for Windows x64 only",
        ));
    }
    let key = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config.get("pubkey"))
        .and_then(|key| key.as_str())
        .unwrap_or_default();
    if key.is_empty() {
        return Err(UpdateError::new(
            UpdateErrorKind::Unconfigured,
            "This candidate has no release signing public key. Install an official release to enable app updates",
        ));
    }
    let update = check_release(updater_builder(app).build().map_err(UpdateError::updater)?).await?;
    if update
        .as_ref()
        .is_some_and(|update| update.download_url.scheme() != "https")
    {
        return Err(UpdateError::new(
            UpdateErrorKind::Metadata,
            "App update download must use HTTPS",
        ));
    }
    Ok(update.map(|update| UpdateSource::Release(Box::new(update))))
}

#[tauri::command]
pub(crate) async fn app_update_check(app: AppHandle) -> Result<Option<UpdateInfo>, UpdateError> {
    let state = app.state::<AppUpdates>();
    let _previous = take_pending(&state)?;
    let result = check_source(&app).await;
    match result {
        Ok(Some(update)) => {
            let info = update.metadata();
            *state.pending.lock().unwrap() = Pending::Available(update);
            Ok(Some(info))
        }
        Ok(None) => {
            *state.pending.lock().unwrap() = Pending::Empty;
            Ok(None)
        }
        Err(error) => {
            *state.pending.lock().unwrap() = Pending::Empty;
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) async fn app_update_download(
    app: AppHandle,
    progress: Channel<DownloadProgress>,
) -> Result<(), UpdateError> {
    let state = app.state::<AppUpdates>();
    let previous = take_pending(&state)?;
    let Pending::Available(update) = previous else {
        *state.pending.lock().unwrap() = previous;
        return Err(UpdateError::new(
            UpdateErrorKind::Busy,
            "Check for a newer app version before downloading",
        ));
    };
    let result = update.download(progress).await;
    match result {
        Ok(bytes) => {
            // The official updater returns bytes only after signature AND signed-version verification.
            *state.pending.lock().unwrap() = Pending::Downloaded { update, bytes };
            Ok(())
        }
        Err(error) => {
            *state.pending.lock().unwrap() = Pending::Available(update);
            Err(error)
        }
    }
}

#[tauri::command]
pub(crate) async fn app_update_install(app: AppHandle) -> Result<(), UpdateError> {
    let disabled =
        !cfg!(windows) || std::env::var("TOUCHDECK_MOCK").is_ok_and(|value| value == "1");
    #[cfg(feature = "e2e")]
    let disabled = disabled && !app.state::<AppUpdates>().scripted.enabled();
    if disabled {
        return Err(UpdateError::new(
            UpdateErrorKind::Unsupported,
            "Installer execution is disabled on this platform or mock session",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _reservation = app.state::<AppUpdateGate>().begin_install()?;
        if crate::commands::deck_info(app.state()).is_some() {
            return Err(UpdateError::new(
                UpdateErrorKind::Busy,
                "Disconnect the deck before installing the app update",
            ));
        }
        let state = app.state::<AppUpdates>();
        let previous = take_pending(&state)?;
        let Pending::Downloaded { update, bytes } = previous else {
            *state.pending.lock().unwrap() = previous;
            return Err(UpdateError::new(
                UpdateErrorKind::Busy,
                "Download and verify the app update before installing",
            ));
        };
        let result = update.install(&bytes);
        *state.pending.lock().unwrap() = Pending::Downloaded { update, bytes };
        result
    })
    .await
    .map_err(|error| UpdateError::new(UpdateErrorKind::Internal, error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updater_errors_preserve_their_kind_across_ipc() {
        use tauri_plugin_updater::Error;
        for (error, kind) in [
            (Error::ReleaseNotFound, UpdateErrorKind::Network),
            (Error::UnsupportedOs, UpdateErrorKind::Unsupported),
            (Error::MissingSignedVersion, UpdateErrorKind::Signature),
            (Error::EmptyEndpoints, UpdateErrorKind::Metadata),
        ] {
            assert_eq!(UpdateError::updater(error).kind, kind);
        }
        let error = UpdateError::new(UpdateErrorKind::Unconfigured, "Install an official release");
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "kind": "unconfigured", "message": "Install an official release"
            })
        );
    }

    #[test]
    fn installer_reservation_and_transfers_exclude_each_other_and_release_on_error() {
        let gate = AppUpdateGate::default();
        let first = gate.begin_transfer().unwrap();
        let second = gate.begin_transfer().unwrap();
        assert!(gate.begin_install().is_err());
        drop(first);
        assert!(gate.begin_install().is_err());
        drop(second);
        let install = gate.begin_install().unwrap();
        assert!(gate.begin_transfer().is_err());
        assert!(gate.begin_install().is_err());
        drop(install);
        assert!(gate.begin_transfer().is_ok());
    }

    #[test]
    fn active_transfer_returns_the_typed_install_refusal_and_allows_retry() {
        let gate = AppUpdateGate::default();
        let transfer = gate.begin_transfer().unwrap();
        let Err(error) = gate.begin_install() else {
            panic!("an active transfer must refuse installation");
        };
        assert_eq!(error.kind, UpdateErrorKind::Busy);
        assert_eq!(
            error.message,
            "Wait for deck operations to finish before installing the app update"
        );
        drop(transfer);
        assert!(gate.begin_install().is_ok());
    }

    #[test]
    #[cfg(feature = "updater-fixture")]
    #[ignore = "requires the disposable HTTPS fixture from scripts/verify_updater.py"]
    fn signed_https_fixture() {
        use std::path::PathBuf;
        let dir = PathBuf::from(std::env::var_os("TOUCHDECK_UPDATER_FIXTURE_DIR").unwrap());
        let endpoint = std::env::var("TOUCHDECK_UPDATER_FIXTURE_URL").unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("release-manifest.json")).unwrap())
                .unwrap();
        let public_key = manifest["updater_public_key"].as_str().unwrap();
        let version = manifest["version"].as_str().unwrap();
        let certificate =
            reqwest::Certificate::from_pem(&std::fs::read(dir.join("fixture-ca.pem")).unwrap())
                .unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.package_info_mut().version = "0.0.0".parse().unwrap();
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
                "pubkey": public_key, "requireSignedVersion": true,
                "endpoints": [format!("{endpoint}/good.json")]
            }),
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        let make = |name: &str| {
            updater_builder(app.handle())
                .no_proxy()
                .endpoints(vec![format!("{endpoint}/{name}.json").parse().unwrap()])
                .unwrap()
                .configure_client({
                    let cert = certificate.clone();
                    move |client| configure_client(client).add_root_certificate(cert.clone())
                })
                .build()
                .unwrap()
        };
        tauri::async_runtime::block_on(async {
            // TLS validation remains enabled: without the explicit test CA the self-signed server is rejected.
            assert!(
                app.updater_builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap()
                    .check()
                    .await
                    .is_err()
            );
            let update = check_release(make("good")).await.unwrap().unwrap();
            assert_eq!(update.version, version);
            assert_eq!(update.timeout, None);
            let verified = update.download(|_, _| {}, || {}).await.unwrap();
            assert_eq!(verified, std::fs::read(dir.join("installer.exe")).unwrap());
            let trickle = check_release(make("trickle")).await.unwrap().unwrap();
            let started = std::time::Instant::now();
            assert_eq!(trickle.download(|_, _| {}, || {}).await.unwrap(), verified);
            assert!(started.elapsed() > CHECK_TIMEOUT);
            println!(
                "steady trickle verified after {:?} (metadata deadline {:?})",
                started.elapsed(),
                CHECK_TIMEOUT
            );
            let stalled = check_release(make("stall")).await.unwrap().unwrap();
            let started = std::time::Instant::now();
            let error = stalled.download(|_, _| {}, || {}).await.unwrap_err();
            assert!(
                matches!(&error, tauri_plugin_updater::Error::Reqwest(error) if error.is_timeout())
            );
            assert_eq!(UpdateError::updater(error).kind, UpdateErrorKind::Network);
            assert!(started.elapsed() >= READ_TIMEOUT);
            assert!(started.elapsed() < READ_TIMEOUT + Duration::from_secs(3));
            println!(
                "stalled download rejected after {:?} (idle deadline {:?})",
                started.elapsed(),
                READ_TIMEOUT
            );
            for name in ["corrupt", "wrong-version"] {
                let update = check_release(make(name)).await.unwrap().unwrap();
                let error = update.download(|_, _| {}, || {}).await.unwrap_err();
                assert_eq!(UpdateError::updater(error).kind, UpdateErrorKind::Signature);
            }
            assert!(make("malformed").check().await.is_err());
            assert!(make("current").check().await.unwrap().is_none());
            let offline = app
                .updater_builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .endpoints(vec!["https://127.0.0.1:1/latest.json".parse().unwrap()])
                .unwrap()
                .build()
                .unwrap();
            assert!(offline.check().await.is_err());
        });
        println!(
            "signed HTTPS installer accepted; corrupt payload, mismatched signed version, invalid TLS, malformed metadata and offline endpoint rejected; current version has no update; installer never executed"
        );
    }
}
