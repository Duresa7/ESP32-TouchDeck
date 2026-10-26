use std::mem::size_of;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
    MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

use crate::input::{InputSink, KeyEvent};
use crate::keymap::{self, Code};

pub struct SendInputSink;

impl InputSink for SendInputSink {
    fn send(&mut self, event: KeyEvent) -> Result<(), String> {
        let (code, up) = match event {
            KeyEvent::Down(code) => (code, false),
            KeyEvent::Up(code) => (code, true),
        };
        let os = keymap::lookup(code).ok_or_else(|| format!("no virtual key for {code:?}"))?;
        let mut flags = 0;
        if os.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        // Consumer keys carry no scan code, so the OS treats them as the media keys they name.
        let scan = match code {
            Code::Keyboard(_) => {
                (unsafe { MapVirtualKeyW(u32::from(os.vk), MAPVK_VK_TO_VSC) }) as u16
            }
            Code::Consumer(_) => 0,
        };
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: os.vk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
        if sent == 1 {
            Ok(())
        } else {
            Err(format!("SendInput refused {code:?}"))
        }
    }
}

/// Brings `hwnd` to the foreground without sending any key, by sharing the foreground thread's
/// input state for the call. Returns whether `hwnd` is the foreground window afterwards.
///
/// # Safety
/// `hwnd` must be a window handle, or null.
pub unsafe fn bring_to_front(hwnd: HWND) -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground == hwnd {
            return true;
        }
        SetForegroundWindow(hwnd);
        GetForegroundWindow() == hwnd
    }
}
