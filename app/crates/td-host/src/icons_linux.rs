//! App icons on Linux: the `Icon=` of the `.desktop` file whose `Exec=` names the path,
//! resolved through the icon theme and rendered from SVG or decoded from PNG.

use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::icon_pack;

const RENDER: u32 = 256;

pub fn app_icon(path: &Path) -> Result<RgbaImage, String> {
    let icon = desktop_icon(path)
        .ok_or_else(|| format!("{}: no .desktop file names this program", path.display()))?;
    let file = resolve(&icon).ok_or_else(|| format!("{icon}: not found in the icon theme"))?;
    load(&file)
}

/// The directories that hold `.desktop` files, per the XDG base directory spec.
fn application_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let shared = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(shared.split(':').map(PathBuf::from))
        .map(|dir| dir.join("applications"))
        .collect()
}

/// Reads `Exec=` and `Icon=` from a desktop entry's `[Desktop Entry]` group.
pub fn parse_desktop(text: &str) -> (Option<String>, Option<String>) {
    let (mut exec, mut icon, mut group) = (None, None, false);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            group = line == "[Desktop Entry]";
        } else if group {
            if let Some(value) = line.strip_prefix("Exec=") {
                exec.get_or_insert_with(|| value.to_owned());
            } else if let Some(value) = line.strip_prefix("Icon=") {
                icon.get_or_insert_with(|| value.to_owned());
            }
        }
    }
    (exec, icon)
}

/// Whether an `Exec=` line runs `path`, by full path or by the program's name on `PATH`.
pub fn exec_names(exec: &str, path: &Path) -> bool {
    let Some(words) = exec_words(exec) else {
        return false;
    };
    let mut words = words.iter().map(String::as_str);
    let Some(first) = words.next() else {
        return false;
    };
    let program = if Path::new(first)
        .file_name()
        .is_some_and(|name| name == "env")
    {
        env_program(words)
    } else {
        Some(first)
    };
    let Some(program) = program else {
        return false;
    };
    let program = Path::new(program);
    program == path
        || (program.components().count() == 1 && path.file_name() == Some(program.as_os_str()))
}

/// Split desktop-entry arguments, keeping whitespace inside quoted executable paths.
fn exec_words(exec: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut present = false;
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                present = true;
            }
            '\\' if quoted => {
                word.push(chars.next()?);
                present = true;
            }
            ch if ch.is_whitespace() && !quoted => {
                if present {
                    words.push(std::mem::take(&mut word));
                    present = false;
                }
            }
            ch => {
                word.push(ch);
                present = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if present {
        words.push(word);
    }
    Some(words)
}

/// `env` is a launcher; its assignments and options precede the app executable.
fn env_program<'a>(mut words: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    while let Some(word) = words.next() {
        match word {
            "--" => return words.next(),
            "-i" | "--ignore-environment" => continue,
            "-u" | "--unset" | "-C" | "--chdir" => {
                words.next()?;
            }
            _ if word.starts_with("--unset=") || word.starts_with("--chdir=") => continue,
            _ if word.starts_with('-') => return None,
            _ if word
                .split_once('=')
                .is_some_and(|(name, _)| !name.is_empty()) =>
            {
                continue;
            }
            _ => return Some(word),
        }
    }
    None
}

fn desktop_icon(path: &Path) -> Option<String> {
    for dir in application_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.path();
            if file.extension().is_none_or(|ext| ext != "desktop") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            if let (Some(exec), Some(icon)) = parse_desktop(&text)
                && exec_names(&exec, path)
            {
                return Some(icon);
            }
        }
    }
    None
}

fn resolve(icon: &str) -> Option<PathBuf> {
    let direct = Path::new(icon);
    if direct.is_absolute() {
        return direct.exists().then(|| direct.to_owned());
    }
    freedesktop_icons::lookup(icon)
        .with_size(RENDER as u16)
        .find()
        .or_else(|| freedesktop_icons::lookup(icon).find())
}

fn load(file: &Path) -> Result<RgbaImage, String> {
    let bytes = std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
    if file.extension().is_some_and(|ext| ext == "svg") {
        return icon_pack::render_svg(&bytes, RENDER);
    }
    image::load_from_memory(&bytes)
        .map(|image| image.to_rgba8())
        .map_err(|error| format!("{}: {error}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_entry_names_its_program_and_icon() {
        let text = "[Desktop Entry]\nName=Edit\nExec=env A=1 /usr/bin/gedit %U\nIcon=org.gnome.gedit\n[Desktop Action new]\nIcon=other\n";
        let (exec, icon) = parse_desktop(text);
        assert_eq!(icon.as_deref(), Some("org.gnome.gedit"));
        let exec = exec.unwrap();
        assert!(exec_names(&exec, Path::new("/usr/bin/gedit")));
        assert!(!exec_names(&exec, Path::new("/usr/bin/vim")));
        assert!(exec_names("gedit %U", Path::new("/usr/bin/gedit")));
    }

    #[test]
    fn desktop_exec_matches_wrapped_and_quoted_programs() {
        assert!(exec_names(
            "/usr/bin/env -i -u OLD A=1 gedit %U",
            Path::new("/usr/bin/gedit")
        ));
        assert!(exec_names(
            "env A=1 -- \"/opt/My Editor/bin/edit\" %F",
            Path::new("/opt/My Editor/bin/edit")
        ));
        assert!(!exec_names(
            "env A=1 /usr/bin/gedit %U",
            Path::new("/usr/bin/env")
        ));
        assert!(!exec_names(
            "env A=1 /usr/bin/gedit %U",
            Path::new("/usr/bin/vim")
        ));
        assert!(!exec_names(
            "env --bad-option /usr/bin/gedit",
            Path::new("/usr/bin/gedit")
        ));
        assert!(!exec_names(
            "\"/opt/My Editor/bin/edit %F",
            Path::new("/opt/My Editor/bin/edit")
        ));
    }

    #[test]
    fn an_svg_theme_icon_renders_into_an_icon_file() {
        let dir = std::env::temp_dir().join(format!("td-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let svg = dir.join("app.svg");
        std::fs::write(&svg, br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="red"/></svg>"#).unwrap();
        let image = load(&svg).unwrap();
        let file = icon_pack::pack_any(&image);
        assert!(icon_pack::is_icon(&file));
        assert_eq!(file[icon_pack::HEADER_BYTES + icon_pack::COLOR_BYTES], 255);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
