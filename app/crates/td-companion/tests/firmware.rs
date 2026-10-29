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
    {
        let seen = prompt.seen.lock().unwrap();
        assert_eq!(seen[0].target, "mocka");
        assert_eq!(seen[0].version, info.version);
        assert_eq!(seen[0].build, info.build);
        assert!(!seen[0].port.is_empty());
        assert_eq!(seen[0].current, "mock");
    }
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
    let mut events = td.events().await;
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
    use futures_util::StreamExt;
    let mut removed = false;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(message)) = events.next().await {
            let Ok(text) = message.to_text() else {
                continue;
            };
            let frame: serde_json::Value = serde_json::from_str(text).unwrap();
            if frame["type"] == "new-board.removed" {
                removed = true;
            }
            if frame["type"] == "device.added" {
                assert!(removed);
                assert_eq!(frame["data"]["id"], board.id.as_str());
                assert_eq!(frame["data"]["setup"], "needs-setup");
                break;
            }
        }
    })
    .await
    .unwrap();
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

#[tokio::test]
async fn confirmation_holds_only_its_target_and_reports_busy_job() {
    use td_companion::{
        devices::LinkState,
        features::layouts::{LayoutBody, LayoutPath, Push},
    };
    let td = TestCompanion::start().await;
    let a = td.plug_device("mocka").await;
    let b = td.plug_device("mockb").await;
    let prompt = Arc::new(Prompt::default());
    td.firmware_prompter(prompt.clone());
    let job = td
        .call::<Repair>(FirmwareTarget { device: a.clone() }, NoQuery, request())
        .await
        .unwrap();
    td.until(|_| !prompt.seen.lock().unwrap().is_empty()).await;
    assert!(
        matches!(td.device(&a).unwrap().link, LinkState::Updating { job: ref id } if id == &job.id)
    );
    let busy = td
        .request::<Update>(FirmwareTarget { device: a }, NoQuery, &request())
        .send()
        .await
        .unwrap();
    assert_eq!(busy.status(), 409);
    let body: serde_json::Value = busy.json().await.unwrap();
    assert_eq!(body["error"]["code"], "device.busy");
    assert_eq!(body["error"]["details"]["job"]["id"], job.id);
    let layout = td_core::Layout::parse(br#"{"schema":1,"name":"Other Device","orientation":"landscape","pages":[{"id":"main","title":"Main","kind":"grid","rows":1,"cols":1,"tiles":[{"id":"a","label":"A","row":0,"col":0,"rowSpan":1,"colSpan":1,"icon":null,"action":{"type":"none"}}]}]}"#).unwrap();
    let push = td
        .call::<Push>(
            LayoutPath {
                device: b.to_string(),
                orientation: td_core::Orientation::Landscape,
            },
            NoQuery,
            LayoutBody::new(&layout),
        )
        .await
        .unwrap();
    assert_eq!(td.wait(&push).await.state, JobState::Succeeded);
    prompt.answer(false);
    assert_eq!(td.wait(&job).await.error.unwrap().code, "firmware.declined");
}

#[tokio::test]
async fn same_or_newer_bundle_needs_no_prompt_and_emits_progress() {
    use futures_util::StreamExt;
    let td = TestCompanion::start().await;
    let id = td.plug_device("mocka").await;
    td.deck("mocka").flash_bundle(
        &td_link::bundle::Bundle::parse(include_bytes!(
            "../../td-link/tests/fixtures/touchdeck-test-b.tdfw"
        ))
        .unwrap(),
    );
    td.call::<td_companion::features::devices::Restart>(
        td_companion::features::devices::DevicePath { device: id.clone() },
        NoQuery,
        (),
    )
    .await
    .unwrap();
    td.until(|td| {
        td.device(&id).is_some_and(|d| {
            matches!(d.link, td_companion::devices::LinkState::Connected { .. })
                && d.firmware.is_some_and(|f| f.version == "test-only")
        })
    })
    .await;
    let prompt = Arc::new(Prompt::default());
    td.firmware_prompter(prompt.clone());
    let mut events = td.events().await;
    let job = td
        .call::<Update>(FirmwareTarget { device: id }, NoQuery, request())
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
    assert!(prompt.seen.lock().unwrap().is_empty());
    let mut sending = false;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(message)) = events.next().await {
            let Ok(text) = message.to_text() else {
                continue;
            };
            let frame: serde_json::Value = serde_json::from_str(text).unwrap();
            if frame["type"] != "job.updated" || frame["data"]["id"] != job.id {
                continue;
            }
            if frame["data"]["progress"]["phase"] == "sending"
                && frame["data"]["progress"]["bytes"].as_u64().unwrap_or(0) > 0
            {
                sending = true;
            }
            if frame["data"]["state"] == "succeeded" {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(sending);
}

#[tokio::test]
async fn ending_or_revoking_lock_owner_allows_new_jobs() {
    use serde_json::{Value, json};
    for action in ["end", "revoke", "reset"] {
        let td = TestCompanion::start().await;
        let id = td.plug_device("mocka").await;
        let owner: Value = td
            .raw()
            .post("/api/v1/auth/sessions")
            .bearer_auth(&td.pairing_token)
            .json(&json!({"kind":"window"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = owner["token"].as_str().unwrap();
        assert_eq!(
            td.raw()
                .post("/api/v1/system/lock")
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            td.request::<Update>(FirmwareTarget { device: id.clone() }, NoQuery, &request())
                .send()
                .await
                .unwrap()
                .status(),
            503
        );
        let response = match action {
            "end" => td
                .raw()
                .request(reqwest::Method::DELETE, "/api/v1/auth/sessions/self")
                .bearer_auth(token)
                .send()
                .await
                .unwrap(),
            "revoke" => td
                .raw()
                .request(
                    reqwest::Method::DELETE,
                    &format!("/api/v1/auth/sessions/{}", owner["id"].as_str().unwrap()),
                )
                .bearer_auth(&td.token)
                .send()
                .await
                .unwrap(),
            _ => td
                .raw()
                .post("/api/v1/auth/reset")
                .bearer_auth(&td.token)
                .send()
                .await
                .unwrap(),
        };
        assert_eq!(response.status(), 204);
        if action != "reset" {
            let prompt = Arc::new(Prompt::default());
            prompt.answer(false);
            td.firmware_prompter(prompt);
            let job = td
                .call::<Repair>(FirmwareTarget { device: id }, NoQuery, request())
                .await
                .unwrap();
            assert_eq!(td.wait(&job).await.error.unwrap().code, "firmware.declined");
        } else {
            let session: Value = td.raw().post("/api/v1/auth/sessions").bearer_auth(td.current_pairing_token()).json(&json!({"kind":"window"})).send().await.unwrap().json().await.unwrap();
            let response = td.raw().post("/api/v1/system/lock").bearer_auth(session["token"].as_str().unwrap()).send().await.unwrap();
            assert_eq!(response.status(), 200);
        }
    }
}

#[tokio::test]
async fn overlapping_file_pickers_are_rejected() {
    #[derive(Default)]
    struct Picker(Prompt);
    impl Prompter for Picker {
        fn pick(&self) -> Result<Option<Vec<u8>>, String> {
            self.0.confirm(&Confirmation {
                operation: "pick".into(),
                target: String::new(),
                name: String::new(),
                port: String::new(),
                current: String::new(),
                version: String::new(),
                build: String::new(),
            });
            Ok(None)
        }
        fn confirm(&self, _: &Confirmation) -> bool {
            false
        }
    }
    let td = TestCompanion::start().await;
    let id = td.plug_device("mocka").await;
    let prompt = Arc::new(Picker::default());
    td.firmware_prompter(prompt.clone());
    let first = td.request::<Pick>((), NoQuery, &FirmwareTarget { device: id.clone() });
    let task = tokio::spawn(async move { first.send().await.unwrap() });
    td.until(|_| !prompt.0.seen.lock().unwrap().is_empty())
        .await;
    let second = td
        .request::<Pick>((), NoQuery, &FirmwareTarget { device: id })
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 409);
    assert_eq!(
        second.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "system.dialog_open"
    );
    prompt.0.answer(false);
    assert!(task.await.unwrap().status().is_success());
}

#[tokio::test]
async fn browser_cannot_take_install_lock() {
    let td = TestCompanion::start().await;
    let response = td.raw().post("/api/v1/system/lock").bearer_auth(td.browser_token()).send().await.unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(response.json::<serde_json::Value>().await.unwrap()["error"]["code"], "auth.forbidden");
}
