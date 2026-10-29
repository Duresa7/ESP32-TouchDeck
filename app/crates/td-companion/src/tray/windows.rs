use super::*;
use std::{
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
pub(super) fn start(ctx: Ctx) {
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
                        let exe = std::env::var_os("TOUCHDECK_WINDOW_BIN")
                            .map(std::path::PathBuf::from)
                            .or_else(|| {
                                std::env::current_exe()
                                    .ok()
                                    .map(|p| p.with_file_name("Open TouchDeck.exe"))
                            });
                        if let Some(exe) = exe {
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
