use super::{Answer, ApprovalRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub trait ApprovalPrompter: Send + Sync {
    /// Runs on the approval thread. Must close when `expired` becomes true.
    fn prompt(
        &self,
        request: &ApprovalRequest,
        context: &str,
        expired: &(dyn Fn() -> bool + Sync),
    ) -> Answer;
    fn show(&self, id: &str);
}
pub trait TrayNotifier: Send + Sync {
    fn waiting(&self, request: &ApprovalRequest);
}
#[derive(Default)]
pub struct NoopNotifier;
impl TrayNotifier for NoopNotifier {
    fn waiting(&self, _: &ApprovalRequest) {}
}
#[derive(Serialize, Deserialize, Clone, Copy, TS)]
#[serde(rename_all = "lowercase")]
pub enum ScriptAnswer {
    Once,
    Always,
    Deny,
    Never,
}
#[cfg(any(test, feature = "test-hooks"))]
#[derive(Default)]
pub struct FakePrompter {
    answers: parking_lot::Mutex<std::collections::VecDeque<ScriptAnswer>>,
}
#[cfg(any(test, feature = "test-hooks"))]
impl FakePrompter {
    pub fn script(&self, answers: Vec<ScriptAnswer>) {
        *self.answers.lock() = answers.into();
    }
}
#[cfg(any(test, feature = "test-hooks"))]
impl ApprovalPrompter for FakePrompter {
    fn prompt(&self, _: &ApprovalRequest, _: &str, expired: &(dyn Fn() -> bool + Sync)) -> Answer {
        loop {
            if expired() {
                return Answer::Expired;
            }
            match self.answers.lock().pop_front() {
                Some(ScriptAnswer::Once) => return Answer::Once,
                Some(ScriptAnswer::Always) => return Answer::Always,
                Some(ScriptAnswer::Deny) => return Answer::Deny,
                Some(ScriptAnswer::Never) | None => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    fn show(&self, _: &str) {}
}
pub struct NativePrompter {
    notifier: std::sync::Arc<dyn TrayNotifier>,
    #[cfg(windows)]
    active: parking_lot::Mutex<(String, isize)>,
}
impl Default for NativePrompter {
    fn default() -> Self {
        Self::new(std::sync::Arc::new(NoopNotifier))
    }
}
impl NativePrompter {
    pub fn new(notifier: std::sync::Arc<dyn TrayNotifier>) -> Self {
        Self {
            notifier,
            #[cfg(windows)]
            active: Default::default(),
        }
    }
}
#[cfg(not(windows))]
impl ApprovalPrompter for NativePrompter {
    fn prompt(&self, request: &ApprovalRequest, _: &str, _: &(dyn Fn() -> bool + Sync)) -> Answer {
        self.notifier.waiting(request);
        Answer::Deny
    }
    fn show(&self, _: &str) {}
}
#[cfg(windows)]
mod windows {
    use super::*;
    use windows_sys::Win32::{
        Foundation::HWND,
        UI::{Controls::*, WindowsAndMessaging::*},
    };
    struct Callback<'a> {
        owner: &'a NativePrompter,
        request: &'a ApprovalRequest,
        expired: &'a (dyn Fn() -> bool + Sync),
    }
    unsafe extern "system" fn callback(
        hwnd: HWND,
        msg: u32,
        _: usize,
        _: isize,
        data: isize,
    ) -> i32 {
        let context = unsafe { &*(data as *const Callback<'_>) };
        if msg == TDN_CREATED as u32 {
            *context.owner.active.lock() = (context.request.id.clone(), hwnd as isize);
            if unsafe { SetForegroundWindow(hwnd) } == 0 {
                context.owner.notifier.waiting(context.request);
            }
        }
        if msg == TDN_TIMER as u32 && (context.expired)() {
            unsafe {
                SendMessageW(hwnd, TDM_CLICK_BUTTON as u32, 102, 0);
            }
        }
        if msg == TDN_DESTROYED as u32 {
            *context.owner.active.lock() = Default::default();
        }
        0
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    impl ApprovalPrompter for NativePrompter {
        fn prompt(
            &self,
            request: &ApprovalRequest,
            context: &str,
            expired: &(dyn Fn() -> bool + Sync),
        ) -> Answer {
            let title = wide("Open TouchDeck - approve action");
            let heading = wide("Allow this action on this PC?");
            let content = wide(&format!(
                "{context}\nKind: {}\nProgram: {}\nArguments: {}\nWorking folder: {}\nShell: {}",
                request.spec.kind,
                serde_json::to_string(&request.spec.program).unwrap(),
                serde_json::to_string(&request.spec.args).unwrap(),
                serde_json::to_string(&request.spec.cwd).unwrap(),
                request.spec.shell
            ));
            let once = wide("Run once");
            let always = wide("Always allow");
            let deny = wide("Deny");
            let buttons = [
                TASKDIALOG_BUTTON {
                    nButtonID: 100,
                    pszButtonText: once.as_ptr(),
                },
                TASKDIALOG_BUTTON {
                    nButtonID: 101,
                    pszButtonText: always.as_ptr(),
                },
                TASKDIALOG_BUTTON {
                    nButtonID: 102,
                    pszButtonText: deny.as_ptr(),
                },
            ];
            let callback_data = Callback {
                owner: self,
                request,
                expired,
            };
            let config = TASKDIALOGCONFIG {
                cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
                dwFlags: TDF_ALLOW_DIALOG_CANCELLATION | TDF_CALLBACK_TIMER | TDF_SIZE_TO_CONTENT,
                pszWindowTitle: title.as_ptr(),
                pszMainInstruction: heading.as_ptr(),
                pszContent: content.as_ptr(),
                cButtons: buttons.len() as u32,
                pButtons: buttons.as_ptr(),
                nDefaultButton: 102,
                pfCallback: Some(callback),
                lpCallbackData: &callback_data as *const _ as isize,
                ..unsafe { std::mem::zeroed() }
            };
            let mut selected = 102;
            let result = unsafe {
                TaskDialogIndirect(
                    &config,
                    &mut selected,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if result < 0 || expired() {
                return if expired() {
                    Answer::Expired
                } else {
                    Answer::Deny
                };
            }
            match selected {
                100 => Answer::Once,
                101 => Answer::Always,
                _ => Answer::Deny,
            }
        }
        fn show(&self, id: &str) {
            let active = self.active.lock();
            if active.0 == id && active.1 != 0 {
                unsafe {
                    SetForegroundWindow(active.1 as HWND);
                }
            }
        }
    }
}
