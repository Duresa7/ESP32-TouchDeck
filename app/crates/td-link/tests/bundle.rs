use std::io::{Cursor, Read, Write};
use td_link::bundle::Bundle;
const FIXTURE: &[u8] = include_bytes!("fixtures/touchdeck-test.tdfw");
fn changed(name: &str, replacement: Option<&[u8]>) -> Vec<u8> {
    let mut input = zip::ZipArchive::new(Cursor::new(FIXTURE)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for idx in 0..input.len() {
        let mut file = input.by_index(idx).unwrap();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let filename = file.name().unwrap();
        let bytes = if filename.as_ref() == name {
            match replacement {
                Some(bytes) => bytes,
                None => continue,
            }
        } else {
            &data
        };
        out.start_file(filename, zip::write::SimpleFileOptions::default())
            .unwrap();
        out.write_all(bytes).unwrap();
    }
    out.finish().unwrap().into_inner()
}
#[test]
fn fixture_and_manifest_offsets() {
    let b = Bundle::parse(FIXTURE).unwrap();
    assert_eq!(b.manifest.version, "test-only");
    assert_eq!(
        b.images.iter().map(|i| i.offset).collect::<Vec<_>>(),
        [0, 0x8000, 0x40000]
    );
}
#[test]
fn missing_bootloader_is_refused() {
    assert!(Bundle::parse(&changed("bootloader.bin", None)).is_err());
}
#[test]
fn corrupt_sha_is_refused() {
    assert!(
        Bundle::parse(&changed("touchdeck.bin", Some(b"corrupt")))
            .unwrap_err()
            .contains("sha256")
    );
}
#[test]
fn unsafe_offsets_are_refused() {
    let b = Bundle::parse(FIXTURE).unwrap();
    let mut z = zip::ZipArchive::new(Cursor::new(FIXTURE)).unwrap();
    let mut text = String::new();
    z.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let text = text.replace("0x40000", "0x840000");
    assert_eq!(b.images[2].offset, 0x40000);
    assert!(
        Bundle::parse(&changed("manifest.json", Some(text.as_bytes())))
            .unwrap_err()
            .contains("unsafe offset")
    );
}
