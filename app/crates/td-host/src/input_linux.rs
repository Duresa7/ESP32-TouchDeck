//! A virtual keyboard through `/dev/uinput`, set up with the legacy `uinput_user_dev` write so it
//! works on every kernel that has uinput.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;

use crate::input::{InputSink, KeyEvent};
use crate::keymap;

const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;
const UI_SET_EVBIT: u64 = 0x4004_5564;
const UI_SET_KEYBIT: u64 = 0x4004_5565;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;
const BUS_USB: u16 = 0x03;
const VENDOR: u16 = 0x303A;
const PRODUCT: u16 = 0x1001;
/// `name[80]`, `input_id`, `ff_effects_max`, and four `absmax`-style arrays of 64 ints.
const USER_DEV_SIZE: usize = 80 + 8 + 4 + 4 * 64 * 4;

pub struct UinputSink {
    file: File,
}

impl UinputSink {
    pub fn open() -> Result<UinputSink, String> {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .map_err(|error| format!("/dev/uinput: {error}"))?;
        let fd = file.as_raw_fd();
        let ioctl = |request: u64, value: libc::c_int| -> Result<(), String> {
            if unsafe { libc::ioctl(fd, request as _, value) } < 0 {
                return Err(format!(
                    "uinput ioctl {request:#x}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(())
        };
        ioctl(UI_SET_EVBIT, libc::c_int::from(EV_KEY))?;
        for (_, os) in keymap::all() {
            ioctl(UI_SET_KEYBIT, libc::c_int::from(os.evdev))?;
        }
        let mut dev = vec![0u8; USER_DEV_SIZE];
        dev[..9].copy_from_slice(b"TouchDeck");
        dev[80..82].copy_from_slice(&BUS_USB.to_ne_bytes());
        dev[82..84].copy_from_slice(&VENDOR.to_ne_bytes());
        dev[84..86].copy_from_slice(&PRODUCT.to_ne_bytes());
        (&file)
            .write_all(&dev)
            .map_err(|error| format!("uinput setup: {error}"))?;
        if unsafe { libc::ioctl(fd, UI_DEV_CREATE as _) } < 0 {
            return Err(format!(
                "uinput create: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(UinputSink { file })
    }

    fn write_event(&mut self, kind: u16, code: u16, value: i32) -> std::io::Result<()> {
        let mut event = [0u8; size_of::<libc::timeval>() + 8];
        let at = size_of::<libc::timeval>();
        event[at..at + 2].copy_from_slice(&kind.to_ne_bytes());
        event[at + 2..at + 4].copy_from_slice(&code.to_ne_bytes());
        event[at + 4..at + 8].copy_from_slice(&value.to_ne_bytes());
        self.file.write_all(&event)
    }
}

impl InputSink for UinputSink {
    fn send(&mut self, event: KeyEvent) -> Result<(), String> {
        let (code, value) = match event {
            KeyEvent::Down(code) => (code, 1),
            KeyEvent::Up(code) => (code, 0),
        };
        let os = keymap::lookup(code).ok_or_else(|| format!("no evdev code for {code:?}"))?;
        self.write_event(EV_KEY, os.evdev, value)
            .and_then(|()| self.write_event(EV_SYN, SYN_REPORT, 0))
            .map_err(|error| format!("uinput write: {error}"))
    }
}

impl Drop for UinputSink {
    fn drop(&mut self) {
        unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY as _) };
    }
}
