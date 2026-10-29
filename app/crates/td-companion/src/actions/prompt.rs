use super::{ActionSpec, Answer, ApprovalRequest};
use serde::{Deserialize, Serialize};
use std::{fmt::Write, ops::RangeInclusive};
use ts_rs::TS;
pub trait ApprovalPrompter: Send + Sync {
    /// Runs on the approval thread. Must close when `expired` becomes true. An error denies.
    fn prompt(
        &self,
        request: &ApprovalRequest,
        text: &str,
        expired: &(dyn Fn() -> bool + Sync),
    ) -> Result<Answer, String>;
    fn show(&self, id: &str);
}
pub(super) struct Context {
    pub device: String,
    pub page: String,
    pub tile: String,
}
/// Every value from the Device or its Layout goes through [`shown`].
pub(super) fn dialog_text(context: &Context, spec: &ActionSpec) -> String {
    let args: Vec<_> = spec.args.iter().map(|a| shown(a)).collect();
    format!(
        "Device: {}\nPage: {}\nTile: {}\nKind: {}\nProgram: {}\nArguments: [{}]\nWorking folder: {}\nShell: {}",
        shown(&context.device),
        shown(&context.page),
        shown(&context.tile),
        spec.kind,
        shown(&spec.program),
        args.join(", "),
        shown(&spec.cwd),
        spec.shell
    )
}
/// Unicode's format characters (category Cf).
const FORMAT: &[RangeInclusive<char>] = &[
    '\u{AD}'..='\u{AD}',
    '\u{600}'..='\u{605}',
    '\u{61C}'..='\u{61C}',
    '\u{6DD}'..='\u{6DD}',
    '\u{70F}'..='\u{70F}',
    '\u{890}'..='\u{891}',
    '\u{8E2}'..='\u{8E2}',
    '\u{180E}'..='\u{180E}',
    '\u{200B}'..='\u{200F}',
    '\u{202A}'..='\u{202E}',
    '\u{2060}'..='\u{2064}',
    '\u{2066}'..='\u{206F}',
    '\u{FEFF}'..='\u{FEFF}',
    '\u{FFF9}'..='\u{FFFB}',
    '\u{110BD}'..='\u{110BD}',
    '\u{110CD}'..='\u{110CD}',
    '\u{13430}'..='\u{1343F}',
    '\u{1BCA0}'..='\u{1BCA3}',
    '\u{1D173}'..='\u{1D17A}',
    '\u{E0001}'..='\u{E0001}',
    '\u{E0020}'..='\u{E007F}',
];
/// Quotes `value` and writes every control, format, line separator and paragraph separator
/// character as `\u{...}`, so nothing in it is invisible or reorders the text around it.
pub(super) fn shown(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\u{2028}' | '\u{2029}' => escape(&mut out, c),
            c if c.is_control() || FORMAT.iter().any(|r| r.contains(&c)) => escape(&mut out, c),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn escape(out: &mut String, c: char) {
    let _ = write!(out, "\\u{{{:04X}}}", u32::from(c));
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
    /// The prompt fails to show.
    Fail,
    Never,
}
#[cfg(any(test, feature = "test-hooks"))]
#[derive(Default)]
pub struct FakePrompter {
    answers: parking_lot::Mutex<std::collections::VecDeque<ScriptAnswer>>,
    texts: parking_lot::Mutex<Vec<String>>,
}
#[cfg(any(test, feature = "test-hooks"))]
impl FakePrompter {
    pub fn script(&self, answers: Vec<ScriptAnswer>) {
        *self.answers.lock() = answers.into();
    }
    /// The text of every dialog it was asked to show, oldest first.
    pub fn texts(&self) -> Vec<String> {
        self.texts.lock().clone()
    }
}
#[cfg(any(test, feature = "test-hooks"))]
impl ApprovalPrompter for FakePrompter {
    fn prompt(
        &self,
        _: &ApprovalRequest,
        text: &str,
        expired: &(dyn Fn() -> bool + Sync),
    ) -> Result<Answer, String> {
        self.texts.lock().push(text.into());
        loop {
            if expired() {
                return Ok(Answer::Expired);
            }
            match self.answers.lock().pop_front() {
                Some(ScriptAnswer::Once) => return Ok(Answer::Once),
                Some(ScriptAnswer::Always) => return Ok(Answer::Always),
                Some(ScriptAnswer::Deny) => return Ok(Answer::Deny),
                Some(ScriptAnswer::Fail) => return Err("The scripted prompt failed.".into()),
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
    fn prompt(
        &self,
        request: &ApprovalRequest,
        _: &str,
        _: &(dyn Fn() -> bool + Sync),
    ) -> Result<Answer, String> {
        self.notifier.waiting(request);
        Err("This system has no native approval prompt.".into())
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
            text: &str,
            expired: &(dyn Fn() -> bool + Sync),
        ) -> Result<Answer, String> {
            let title = wide("Open TouchDeck - approve action");
            let heading = wide("Allow this action on this PC?");
            let content = wide(text);
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
            if expired() {
                return Ok(Answer::Expired);
            }
            if result < 0 {
                return Err(format!("The native prompt failed with {result:#x}."));
            }
            Ok(match selected {
                100 => Answer::Once,
                101 => Answer::Always,
                _ => Answer::Deny,
            })
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
#[cfg(test)]
mod tests {
    use super::shown;

    fn escaped(c: char) -> String {
        format!("\"a\\u{{{:04X}}}b\"", u32::from(c))
    }

    #[test]
    fn control_characters_are_escaped() {
        for c in [
            '\0', '\t', '\n', '\r', '\u{1B}', '\u{7F}', '\u{85}', '\u{9F}',
        ] {
            assert_eq!(shown(&format!("a{c}b")), escaped(c));
        }
    }

    #[test]
    fn direction_controls_are_escaped() {
        for c in ('\u{202A}'..='\u{202E}').chain('\u{2066}'..='\u{2069}') {
            assert_eq!(shown(&format!("a{c}b")), escaped(c));
        }
        assert_eq!(
            shown("report\u{202E}gpj.exe\u{202C}"),
            "\"report\\u{202E}gpj.exe\\u{202C}\""
        );
    }

    #[test]
    fn zero_width_and_byte_order_marks_are_escaped() {
        for c in ('\u{200B}'..='\u{200F}').chain(['\u{2060}', '\u{FEFF}', '\u{AD}']) {
            assert_eq!(shown(&format!("a{c}b")), escaped(c));
        }
    }

    #[test]
    fn other_format_characters_are_escaped() {
        for c in [
            '\u{600}',
            '\u{61C}',
            '\u{180E}',
            '\u{FFF9}',
            '\u{E0001}',
            '\u{E007F}',
        ] {
            assert_eq!(shown(&format!("a{c}b")), escaped(c));
        }
    }

    #[test]
    fn line_and_paragraph_separators_are_escaped() {
        for c in ['\u{2028}', '\u{2029}'] {
            assert_eq!(shown(&format!("a{c}b")), escaped(c));
        }
    }

    #[test]
    fn quotes_and_backslashes_cannot_forge_an_escape() {
        assert_eq!(shown(r#"say "\u{202E}""#), r#""say \"\\u{202E}\"""#);
    }

    #[test]
    fn visible_text_is_unchanged() {
        assert_eq!(
            shown("C:/Tools/run.exe --name=Zoë 日本 🙂"),
            "\"C:/Tools/run.exe --name=Zoë 日本 🙂\""
        );
    }
}
