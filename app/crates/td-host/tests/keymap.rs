use std::collections::HashSet;

use td_host::keymap::{self, Code, OsKey, consumer, keyboard};

fn vk(usage: u8) -> u16 {
    keyboard(usage).unwrap().vk
}

fn evdev(usage: u8) -> u16 {
    keyboard(usage).unwrap().evdev
}

#[test]
fn letters_map_to_their_virtual_key_and_evdev_code() {
    for (i, usage) in (0x04..=0x1D).enumerate() {
        assert_eq!(vk(usage), u16::from(b'A') + i as u16);
    }
    assert_eq!((evdev(0x04), evdev(0x14), evdev(0x1D)), (30, 16, 44));
}

#[test]
fn digits_run_one_to_nine_then_zero() {
    assert_eq!((vk(0x1E), evdev(0x1E)), (u16::from(b'1'), 2));
    assert_eq!((vk(0x26), evdev(0x26)), (u16::from(b'9'), 10));
    assert_eq!((vk(0x27), evdev(0x27)), (u16::from(b'0'), 11));
}

#[test]
fn f1_to_f12_map_to_vk_f1_and_their_evdev_codes() {
    for (i, usage) in (0x3A..=0x45).enumerate() {
        assert_eq!(vk(usage), 0x70 + i as u16);
    }
    assert_eq!(
        (evdev(0x3A), evdev(0x43), evdev(0x44), evdev(0x45)),
        (59, 68, 87, 88)
    );
}

#[test]
fn f13_to_f24_map_to_vk_f13_and_key_f13() {
    for (i, usage) in (0x68..=0x73).enumerate() {
        assert_eq!(vk(usage), 0x7C + i as u16);
        assert_eq!(evdev(usage), 183 + i as u16);
        assert_eq!(keymap::name(Code::Keyboard(usage)), format!("F{}", 13 + i));
    }
}

#[test]
fn punctuation_and_editing_keys_map() {
    let expected = [
        (0x28, 0x0D, 28),
        (0x29, 0x1B, 1),
        (0x2A, 0x08, 14),
        (0x2B, 0x09, 15),
        (0x2C, 0x20, 57),
        (0x2D, 0xBD, 12),
        (0x2E, 0xBB, 13),
        (0x2F, 0xDB, 26),
        (0x30, 0xDD, 27),
        (0x31, 0xDC, 43),
        (0x33, 0xBA, 39),
        (0x34, 0xDE, 40),
        (0x35, 0xC0, 41),
        (0x36, 0xBC, 51),
        (0x37, 0xBE, 52),
        (0x38, 0xBF, 53),
        (0x39, 0x14, 58),
    ];
    for (usage, v, e) in expected {
        assert_eq!((vk(usage), evdev(usage)), (v, e), "usage {usage:#x}");
    }
}

#[test]
fn navigation_keys_are_extended() {
    let expected = [
        (0x49, 0x2D, 110),
        (0x4A, 0x24, 102),
        (0x4B, 0x21, 104),
        (0x4C, 0x2E, 111),
        (0x4D, 0x23, 107),
        (0x4E, 0x22, 109),
        (0x4F, 0x27, 106),
        (0x50, 0x25, 105),
        (0x51, 0x28, 108),
        (0x52, 0x26, 103),
    ];
    for (usage, v, e) in expected {
        assert_eq!(
            keyboard(usage),
            Some(OsKey {
                vk: v,
                evdev: e,
                extended: true
            }),
            "usage {usage:#x}"
        );
    }
}

#[test]
fn modifiers_map_left_and_right() {
    let expected = [
        (0xE0, 0xA2, 29),
        (0xE1, 0xA0, 42),
        (0xE2, 0xA4, 56),
        (0xE3, 0x5B, 125),
        (0xE4, 0xA3, 97),
        (0xE5, 0xA1, 54),
        (0xE6, 0xA5, 100),
        (0xE7, 0x5C, 126),
    ];
    for (usage, v, e) in expected {
        assert_eq!((vk(usage), evdev(usage)), (v, e), "usage {usage:#x}");
    }
}

#[test]
fn the_six_consumer_usages_map_to_media_and_volume_keys() {
    let expected = [
        (0xB5, 0xB0, 163),
        (0xB6, 0xB1, 165),
        (0xCD, 0xB3, 164),
        (0xE2, 0xAD, 113),
        (0xE9, 0xAF, 115),
        (0xEA, 0xAE, 114),
    ];
    for (usage, v, e) in expected {
        let os = consumer(usage).unwrap();
        assert_eq!((os.vk, os.evdev), (v, e), "consumer {usage:#x}");
    }
    assert_eq!(consumer(0), None);
    assert_eq!(consumer(0xB7), None);
}

#[test]
fn unmapped_usages_are_none() {
    for usage in [0x00, 0x01, 0x03, 0x32, 0x53, 0x67, 0x74, 0xDF, 0xE8, 0xFF] {
        assert_eq!(keyboard(usage), None, "usage {usage:#x}");
    }
}

#[test]
fn no_two_codes_share_a_virtual_key_or_an_evdev_code() {
    let all: Vec<_> = keymap::all().collect();
    let vks: HashSet<u16> = all.iter().map(|(_, os)| os.vk).collect();
    let evdevs: HashSet<u16> = all.iter().map(|(_, os)| os.evdev).collect();
    assert_eq!(vks.len(), all.len());
    assert_eq!(evdevs.len(), all.len());
    assert_eq!(all.len(), 26 + 10 + 17 + 12 + 3 + 10 + 12 + 8 + 6);
}
