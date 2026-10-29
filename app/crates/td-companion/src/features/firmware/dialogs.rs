use super::Confirmation;
use std::path::PathBuf;

#[cfg(windows)]
fn native(script: &str, text: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
        .env("TD_DIALOG_TEXT", text)
        .creation_flags(0x08000000)
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
        let output = std::process::Command::new("zenity")
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
pub fn confirm(c: &Confirmation) -> bool {
    let text = format!(
        "{} Firmware?\nTarget: {} ({})\nPort: {}\nCurrent: {}\nBundle: {}\nBuild: {}",
        c.operation, c.name, c.target, c.port, c.current, c.version, c.build
    );
    #[cfg(windows)]
    {
        native(r#"Add-Type -AssemblyName System.Windows.Forms; [Console]::Write([System.Windows.Forms.MessageBox]::Show($env:TD_DIALOG_TEXT, 'Open TouchDeck Firmware', 'YesNo', 'Warning', 'Button2'))"#, &text).is_ok_and(|s| s == "Yes")
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("zenity")
            .args(["--question", "--default-cancel", "--text", &text])
            .status()
            .is_ok_and(|s| s.success())
    }
}
