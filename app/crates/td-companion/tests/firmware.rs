use std::sync::{Arc, Condvar, Mutex};
use td_companion::{api::NoQuery, features::firmware::*, jobs::JobState, testing::TestCompanion};
#[derive(Default)]
struct Prompt {
    answer: Mutex<Option<bool>>,
    wake: Condvar,
    seen: Mutex<Vec<Confirmation>>,
}
impl Prompt {
    fn answer(&self, answer: bool) {
        *self.answer.lock().unwrap() = Some(answer);
        self.wake.notify_all();
    }
}
impl Prompter for Prompt {
    fn pick(&self) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(
            include_bytes!("../../td-link/tests/fixtures/touchdeck-test.tdfw").to_vec(),
        ))
    }
    fn confirm(&self, c: &Confirmation) -> bool {
        self.seen.lock().unwrap().push(c.clone());
        let mut a = self.answer.lock().unwrap();
        while a.is_none() {
            a = self.wake.wait(a).unwrap();
        }
        a.take().unwrap()
    }
}
fn request() -> FirmwareRequest {
    FirmwareRequest {
        bundle: "bundled".into(),
    }
}
#[tokio::test]
async fn consent_consumes_selection_and_decline_writes_nothing() {
    let td = TestCompanion::start().await;
    let a = td.plug_device("mocka").await;
    let b = td.plug_device("mockb").await;
    let prompt = Arc::new(Prompt::default());
    td.firmware_prompter(prompt.clone());
    let info = td
        .call::<Pick>((), NoQuery, FirmwareTarget { device: a.clone() })
        .await
        .unwrap()
        .bundle
        .unwrap();
    let wrong = td
        .request::<Update>(
            FirmwareTarget { device: b },
            NoQuery,
            &FirmwareRequest {
                bundle: info.id.clone(),
            },
        )
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 404);
    let job = td
        .call::<Repair>(
            FirmwareTarget { device: a.clone() },
            NoQuery,
            FirmwareRequest {
                bundle: info.id.clone(),
            },
        )
        .await
        .unwrap();
    td.until(|_| !prompt.seen.lock().unwrap().is_empty()).await;
    assert!(
        !td.deck("mocka")
            .received()
            .iter()
            .any(|v| v["m"] == "update.begin")
    );
    assert_eq!(prompt.seen.lock().unwrap()[0].target, "mocka");
    let reused = td
        .request::<Repair>(
            FirmwareTarget { device: a },
            NoQuery,
            &FirmwareRequest { bundle: info.id },
        )
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), 404);
    prompt.answer(false);
    let done = td.wait(&job).await;
    assert_eq!(done.state, JobState::Failed);
    assert_eq!(done.error.unwrap().code, "firmware.declined");
}
#[tokio::test]
async fn update_reports_success_and_rollback() {
    for rollback in [false, true] {
        let td = TestCompanion::start().await;
        let id = td.plug_device("mocka").await;
        let prompt = Arc::new(Prompt::default());
        prompt.answer(true);
        td.firmware_prompter(prompt);
        if rollback {
            td.deck("mocka").update_mode("rollback");
        }
        let job = td
            .call::<Update>(FirmwareTarget { device: id }, NoQuery, request())
            .await
            .unwrap();
        let done = td.wait(&job).await;
        if rollback {
            assert_eq!(done.error.unwrap().code, "firmware.rolled_back");
        } else {
            assert_eq!(done.state, JobState::Succeeded, "{:?}", done.error);
            assert!(done.result.is_some());
        }
    }
}
#[tokio::test]
async fn picked_bundle_expires() {
    let td = TestCompanion::start().await;
    let _events = td.events().await;
    let id = td.plug_device("mocka").await;
    td.firmware_prompter(Arc::new(Prompt::default()));
    let info = td
        .call::<Pick>((), NoQuery, FirmwareTarget { device: id.clone() })
        .await
        .unwrap()
        .bundle
        .unwrap();
    td.clock().advance(std::time::Duration::from_secs(601));
    let response = td
        .request::<Update>(
            FirmwareTarget { device: id },
            NoQuery,
            &FirmwareRequest { bundle: info.id },
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn new_board_install_requires_consent_and_needs_setup() {
    use td_companion::devices::{BoardKind, SetupState};
    let td = TestCompanion::start().await;
    let board = td.plug_board("newboard", BoardKind::OtherFirmware).await;
    let prompt = Arc::new(Prompt::default());
    td.firmware_prompter(prompt.clone());
    let job = td
        .call::<Install>(
            FirmwareTarget {
                device: board.id.clone(),
            },
            NoQuery,
            request(),
        )
        .await
        .unwrap();
    td.until(|_| !prompt.seen.lock().unwrap().is_empty()).await;
    assert_eq!(td.deck("newboard").reboots(), 0);
    prompt.answer(true);
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
    td.until(|td| {
        td.device(&board.id)
            .is_some_and(|d| d.setup == SetupState::NeedsSetup)
    })
    .await;
}

#[tokio::test]
async fn install_lock_is_owner_bound_and_expires() {
    use td_companion::features::system::{InstallLock, Lock, LockRenew, Unlock};
    let td = TestCompanion::start().await;
    let _socket = td.events().await;
    let lock = td.call::<Lock>((), NoQuery, ()).await.unwrap();
    let other: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/api/v1/auth/sessions", td.url))
        .bearer_auth(&td.pairing_token)
        .json(&serde_json::json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    for method in [reqwest::Method::PUT, reqwest::Method::DELETE] {
        let response = reqwest::Client::new()
            .request(
                method,
                format!("{}/api/v1/system/lock/{}", td.url, lock.lock),
            )
            .bearer_auth(other["token"].as_str().unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    td.clock().advance(std::time::Duration::from_secs(500));
    td.call::<LockRenew>(
        InstallLock {
            lock: lock.lock.clone(),
        },
        NoQuery,
        (),
    )
    .await
    .unwrap();
    td.clock().advance(std::time::Duration::from_secs(101));
    assert_eq!(
        td.request::<Lock>((), NoQuery, &())
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    td.call::<Unlock>(lock, NoQuery, ()).await.unwrap();
    td.call::<Lock>((), NoQuery, ()).await.unwrap();
    td.clock().advance(std::time::Duration::from_secs(601));
    td.call::<Lock>((), NoQuery, ()).await.unwrap();
}
