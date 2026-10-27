//! Scripted updater I/O. This entire module is absent without the e2e feature.
use std::sync::mpsc::{Sender, channel};

use serde::Deserialize;

use super::*;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Scenario {
    Current,
    Available,
    Network,
    Signature,
    Metadata,
    Unconfigured,
    Unsupported,
    Busy,
    Installer,
    Internal,
}

#[derive(Default)]
struct Script {
    scenario: Option<Scenario>,
    checks: usize,
    downloads: usize,
    installs: usize,
    finish_download: Option<Sender<()>>,
}

#[derive(Clone, Default)]
pub(super) struct ScriptedUpdater(Arc<Mutex<Script>>);

pub(super) struct ScriptedUpdate {
    scenario: Scenario,
    script: ScriptedUpdater,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    fixture: &'static str,
    checks: usize,
    downloads: usize,
    installs: usize,
    awaiting_download: bool,
}

impl ScriptedUpdater {
    pub(super) fn enabled(&self) -> bool {
        self.0.lock().unwrap().scenario.is_some()
    }

    pub(super) fn check(&self) -> Option<Result<Option<ScriptedUpdate>, UpdateError>> {
        let mut script = self.0.lock().unwrap();
        let scenario = script.scenario?;
        script.checks += 1;
        let error = match scenario {
            Scenario::Current => return Some(Ok(None)),
            Scenario::Available | Scenario::Signature | Scenario::Installer => {
                return Some(Ok(Some(ScriptedUpdate {
                    scenario,
                    script: self.clone(),
                })));
            }
            Scenario::Network => (
                UpdateErrorKind::Network,
                "Cannot reach the app update server",
            ),
            Scenario::Metadata => (UpdateErrorKind::Metadata, "App update metadata is invalid"),
            Scenario::Unconfigured => (
                UpdateErrorKind::Unconfigured,
                "Install an official release to enable app updates",
            ),
            Scenario::Unsupported => (
                UpdateErrorKind::Unsupported,
                "Desktop app updates are currently packaged for Windows x64 only",
            ),
            Scenario::Busy => (
                UpdateErrorKind::Busy,
                "An app update operation is already running",
            ),
            Scenario::Internal => (
                UpdateErrorKind::Internal,
                "The app updater failed unexpectedly",
            ),
        };
        Some(Err(UpdateError::new(error.0, error.1)))
    }
}

impl ScriptedUpdate {
    pub(super) fn metadata(&self) -> UpdateInfo {
        UpdateInfo {
            version: "0.2.0".into(),
            current_version: "0.1.0".into(),
            notes: "Scripted release notes\nImprove USB deck reliability.".into(),
        }
    }

    pub(super) async fn download(
        &self,
        progress: Channel<DownloadProgress>,
    ) -> Result<Vec<u8>, UpdateError> {
        let (finish, receiver) = channel();
        {
            let mut script = self.script.0.lock().unwrap();
            script.downloads += 1;
            script.finish_download = Some(finish);
        }
        let _ = progress.send(DownloadProgress {
            downloaded: 16_384,
            total: Some(65_536),
        });
        // The spec releases this after observing progress, so screenshots never race a timer.
        let completed = tauri::async_runtime::spawn_blocking(move || {
            receiver.recv_timeout(Duration::from_secs(20))
        })
        .await;
        self.script.0.lock().unwrap().finish_download = None;
        if !matches!(completed, Ok(Ok(()))) {
            return Err(UpdateError::new(
                UpdateErrorKind::Internal,
                "Scripted download was not released",
            ));
        }
        if matches!(self.scenario, Scenario::Signature) {
            return Err(UpdateError::new(
                UpdateErrorKind::Signature,
                "App update signature verification failed",
            ));
        }
        let _ = progress.send(DownloadProgress {
            downloaded: 65_536,
            total: Some(65_536),
        });
        Ok(Vec::new())
    }

    pub(super) fn install(&self) -> Result<(), UpdateError> {
        self.script.0.lock().unwrap().installs += 1;
        if matches!(self.scenario, Scenario::Installer) {
            return Err(UpdateError::new(
                UpdateErrorKind::Installer,
                "Could not start the app installer",
            ));
        }
        // Deliberately no installer, exit, restart, or filesystem write.
        Ok(())
    }
}

/// Select a scenario, release a paused download, or inspect counts. Only mock e2e sessions qualify.
#[tauri::command]
pub(crate) fn e2e_app_update(
    app: AppHandle,
    scenario: Option<Scenario>,
    finish_download: Option<bool>,
) -> Result<Record, UpdateError> {
    if !std::env::var("TOUCHDECK_MOCK").is_ok_and(|value| value == "1") {
        return Err(UpdateError::new(
            UpdateErrorKind::Unsupported,
            "Scripted app updates require the mock deck",
        ));
    }
    let state = app.state::<AppUpdates>();
    if let Some(scenario) = scenario {
        let _previous = take_pending(&state)?;
        *state.scripted.0.lock().unwrap() = Script {
            scenario: Some(scenario),
            ..Script::default()
        };
        *state.pending.lock().unwrap() = Pending::Empty;
    }
    let mut script = state.scripted.0.lock().unwrap();
    if finish_download == Some(true) {
        script
            .finish_download
            .take()
            .ok_or_else(|| {
                UpdateError::new(UpdateErrorKind::Busy, "No scripted download is waiting")
            })?
            .send(())
            .map_err(|_| {
                UpdateError::new(UpdateErrorKind::Internal, "Scripted download stopped")
            })?;
    }
    Ok(Record {
        fixture: "touchdeck-scripted-app-updater-e2e",
        checks: script.checks,
        downloads: script.downloads,
        installs: script.installs,
        awaiting_download: script.finish_download.is_some(),
    })
}
