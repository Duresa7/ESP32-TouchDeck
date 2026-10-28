use serde_json::{Value, json};
use std::{
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn launch(root: &std::path::Path) -> Process {
    Process(
        Command::new(env!("CARGO_BIN_EXE_td-companion"))
            .args(["--mock-devices", "2"])
            .env("TOUCHDECK_APP_DATA_DIR", root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
async fn runtime(root: &std::path::Path) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(bytes) = std::fs::read(root.join("runtime/companion.json"))
                && let Ok(value) = serde_json::from_slice(&bytes)
            {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap()
}
async fn session(client: &reqwest::Client, url: &str, runtime: &Value) -> String {
    client
        .post(format!("{url}/api/v1/auth/sessions"))
        .bearer_auth(runtime["token"].as_str().unwrap())
        .json(&json!({"kind":"window"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .into()
}
async fn stop(child: &mut Process, client: &reqwest::Client, url: &str, token: &str) {
    assert_eq!(
        client
            .post(format!("{url}/api/v1/system/shutdown"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn process_single_instance_runtime_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut first = launch(root.path());
    let info = runtime(root.path()).await;
    let url = format!("http://127.0.0.1:{}", info["port"]);
    let client = reqwest::Client::new();
    let token = session(&client, &url, &info).await;
    #[cfg(windows)]
    {
        let script = r#"$ErrorActionPreference='Stop'; $acl=Get-Acl -LiteralPath $env:TD_RUNTIME; $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; $owner=$acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value; $rules=$acl.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]); if (!$acl.AreAccessRulesProtected -or $owner -ne $sid -or $rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $sid -or $rules[0].AccessControlType -ne 'Allow') {exit 1}"#;
        assert!(
            Command::new("pwsh")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("TD_RUNTIME", root.path().join("runtime/companion.json"))
                .status()
                .unwrap()
                .success()
        );
    }
    let mut second = launch(root.path());
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = second.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert!(!status.success());
    assert!(
        client
            .get(format!("{url}/api/v1/system/ping"))
            .send()
            .await
            .unwrap()
            .status()
            .is_success()
    );
    stop(&mut first, &client, &url, &token).await;
    assert!(!root.path().join("runtime/companion.json").exists());
    let mut restarted = launch(root.path());
    let next = runtime(root.path()).await;
    assert_ne!(info["token"], next["token"]);
    assert_eq!(info["port"], next["port"]);
    assert_eq!(
        client
            .get(format!("{url}/api/v1/devices"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let new_token = session(&client, &url, &next).await;
    stop(&mut restarted, &client, &url, &new_token).await;
    #[cfg(windows)]
    {
        std::fs::remove_dir(root.path().join("runtime")).unwrap();
        let target = tempfile::tempdir().unwrap();
        // Both paths belong to this test's temporary folders. No shell interpolation.
        let script =
            "New-Item -ItemType Junction -Path $env:TD_LINK -Target $env:TD_TARGET | Out-Null";
        assert!(
            Command::new("pwsh")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("TD_LINK", root.path().join("runtime"))
                .env("TD_TARGET", target.path())
                .status()
                .unwrap()
                .success()
        );
        let mut rejected = launch(root.path());
        let status = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(status) = rejected.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap();
        assert!(!status.success());
        assert!(!target.path().join("companion.json").exists());
        std::fs::remove_dir(root.path().join("runtime")).unwrap();
    }
}
