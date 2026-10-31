use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub trait CommandApproval: Send + Sync {
    fn approved(&self, program: &str, args: &[String]) -> bool;
}

pub(super) fn capture(program: &str, args: &[String]) -> Result<String, &'static str> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Couldn't start the command.")?
        .block_on(capture_with_timeout(program, args, Duration::from_secs(10)))
}

async fn capture_with_timeout(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> Result<String, &'static str> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| "Couldn't start the command.")?;
    let stdout = child.stdout.take().ok_or("Couldn't read command output.")?;
    let result = tokio::time::timeout(timeout, async {
        let mut bytes = Vec::new();
        stdout
            .take(65537)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "Couldn't read command output.")?;
        if bytes.len() > 65536 {
            return Err("Command output is over 64 KiB.");
        }
        let status = child
            .wait()
            .await
            .map_err(|_| "Couldn't finish the command.")?;
        if !status.success() {
            return Err("The command failed. Check the program and arguments.");
        }
        String::from_utf8(bytes).map_err(|_| "Command output must be UTF-8.")
    })
    .await
    .unwrap_or(Err("The command exceeded 10 s and was stopped."));
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_bounds_kill_slow_process_and_pass_arguments_literally() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/custom-command.rs");
        assert!(
            std::process::Command::new("rustc")
                .arg(source)
                .arg("-o")
                .arg(&program)
                .status()
                .unwrap()
                .success()
        );
        let program = program.to_str().unwrap();
        let literal = "a & echo wrong; $(echo wrong) | > < %PATH%";
        assert_eq!(
            capture(program, &["literal".into(), literal.into()]),
            Ok(literal.into())
        );
        assert_eq!(
            capture(program, &["large".into()]),
            Err("Command output is over 64 KiB.")
        );
        let marker = dir.path().join("must-not-exist");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            runtime.block_on(capture_with_timeout(
                program,
                &["slow".into(), marker.to_string_lossy().into()],
                Duration::from_millis(100)
            )),
            Err("The command exceeded 10 s and was stopped.")
        );
        std::thread::sleep(Duration::from_millis(2100));
        assert!(!marker.exists());
    }
}
