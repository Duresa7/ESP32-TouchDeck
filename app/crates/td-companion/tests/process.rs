use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn launch(root: &Path) -> Process {
    Process(
        Command::new(env!("CARGO_BIN_EXE_td-companion"))
            .args(["--mock-devices", "2"])
            .env("TOUCHDECK_APP_DATA_DIR", root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}
/// Waits for a Companion that should refuse to start, and returns its status and stderr.
async fn refused(mut child: Process) -> (ExitStatus, String) {
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("the Companion should exit");
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    (status, stderr)
}
async fn runtime(root: &Path) -> Value {
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
fn url(info: &Value) -> String {
    format!("http://127.0.0.1:{}", info["port"])
}
async fn session(client: &reqwest::Client, info: &Value) -> String {
    client
        .post(format!("{}/api/v1/auth/sessions", url(info)))
        .bearer_auth(info["token"].as_str().unwrap())
        .json(&json!({"kind": "window"}))
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
async fn pings(client: &reqwest::Client, info: &Value) -> bool {
    client
        .get(format!("{}/api/v1/system/ping", url(info)))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}
async fn stop(child: &mut Process, client: &reqwest::Client, info: &Value, token: &str) {
    let response = client
        .post(format!("{}/api/v1/system/shutdown", url(info)))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    tokio::time::timeout(Duration::from_secs(10), async {
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
#[cfg(windows)]
fn pwsh(script: &str, vars: &[(&str, &Path)]) -> bool {
    let mut command = Command::new("pwsh");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    for (name, value) in vars {
        command.env(name, value);
    }
    command.status().unwrap().success()
}
/// The path is owned by the user and has a protected DACL with one allow entry for the user.
#[cfg(windows)]
fn owner_only(path: &Path) -> bool {
    let script = "$ErrorActionPreference='Stop'; $acl=Get-Acl -LiteralPath $env:TD_PATH; $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; $owner=$acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value; $rules=$acl.GetAccessRules($true,$true,[System.Security.Principal.SecurityIdentifier]); if (!$acl.AreAccessRulesProtected -or $owner -ne $sid -or $rules.Count -ne 1 -or $rules[0].IdentityReference.Value -ne $sid -or $rules[0].AccessControlType -ne 'Allow') {exit 1}";
    pwsh(script, &[("TD_PATH", path)])
}

#[tokio::test]
async fn one_companion_per_runtime_folder() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let mut first = launch(root.path());
    let info = runtime(root.path()).await;
    let (status, stderr) = refused(launch(root.path())).await;
    assert!(!status.success());
    assert!(
        stderr.contains("The Companion is already running."),
        "{stderr}"
    );
    assert!(pings(&client, &info).await);
    // Another data folder has its own runtime folder and lock.
    let mut elsewhere = launch(other.path());
    let other_info = runtime(other.path()).await;
    assert!(pings(&client, &other_info).await);
    let token = session(&client, &other_info).await;
    stop(&mut elsewhere, &client, &other_info, &token).await;
    let token = session(&client, &info).await;
    stop(&mut first, &client, &info, &token).await;
}

#[tokio::test]
async fn runtime_file_lifecycle_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let mut first = launch(root.path());
    let info = runtime(root.path()).await;
    let token = session(&client, &info).await;
    #[cfg(windows)]
    {
        assert!(owner_only(&root.path().join("runtime/companion.json")));
        assert!(owner_only(&root.path().join("runtime")));
    }
    // Reset pairing rewrites the file with a new pairing token at once.
    let reset = client
        .post(format!("{}/api/v1/auth/reset", url(&info)))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(reset.status(), 204);
    let rotated = runtime(root.path()).await;
    assert_ne!(rotated["token"], info["token"]);
    assert_eq!(rotated["port"], info["port"]);
    let token = session(&client, &rotated).await;
    stop(&mut first, &client, &info, &token).await;
    assert!(!root.path().join("runtime/companion.json").exists());
    let mut restarted = launch(root.path());
    let next = runtime(root.path()).await;
    assert_ne!(info["token"], next["token"]);
    assert_eq!(info["port"], next["port"]);
    let old = client
        .get(format!("{}/api/v1/devices", url(&info)))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(old.status(), 401);
    let token = session(&client, &next).await;
    stop(&mut restarted, &client, &next, &token).await;
}

#[cfg(windows)]
#[tokio::test]
async fn a_junction_at_runtime_refuses_start() {
    let root = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    // Both paths belong to this test's temporary folders. No shell interpolation.
    assert!(pwsh(
        "New-Item -ItemType Junction -Path $env:TD_LINK -Target $env:TD_TARGET | Out-Null",
        &[
            ("TD_LINK", &root.path().join("runtime")),
            ("TD_TARGET", target.path())
        ],
    ));
    let (status, stderr) = refused(launch(root.path())).await;
    assert!(!status.success());
    assert!(stderr.contains("reparse point"), "{stderr}");
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 0);
    std::fs::remove_dir(root.path().join("runtime")).unwrap();
}

#[cfg(windows)]
#[tokio::test]
async fn a_folder_with_only_modify_access_works() {
    // The user holds only inherited Modify here, so no WRITE_OWNER.
    let restricted = tempfile::tempdir().unwrap();
    assert!(pwsh(
        "$sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; icacls $env:TD_DIR /inheritance:r /grant:r \"*$($sid):(OI)(CI)M\" | Out-Null; exit $LASTEXITCODE",
        &[("TD_DIR", restricted.path())],
    ));
    let client = reqwest::Client::new();
    let mut companion = launch(restricted.path());
    let info = runtime(restricted.path()).await;
    assert!(owner_only(&restricted.path().join("runtime")));
    let token = session(&client, &info).await;
    stop(&mut companion, &client, &info, &token).await;
}

/// Making another principal the owner needs an elevated token, as on CI runners.
#[cfg(windows)]
#[tokio::test]
async fn a_runtime_folder_another_account_owns_refuses_start() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("runtime");
    std::fs::create_dir(&dir).unwrap();
    // S-1-5-32-544 is the local Administrators group.
    if !pwsh(
        "icacls $env:TD_DIR /setowner '*S-1-5-32-544' 2>&1 | Out-Null; exit $LASTEXITCODE",
        &[("TD_DIR", &dir)],
    ) {
        // CI runners are elevated, so the check always runs there.
        assert!(
            std::env::var_os("CI").is_none(),
            "CI must be able to set a foreign owner"
        );
        eprintln!("skipped: setting a foreign owner needs an elevated token");
        return;
    }
    let (status, stderr) = refused(launch(root.path())).await;
    assert!(!status.success());
    assert!(stderr.contains("another account owns it"), "{stderr}");
    assert!(pwsh(
        "$acl=Get-Acl -LiteralPath $env:TD_DIR; if ($acl.Owner -notmatch 'Administrators') {exit 1}",
        &[("TD_DIR", &dir)],
    ));
    assert!(!dir.join("companion.json").exists());
}
