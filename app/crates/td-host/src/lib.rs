//! What the app does on the PC for the deck: type its mirrored keys and run its tile actions.

pub mod actions;
pub mod approval;
pub mod input;
#[cfg(target_os = "linux")]
mod input_linux;
#[cfg(windows)]
mod input_windows;
pub mod keymap;
pub mod sources;

pub use approval::{Approvals, CommandSpec, Grant};
pub use input::{FakeSink, InputSink, KeyEvent, Mirror, Mirrored, Outcome};

/// Brings a window to the foreground without sending any key. Returns whether it is in front.
#[cfg(windows)]
pub fn bring_to_front(hwnd: isize) -> bool {
    // SAFETY: Win32 checks the handle and fails the calls on one that names no window.
    unsafe { input_windows::bring_to_front(hwnd as *mut std::ffi::c_void) }
}
