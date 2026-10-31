use std::{
    io::Read,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub trait CommandApproval: Send + Sync {
    fn approved(&self, program: &str, args: &[String]) -> bool;
}

const TIMED_OUT: &str = "The command exceeded 10 s and was stopped.";
const MAX_OUTPUT: usize = 64 * 1024;

pub(super) fn capture(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> Result<String, &'static str> {
    let deadline = Instant::now() + timeout;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut tree = Tree::spawn(command).ok_or("Couldn't start the command.")?;
    let output = read_output(&mut tree.child, deadline).and_then(|bytes| {
        while !tree.finished() {
            if Instant::now() >= deadline {
                return Err(TIMED_OUT);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(bytes)
    });
    let status = tree.stop();
    let bytes = output?;
    if !status
        .map_err(|_| "Couldn't finish the command.")?
        .success()
    {
        return Err("The command failed. Check the program and arguments.");
    }
    String::from_utf8(bytes).map_err(|_| "Command output must be UTF-8.")
}

fn read_output(child: &mut Child, deadline: Instant) -> Result<Vec<u8>, &'static str> {
    let stdout = child.stdout.take().ok_or("Couldn't read command output.")?;
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = stdout
            .take(MAX_OUTPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sent.send(read);
    });
    let bytes = received
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| TIMED_OUT)?
        .map_err(|_| "Couldn't read command output.")?;
    if bytes.len() > MAX_OUTPUT {
        return Err("Command output is over 64 KiB.");
    }
    Ok(bytes)
}

#[cfg(unix)]
struct Tree {
    child: Child,
}
#[cfg(unix)]
impl Tree {
    fn spawn(mut command: Command) -> Option<Self> {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        command.spawn().ok().map(|child| Self { child })
    }
    fn finished(&self) -> bool {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let found = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        found == 0 && unsafe { info.si_pid() } != 0
    }
    fn stop(mut self) -> std::io::Result<ExitStatus> {
        unsafe { libc::killpg(self.child.id() as libc::pid_t, libc::SIGKILL) };
        self.child.wait()
    }
}

#[cfg(windows)]
struct Job(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}
#[cfg(windows)]
struct Tree {
    child: Child,
    job: Job,
}
#[cfg(windows)]
impl Tree {
    fn spawn(mut command: Command) -> Option<Self> {
        use std::os::windows::{io::AsRawHandle, process::CommandExt};
        use windows_sys::Win32::System::{JobObjects::*, Threading::*};
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return None;
        }
        let job = Job(job);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let limited = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of_val(&limits) as u32,
            )
        };
        if limited == 0 {
            return None;
        }
        let mut child = command.spawn().ok()?;
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0
            || !resume(child.id())
        {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        Some(Self { child, job })
    }
    fn finished(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
    fn stop(mut self) -> std::io::Result<ExitStatus> {
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job.0, 1) };
        self.child.wait()
    }
}
#[cfg(windows)]
fn resume(process: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{Diagnostics::ToolHelp::*, Threading::*},
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return false;
    }
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut resumed = false;
    let mut more = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == process {
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if !thread.is_null() {
                resumed |= unsafe { ResumeThread(thread) } != u32::MAX;
                unsafe { CloseHandle(thread) };
            }
        }
        more = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    resumed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    fn fixture() -> (tempfile::TempDir, String) {
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
        let program = program.to_str().unwrap().to_owned();
        (dir, program)
    }
    #[test]
    fn command_bounds_kill_slow_process_and_pass_arguments_literally() {
        let (dir, program) = fixture();
        let literal = "a & echo wrong; $(echo wrong) | > < %PATH%";
        let second = Duration::from_secs(10);
        assert_eq!(
            capture(&program, &["literal".into(), literal.into()], second),
            Ok(literal.into())
        );
        assert_eq!(
            capture(&program, &["large".into()], second),
            Err("Command output is over 64 KiB.")
        );
        let marker = dir.path().join("must-not-exist");
        assert_eq!(
            capture(
                &program,
                &["slow".into(), marker.to_string_lossy().into()],
                Duration::from_millis(100)
            ),
            Err(TIMED_OUT)
        );
        std::thread::sleep(Duration::from_millis(2100));
        assert!(!marker.exists());
    }
    #[test]
    fn an_endless_writer_is_cut_at_64_kib_without_waiting_for_the_timeout() {
        let (_dir, program) = fixture();
        let start = Instant::now();
        assert_eq!(
            capture(&program, &["endless".into()], Duration::from_secs(10)),
            Err("Command output is over 64 KiB.")
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
    }
    #[test]
    fn a_timeout_stops_the_processes_the_command_started() {
        let (dir, program) = fixture();
        let marker = dir.path().join("grandchild-ran");
        let start = Instant::now();
        assert_eq!(
            capture(
                &program,
                &["fork".into(), marker.to_string_lossy().into()],
                Duration::from_secs(1)
            ),
            Err(TIMED_OUT)
        );
        let returned = start.elapsed();
        std::thread::sleep(Duration::from_millis(4500).saturating_sub(start.elapsed()));
        assert!(returned < Duration::from_secs(2), "{returned:?}");
        assert!(!marker.exists());
    }
}
