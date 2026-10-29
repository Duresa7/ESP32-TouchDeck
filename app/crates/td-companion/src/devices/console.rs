//! The read-only console allowlist from the contract. The Companion checks every
//! `console.send` line here before it reaches a Device.

/// Characters that quote, escape or separate commands. None may appear in an allowed line.
const SPECIAL: &[char] = &['"', '\'', '\\', '`', ';', '&', '|', '$', '<', '>', '#'];

/// Whether `line` is one read-only command the allowlist names, with exactly its arguments.
pub fn allowed(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.is_empty() || bytes.len() > 256 || !bytes.iter().all(|b| (0x20..=0x7e).contains(b)) {
        return false;
    }
    if line.contains(SPECIAL) {
        return false;
    }
    let words: Vec<&str> = line.split(' ').collect();
    if words.iter().any(|w| w.is_empty()) {
        return false;
    }
    matches!(
        words.as_slice(),
        ["help" | "version" | "heap" | "uptime" | "backlight" | "screenshot"]
            | [
                "link" | "ui" | "layout" | "power" | "recovery" | "update",
                "state"
            ]
            | ["settings", "get"]
            | ["store", "stat"]
            | ["ui", "tile" | "widget" | "dashboard", _]
            | ["store", "stat" | "ls" | "get", _]
            | ["screenshot", "1" | "2" | "4"]
    )
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn every_allowlisted_form_passes() {
        for line in [
            "help",
            "version",
            "heap",
            "uptime",
            "link state",
            "ui state",
            "ui tile launch",
            "ui widget cpu_graph",
            "ui dashboard home",
            "layout state",
            "power state",
            "recovery state",
            "update state",
            "settings get",
            "store stat",
            "store stat /layouts",
            "store ls /assets",
            "store get /layouts/landscape.json",
            "backlight",
            "screenshot",
            "screenshot 1",
            "screenshot 2",
            "screenshot 4",
        ] {
            assert!(allowed(line), "{line}");
        }
    }

    #[test]
    fn everything_else_is_forbidden() {
        let long = format!("store get /{}", "a".repeat(250));
        for line in [
            "",
            "keys a",
            "touch tap 10 10",
            "ui hold launch",
            "ui tile",
            "ui tile a b",
            "reboot",
            "perf",
            "settings set idle 1",
            "store put /x",
            "backlight 10",
            "screenshot 3",
            "version extra",
            "version\n",
            "version\r",
            "help\tversion",
            "version\u{7f}",
            "ui tile caf\u{e9}",
            " version",
            "version ",
            "link  state",
            "store get \"/a b\"",
            "store get /a;reboot",
            "store get /a\\b",
            "store get $HOME",
            "VERSION",
            &long,
        ] {
            assert!(!allowed(line), "{line:?}");
        }
        assert!(long.len() > 256);
    }
}
