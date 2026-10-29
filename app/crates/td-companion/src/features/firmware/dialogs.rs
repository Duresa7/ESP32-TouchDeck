use super::Confirmation;
use std::{
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

#[cfg(windows)]
fn powershell(script: &str, text: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("TD_DIALOG_TEXT", text)
        .creation_flags(0x08000000);
    command
}
#[cfg(windows)]
fn native(script: &str, text: &str) -> Result<String, String> {
    let output = powershell(script, text)
        .output()
        .map_err(|_| "Could not open the native dialog.".to_string())?;
    if !output.status.success() {
        return Err("The native dialog failed.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
pub fn pick() -> Result<Option<PathBuf>, String> {
    #[cfg(windows)]
    {
        let path = native(
            r#"Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'Firmware (*.tdfw)|*.tdfw'; $d.Title = 'Choose Firmware'; if ($d.ShowDialog() -eq 'OK') { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; [Console]::Write($d.FileName) }; $d.Dispose()"#,
            "",
        )?;
        Ok((!path.is_empty()).then(|| PathBuf::from(path)))
    }
    #[cfg(not(windows))]
    {
        let output = Command::new("zenity")
            .args([
                "--file-selection",
                "--title=Choose Firmware",
                "--file-filter=*.tdfw",
            ])
            .output()
            .map_err(|_| "Could not open the native dialog.".to_string())?;
        Ok(output
            .status
            .success()
            .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())))
    }
}
/// Waits for the dialog process to exit, or kills it once `closed` turns true and gives `None`.
fn answered(mut child: Child, closed: &(dyn Fn() -> bool + Sync)) -> Option<Output> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if !closed() => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
pub fn confirm(c: &Confirmation, closed: &(dyn Fn() -> bool + Sync)) -> bool {
    let text = c.text();
    #[cfg(windows)]
    let mut command = powershell(
        r#"Add-Type -AssemblyName System.Windows.Forms; [Console]::Write([System.Windows.Forms.MessageBox]::Show($env:TD_DIALOG_TEXT, 'Open TouchDeck Firmware', 'YesNo', 'Warning', 'Button2'))"#,
        &text,
    );
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("zenity");
        command.args(["--question", "--default-cancel", "--text", &text]);
        command
    };
    let Ok(child) = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    answered(child, closed).is_some_and(|output| {
        output.status.success()
            && (cfg!(not(windows)) || String::from_utf8_lossy(&output.stdout).trim() == "Yes")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn sleeper() -> Command {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("powershell.exe");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep 30",
            ]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        command.stdout(Stdio::piped());
        command
    }

    #[test]
    fn closing_kills_the_dialog_process() {
        let child = sleeper().spawn().unwrap();
        let started = Instant::now();
        let closed = || started.elapsed() > Duration::from_millis(300);
        assert!(answered(child, &closed).is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn an_answered_dialog_returns_its_output() {
        #[cfg(windows)]
        let mut command = powershell("[Console]::Write('Yes')", "");
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("echo");
            command.arg("Yes");
            command
        };
        let child = command.stdout(Stdio::piped()).spawn().unwrap();
        let output = answered(child, &|| false).unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Yes");
    }
}
