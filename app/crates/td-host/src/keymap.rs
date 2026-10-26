//! One table from HID usages to the Windows virtual key and the Linux evdev code.

/// A key as the deck names it: a keyboard usage (modifiers are 0xE0 to 0xE7) or a consumer usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    Keyboard(u8),
    Consumer(u16),
}

/// What the operating system needs to type one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsKey {
    pub vk: u16,
    pub evdev: u16,
    /// Windows sends it with `KEYEVENTF_EXTENDEDKEY`.
    pub extended: bool,
}

const fn key(vk: u16, evdev: u16) -> Option<OsKey> {
    Some(OsKey {
        vk,
        evdev,
        extended: false,
    })
}

const fn ext(vk: u16, evdev: u16) -> Option<OsKey> {
    Some(OsKey {
        vk,
        evdev,
        extended: true,
    })
}

/// evdev codes of the letters a to z, in usage order.
const LETTERS: [u16; 26] = [
    30, 48, 46, 32, 18, 33, 34, 35, 23, 36, 37, 38, 50, 49, 24, 25, 16, 19, 31, 20, 22, 47, 17, 45,
    21, 44,
];

/// The modifier bit for a usage from 0xE0 to 0xE7, as in the HID report's modifier byte.
pub const fn modifier_usage(bit: u8) -> u8 {
    0xE0 + bit
}

pub fn lookup(code: Code) -> Option<OsKey> {
    match code {
        Code::Keyboard(usage) => keyboard(usage),
        Code::Consumer(usage) => consumer(usage),
    }
}

pub fn keyboard(usage: u8) -> Option<OsKey> {
    match usage {
        0x04..=0x1D => key(
            u16::from(b'A' + (usage - 0x04)),
            LETTERS[usize::from(usage - 0x04)],
        ),
        0x1E..=0x26 => key(
            u16::from(b'1' + (usage - 0x1E)),
            2 + u16::from(usage - 0x1E),
        ),
        0x27 => key(u16::from(b'0'), 11),
        0x28 => key(0x0D, 28),
        0x29 => key(0x1B, 1),
        0x2A => key(0x08, 14),
        0x2B => key(0x09, 15),
        0x2C => key(0x20, 57),
        0x2D => key(0xBD, 12),
        0x2E => key(0xBB, 13),
        0x2F => key(0xDB, 26),
        0x30 => key(0xDD, 27),
        0x31 => key(0xDC, 43),
        0x33 => key(0xBA, 39),
        0x34 => key(0xDE, 40),
        0x35 => key(0xC0, 41),
        0x36 => key(0xBC, 51),
        0x37 => key(0xBE, 52),
        0x38 => key(0xBF, 53),
        0x39 => key(0x14, 58),
        0x3A..=0x43 => key(0x70 + u16::from(usage - 0x3A), 59 + u16::from(usage - 0x3A)),
        0x44 => key(0x7A, 87),
        0x45 => key(0x7B, 88),
        0x46 => ext(0x2C, 99),
        0x47 => key(0x91, 70),
        0x48 => key(0x13, 119),
        0x49 => ext(0x2D, 110),
        0x4A => ext(0x24, 102),
        0x4B => ext(0x21, 104),
        0x4C => ext(0x2E, 111),
        0x4D => ext(0x23, 107),
        0x4E => ext(0x22, 109),
        0x4F => ext(0x27, 106),
        0x50 => ext(0x25, 105),
        0x51 => ext(0x28, 108),
        0x52 => ext(0x26, 103),
        0x68..=0x73 => key(
            0x7C + u16::from(usage - 0x68),
            183 + u16::from(usage - 0x68),
        ),
        0xE0 => key(0xA2, 29),
        0xE1 => key(0xA0, 42),
        0xE2 => key(0xA4, 56),
        0xE3 => ext(0x5B, 125),
        0xE4 => ext(0xA3, 97),
        0xE5 => key(0xA1, 54),
        0xE6 => ext(0xA5, 100),
        0xE7 => ext(0x5C, 126),
        _ => None,
    }
}

/// The six consumer usages the deck's media page sends.
pub fn consumer(usage: u16) -> Option<OsKey> {
    match usage {
        0xB5 => ext(0xB0, 163),
        0xB6 => ext(0xB1, 165),
        0xCD => ext(0xB3, 164),
        0xE2 => ext(0xAD, 113),
        0xE9 => ext(0xAF, 115),
        0xEA => ext(0xAE, 114),
        _ => None,
    }
}

/// Every code the table maps, for a sink that must declare its keys up front.
pub fn all() -> impl Iterator<Item = (Code, OsKey)> {
    let keyboard =
        (0..=u8::MAX).filter_map(|usage| keyboard(usage).map(|os| (Code::Keyboard(usage), os)));
    let consumer = [0xB5u16, 0xB6, 0xCD, 0xE2, 0xE9, 0xEA]
        .into_iter()
        .filter_map(|usage| consumer(usage).map(|os| (Code::Consumer(usage), os)));
    keyboard.chain(consumer)
}

/// A short name for logs and the key monitor, such as `F13` or `VolumeUp`.
pub fn name(code: Code) -> String {
    match code {
        Code::Keyboard(usage @ 0x3A..=0x45) => format!("F{}", usage - 0x3A + 1),
        Code::Keyboard(usage @ 0x68..=0x73) => format!("F{}", usage - 0x68 + 13),
        Code::Keyboard(usage @ 0x04..=0x1D) => char::from(b'A' + usage - 0x04).to_string(),
        Code::Keyboard(0xE0) => "LeftCtrl".into(),
        Code::Keyboard(0xE1) => "LeftShift".into(),
        Code::Keyboard(0xE2) => "LeftAlt".into(),
        Code::Keyboard(0xE3) => "LeftWin".into(),
        Code::Keyboard(0xE4) => "RightCtrl".into(),
        Code::Keyboard(0xE5) => "RightShift".into(),
        Code::Keyboard(0xE6) => "RightAlt".into(),
        Code::Keyboard(0xE7) => "RightWin".into(),
        Code::Keyboard(usage) => format!("key {usage}"),
        Code::Consumer(0xB5) => "NextTrack".into(),
        Code::Consumer(0xB6) => "PreviousTrack".into(),
        Code::Consumer(0xCD) => "PlayPause".into(),
        Code::Consumer(0xE2) => "Mute".into(),
        Code::Consumer(0xE9) => "VolumeUp".into(),
        Code::Consumer(0xEA) => "VolumeDown".into(),
        Code::Consumer(usage) => format!("consumer {usage}"),
    }
}
