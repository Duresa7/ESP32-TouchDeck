use super::*;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{OnceLock, mpsc},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};
static NOTICES: OnceLock<mpsc::SyncSender<String>> = OnceLock::new();
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn open(url: &str) -> Result<(), CommonError> {
    let url = wide(url);
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            url.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(CommonError::Internal)
    } else {
        Ok(())
    }
}
/// The window executable tray Open starts. A packaged Companion always starts the
/// `Open TouchDeck.exe` beside it; only development honors `TOUCHDECK_WINDOW_BIN`.
fn window_exe(packaged: bool, dev_override: Option<OsString>, companion: &Path) -> PathBuf {
    match dev_override {
        Some(path) if !packaged => PathBuf::from(path),
        _ => companion.with_file_name("Open TouchDeck.exe"),
    }
}
pub(super) fn notify(message: &str) {
    if let Some(tx) = NOTICES.get() {
        let _ = tx.try_send(message.into());
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
fn copy_wide<const N: usize>(out: &mut [u16; N], value: &str) {
    for (dest, src) in out.iter_mut().take(N - 1).zip(value.encode_utf16()) {
        *dest = src;
    }
}
pub(super) fn start(ctx: Ctx, packaged: bool) {
    let handle = tokio::runtime::Handle::current();
    let (tx, rx) = mpsc::sync_channel(16);
    let _ = NOTICES.set(tx);
    std::thread::spawn(move || unsafe {
        let class = wide("OpenTouchDeckTray");
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return;
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        let mut icon: NOTIFYICONDATAW = std::mem::zeroed();
        icon.cbSize = std::mem::size_of_val(&icon) as u32;
        icon.hWnd = hwnd;
        icon.uID = 1;
        icon.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        icon.uCallbackMessage = WM_APP + 1;
        icon.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        copy_wide(&mut icon.szTip, "Open TouchDeck");
        Shell_NotifyIconW(NIM_ADD, &icon);
        SetTimer(hwnd, 1, 100, None);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if *ctx.shutdown.borrow() {
                break;
            }
            if let Ok(message) = rx.try_recv() {
                icon.uFlags = NIF_INFO;
                icon.szInfo.fill(0);
                copy_wide(&mut icon.szInfo, &message);
                copy_wide(&mut icon.szInfoTitle, "Open TouchDeck");
                icon.dwInfoFlags = NIIF_INFO;
                Shell_NotifyIconW(NIM_MODIFY, &icon);
                icon.szInfo.fill(0);
            }
            if msg.message == WM_APP + 1
                && (msg.lParam as u32 == WM_RBUTTONUP || msg.lParam as u32 == WM_LBUTTONUP)
            {
                let menu = CreatePopupMenu();
                for (id, label) in [
                    (1, "Open"),
                    (2, "Open in browser"),
                    (3, "Reset pairing"),
                    (4, "Quit"),
                ] {
                    AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
                }
                let mut pos: POINT = std::mem::zeroed();
                GetCursorPos(&mut pos);
                SetForegroundWindow(hwnd);
                let action = TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_NONOTIFY,
                    pos.x,
                    pos.y,
                    0,
                    hwnd,
                    null(),
                );
                DestroyMenu(menu);
                match action {
                    1 => {
                        if let Ok(companion) = std::env::current_exe() {
                            let dev_override = std::env::var_os("TOUCHDECK_WINDOW_BIN");
                            let exe = window_exe(packaged, dev_override, &companion);
                            let _ = std::process::Command::new(exe).spawn();
                        }
                    }
                    2 => {
                        let ctx = ctx.clone();
                        handle.spawn(async move {
                            let _ = ctx.pair_browser().await;
                        });
                    }
                    3 => {
                        let ctx = ctx.clone();
                        handle.spawn(async move {
                            let _ = ctx.reset_pairing().await;
                        });
                    }
                    4 => {
                        let _ = ctx.shutdown.send(true);
                    }
                    _ => {}
                }
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Shell_NotifyIconW(NIM_DELETE, &icon);
        KillTimer(hwnd, 1);
        DestroyWindow(hwnd);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPANION: &str = r"C:\Program Files\Open TouchDeck\td-companion.exe";
    const ADJACENT: &str = r"C:\Program Files\Open TouchDeck\Open TouchDeck.exe";
    const OVERRIDE: &str = r"D:\dev\electron.exe";

    #[test]
    fn packaged_open_ignores_the_dev_override() {
        let companion = Path::new(COMPANION);
        assert_eq!(
            window_exe(true, Some(OVERRIDE.into()), companion),
            Path::new(ADJACENT)
        );
        assert_eq!(window_exe(true, None, companion), Path::new(ADJACENT));
    }

    #[test]
    fn development_open_uses_the_override_when_set() {
        let companion = Path::new(COMPANION);
        assert_eq!(
            window_exe(false, Some(OVERRIDE.into()), companion),
            Path::new(OVERRIDE)
        );
        assert_eq!(window_exe(false, None, companion), Path::new(ADJACENT));
    }
}
