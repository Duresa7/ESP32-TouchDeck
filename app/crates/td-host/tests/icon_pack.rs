use image::{Rgba, RgbaImage};
use sha2::{Digest, Sha256};
use td_host::icon_pack::{self, ALPHA_BYTES, COLOR_BYTES, FILE_BYTES, HEADER_BYTES, SIDE};
use td_host::icons::{self, IconCache};

const GOLDEN: &[u8] = include_bytes!("fixtures/icon-64.tdi");

/// The RGBA image `schema/icon-format.md` defines for the golden file.
fn golden_rgba() -> RgbaImage {
    RgbaImage::from_fn(SIDE, SIDE, |x, y| {
        let inner = (16..48).contains(&x) && (16..48).contains(&y);
        let a = if inner {
            255
        } else if (x + y) % 2 == 0 {
            160
        } else {
            0
        };
        Rgba([
            (x * 255 / 63) as u8,
            (y * 255 / 63) as u8,
            ((x ^ y) * 255 / 63) as u8,
            a,
        ])
    })
}

fn alpha(file: &[u8], x: u32, y: u32) -> u8 {
    file[HEADER_BYTES + COLOR_BYTES + (y * SIDE + x) as usize]
}

#[test]
fn the_fixture_rgba_packs_to_the_golden_file_byte_for_byte() {
    let file = icon_pack::pack(&golden_rgba());
    assert_eq!(file.len(), FILE_BYTES);
    assert_eq!(file, GOLDEN);
    assert!(icon_pack::is_icon(&file));
}

#[test]
fn the_id_is_the_file_hash_prefix() {
    let id = icon_pack::icon_id(GOLDEN);
    let hash = Sha256::digest(GOLDEN);
    assert_eq!(id, "be1044446a68262d");
    assert_eq!(id.len(), 16);
    assert!(hash.starts_with(&[0xbe, 0x10, 0x44, 0x44, 0x6a, 0x68, 0x26, 0x2d]));
}

#[test]
fn a_256_px_source_downscales_to_64() {
    let source = RgbaImage::from_pixel(256, 256, Rgba([255, 0, 0, 255]));
    let fitted = icon_pack::fit(&source);
    assert_eq!(fitted.dimensions(), (SIDE, SIDE));
    let file = icon_pack::pack(&fitted);
    assert!(file[HEADER_BYTES + COLOR_BYTES..].iter().all(|a| *a == 255));
    assert_eq!(
        &file[HEADER_BYTES..HEADER_BYTES + 2],
        &0xF800u16.to_le_bytes()
    );
}

#[test]
fn a_non_square_source_is_letterboxed_on_transparent() {
    let source = RgbaImage::from_pixel(256, 128, Rgba([0, 255, 0, 255]));
    let file = icon_pack::pack_any(&source);
    // 256 by 128 fits as 64 by 32, centred on rows 16 to 47.
    assert_eq!(alpha(&file, 32, 0), 0);
    assert_eq!(alpha(&file, 32, 15), 0);
    assert_eq!(alpha(&file, 32, 16), 255);
    assert_eq!(alpha(&file, 32, 47), 255);
    assert_eq!(alpha(&file, 32, 48), 0);
    assert_eq!(alpha(&file, 32, 63), 0);
    assert_eq!(
        file[HEADER_BYTES + COLOR_BYTES..]
            .iter()
            .filter(|a| **a == 255)
            .count(),
        ALPHA_BYTES / 2
    );
}

#[test]
fn every_builtin_svg_renders_and_the_index_names_every_file() {
    let index = icons::builtin_index().unwrap();
    let mut named: Vec<&str> = index.iter().map(|entry| entry.file.as_str()).collect();
    named.sort_unstable();
    let mut files = icons::builtin_svg_names();
    files.sort_unstable();
    assert_eq!(named, files);
    assert!((35..=50).contains(&index.len()), "about 40 icons");
    let set = icons::builtin();
    assert_eq!(set.len(), index.len());
    for icon in set {
        assert!(icon_pack::is_icon(&icon.file), "{}", icon.name);
        assert_eq!(icon.id, icon_pack::icon_id(&icon.file));
        let opaque = icon.file[HEADER_BYTES + COLOR_BYTES..]
            .iter()
            .filter(|a| **a > 128)
            .count();
        assert!(opaque > 50, "{} draws {opaque} opaque pixels", icon.name);
    }
    for entry in &index {
        assert!(
            !entry.source.is_empty() && entry.license == "ISC",
            "{}",
            entry.name
        );
    }
    let mut ids: Vec<&str> = set.iter().map(|icon| icon.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), set.len(), "every built-in icon is distinct");
}

#[test]
fn the_cache_stores_by_id_and_ignores_a_file_that_is_not_its_hash() {
    let dir = std::env::temp_dir().join(format!("td-icon-cache-{}", std::process::id()));
    let cache = IconCache::new(dir.clone());
    let id = cache.put(GOLDEN).unwrap();
    assert_eq!(cache.get(&id).as_deref(), Some(GOLDEN));
    std::fs::write(dir.join("0000000000000000.bin"), GOLDEN).unwrap();
    assert_eq!(cache.get("0000000000000000"), None);
    assert!(cache.put(b"not an icon").is_err());
    let terminal = icons::builtin()
        .iter()
        .find(|i| i.name == "terminal")
        .unwrap();
    assert_eq!(cache.get(&terminal.id), Some(terminal.file.clone()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn actions_suggest_their_default_glyph() {
    let suggest = |value: serde_json::Value| icons::default_for(&value);
    assert_eq!(
        suggest(serde_json::json!({"type": "open", "target": "https://example.com"})),
        Some("browser")
    );
    assert_eq!(
        suggest(serde_json::json!({"type": "open", "target": "C:\\Users"})),
        Some("folder")
    );
    assert_eq!(
        suggest(serde_json::json!({"type": "command", "program": "x"})),
        Some("terminal")
    );
    assert_eq!(
        suggest(serde_json::json!({"type": "widget", "source": "claude-status"})),
        Some("bot")
    );
    assert_eq!(suggest(serde_json::json!({"type": "none"})), None);
    for name in ["browser", "folder", "terminal", "bot", "server", "gauge"] {
        assert!(
            icons::builtin().iter().any(|icon| icon.name == name),
            "{name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn notepad_gives_an_icon_with_at_least_500_opaque_pixels() {
    let dir = std::env::temp_dir().join(format!("td-app-icon-{}", std::process::id()));
    let cache = IconCache::new(dir.clone());
    let id = cache
        .take_app_icon(std::path::Path::new(r"C:\Windows\notepad.exe"))
        .unwrap();
    let file = cache.get(&id).unwrap();
    let opaque = file[HEADER_BYTES + COLOR_BYTES..]
        .iter()
        .filter(|a| **a == 255)
        .count();
    assert!(opaque >= 500, "{opaque} opaque pixels");
    std::fs::remove_dir_all(dir).unwrap();
}
