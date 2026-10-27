//! Desktop app updates. Firmware updates use a separate command and bundle format.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, ipc::Channel};
use tauri_plugin_updater::{Update, UpdaterExt};

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

    fn begin_install(&self) -> Result<InstallGuard, String> {
        let mut state = self.0.lock().unwrap();
        if state.installing || state.transfers != 0 {
            return Err(
                "Wait for deck operations to finish before installing the app update".into(),
            );
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
    Available(Update),
    Downloaded {
        update: Update,
        bytes: Vec<u8>,
    },
    Busy,
}

#[derive(Default)]
pub(crate) struct AppUpdates(Mutex<Pending>);

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

fn take_pending(state: &AppUpdates) -> Result<Pending, String> {
    let mut pending = state.0.lock().unwrap();
    if matches!(*pending, Pending::Busy) {
        return Err("An app update operation is already running".into());
    }
    Ok(std::mem::replace(&mut *pending, Pending::Busy))
}

#[tauri::command]
pub(crate) async fn app_update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    if !cfg!(windows) {
        return Err("Desktop app updates are currently packaged for Windows x64 only".into());
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
        return Err("This candidate has no release signing public key. Install an official release to enable app updates".into());
    }
    let state = app.state::<AppUpdates>();
    let _previous = take_pending(&state)?;
    let result = async {
        app.updater_builder()
            .timeout(Duration::from_secs(15))
            .restart_after_install(false)
            .configure_client(|client| client.https_only(true))
            .build()
            .map_err(|error| error.to_string())?
            .check()
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    match result {
        Ok(Some(update)) => {
            if update.download_url.scheme() != "https" {
                *state.0.lock().unwrap() = Pending::Empty;
                return Err("App update download must use HTTPS".into());
            }
            let info = metadata(&update);
            *state.0.lock().unwrap() = Pending::Available(update);
            Ok(Some(info))
        }
        Ok(None) => {
            *state.0.lock().unwrap() = Pending::Empty;
            Ok(None)
        }
        Err(error) => {
            *state.0.lock().unwrap() = Pending::Empty;
            Err(format!(
                "Could not check for app updates. Check your connection and try again. {error}"
            ))
        }
    }
}

#[tauri::command]
pub(crate) async fn app_update_download(
    app: AppHandle,
    progress: Channel<DownloadProgress>,
) -> Result<(), String> {
    let state = app.state::<AppUpdates>();
    let previous = take_pending(&state)?;
    let Pending::Available(mut update) = previous else {
        *state.0.lock().unwrap() = previous;
        return Err("Check for a newer app version before downloading".into());
    };
    update.timeout = Some(Duration::from_secs(120));
    let mut downloaded = 0;
    let result = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(DownloadProgress { downloaded, total });
            },
            || {},
        )
        .await;
    match result {
        Ok(bytes) => {
            // The official updater returns bytes only after signature AND signed-version verification.
            *state.0.lock().unwrap() = Pending::Downloaded { update, bytes };
            Ok(())
        }
        Err(error) => {
            *state.0.lock().unwrap() = Pending::Available(update);
            Err(format!(
                "App update download or signature verification failed: {error}"
            ))
        }
    }
}

#[tauri::command]
pub(crate) async fn app_update_install(app: AppHandle) -> Result<(), String> {
    if !cfg!(windows) || std::env::var("TOUCHDECK_MOCK").is_ok_and(|value| value == "1") {
        return Err("Installer execution is disabled on this platform or mock session".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _reservation = app.state::<AppUpdateGate>().begin_install()?;
        if crate::commands::deck_info(app.state()).is_some() {
            return Err("Disconnect the deck before installing the app update".into());
        }
        let state = app.state::<AppUpdates>();
        let previous = take_pending(&state)?;
        let Pending::Downloaded { update, bytes } = previous else {
            *state.0.lock().unwrap() = previous;
            return Err("Download and verify the app update before installing".into());
        };
        // On Windows this exits only after the installer launches. Do not request a relaunch.
        let result = update.install(&bytes).map_err(|error| error.to_string());
        *state.0.lock().unwrap() = Pending::Downloaded { update, bytes };
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

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
            app.updater_builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .endpoints(vec![format!("{endpoint}/{name}.json").parse().unwrap()])
                .unwrap()
                .configure_client({
                    let cert = certificate.clone();
                    move |client| client.add_root_certificate(cert.clone())
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
            let update = make("good").check().await.unwrap().unwrap();
            assert_eq!(update.version, version);
            let verified = update.download(|_, _| {}, || {}).await.unwrap();
            assert_eq!(verified, std::fs::read(dir.join("installer.exe")).unwrap());
            assert!(
                make("corrupt")
                    .check()
                    .await
                    .unwrap()
                    .unwrap()
                    .download(|_, _| {}, || {})
                    .await
                    .is_err()
            );
            assert!(
                make("wrong-version")
                    .check()
                    .await
                    .unwrap()
                    .unwrap()
                    .download(|_, _| {}, || {})
                    .await
                    .is_err()
            );
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
