//! Embeds the built-in icon set from `app/icons/builtin` as `(file, bytes)` pairs.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../icons/builtin");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("app/icons/builtin exists")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg") || name == "index.json")
        .collect();
    names.sort();
    let mut out = String::from("&[\n");
    for name in names {
        let path = dir.join(&name);
        writeln!(out, "    ({name:?}, include_bytes!({:?})),", path.display()).unwrap();
    }
    out.push(']');
    let target = Path::new(&std::env::var("OUT_DIR").unwrap()).join("builtin_icons.rs");
    std::fs::write(target, out).unwrap();
}
