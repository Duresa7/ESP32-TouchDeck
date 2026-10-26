use std::path::Path;
use std::time::Instant;

use serde::Deserialize;
use td_core::frame::{BODY_MAX, CHUNK_DATA_MAX, PAYLOAD_MAX, WIRE_MAX};
use td_core::{Body, Frame, Item, Role, StreamDecoder};

#[derive(Deserialize)]
struct Golden {
    protocol: u32,
    crc: String,
    limits: Limits,
    frames: Vec<Fixture>,
    streams: Vec<StreamFixture>,
}

#[derive(Deserialize)]
struct StreamFixture {
    name: String,
    hex: String,
    frames: Vec<String>,
    errors: u64,
}

#[derive(Deserialize)]
struct Limits {
    payload_bytes: usize,
    chunk_data_bytes: usize,
    decoded_body_bytes: usize,
    wire_bytes: usize,
}

#[derive(Deserialize)]
struct Fixture {
    name: String,
    valid: bool,
    hex: String,
    decoded: Option<Decoded>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Decoded {
    ver: u8,
    #[serde(rename = "type")]
    kind: u8,
    seq: u8,
    flags: u8,
    len: usize,
    payload_hex: String,
}

fn golden() -> Golden {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../schema/protocol/golden-frames.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn fields(frame: &Frame) -> (u8, u8, u8, usize, Vec<u8>) {
    let payload = frame.body.payload();
    (
        frame.body.kind(),
        frame.role.seq(),
        frame.role.flags(),
        payload.len(),
        payload,
    )
}

fn split(decoder: &mut StreamDecoder, stream: &[u8], width: usize) -> (Vec<u8>, Vec<Frame>) {
    let (mut text, mut frames) = (Vec::new(), Vec::new());
    for part in stream.chunks(width) {
        for item in decoder.feed(part, Instant::now()) {
            match item {
                Item::Text(bytes) => text.extend(bytes),
                Item::Frame(frame) => frames.push(frame),
            }
        }
    }
    (text, frames)
}

#[test]
fn limits_and_crc_match_the_fixture_file() {
    let golden = golden();
    assert_eq!(golden.protocol, td_core::PROTOCOL);
    assert_eq!(golden.crc, "CRC-16/CCITT-FALSE");
    assert_eq!(golden.limits.payload_bytes, PAYLOAD_MAX);
    assert_eq!(golden.limits.chunk_data_bytes, CHUNK_DATA_MAX);
    assert_eq!(golden.limits.decoded_body_bytes, BODY_MAX);
    assert_eq!(golden.limits.wire_bytes, WIRE_MAX);
    assert_eq!(td_core::frame::crc16(b"123456789"), 0x29B1);
}

#[test]
fn every_valid_frame_decodes_and_encodes_byte_for_byte() {
    let golden = golden();
    let (mut valid, mut longest) = (0, 0);
    for fixture in golden.frames.iter().filter(|f| f.valid) {
        let wire = unhex(&fixture.hex);
        let expected = fixture.decoded.as_ref().unwrap();
        let frame = Frame::decode_wire(&wire).unwrap_or_else(|e| panic!("{}: {e}", fixture.name));
        assert_eq!(expected.ver, td_core::frame::VERSION, "{}", fixture.name);
        assert_eq!(
            fields(&frame),
            (
                expected.kind,
                expected.seq,
                expected.flags,
                expected.len,
                unhex(&expected.payload_hex)
            ),
            "{}",
            fixture.name
        );
        assert_eq!(frame.encode().unwrap(), wire, "{}", fixture.name);
        assert!(wire.len() <= WIRE_MAX, "{}", fixture.name);
        longest = longest.max(wire.len());
        valid += 1;
    }
    assert!(valid >= 12);
    assert_eq!(longest, 2061, "the largest escaped frame in link-v1.md");
}

#[test]
fn every_invalid_frame_fails_with_its_error() {
    let golden = golden();
    let invalid: Vec<_> = golden.frames.iter().filter(|f| !f.valid).collect();
    assert!(invalid.len() >= 3);
    for fixture in invalid {
        let error = Frame::decode_wire(&unhex(&fixture.hex)).expect_err(&fixture.name);
        assert_eq!(
            Some(error.as_str()),
            fixture.error.as_deref(),
            "{}",
            fixture.name
        );
    }
}

#[test]
fn stream_splits_frames_from_text_at_any_read_size() {
    let golden = golden();
    for fixture in golden.frames.iter().filter(|f| f.valid) {
        let wire = unhex(&fixture.hex);
        let expected = Frame::decode_wire(&wire).unwrap();
        let mut stream = b"version\r\n".to_vec();
        stream.extend(&wire);
        stream.extend(&wire);
        stream.extend(b"a\xdb\xdd\x00\n");
        for width in [1, 2, 7, 64, 4096] {
            let mut decoder = StreamDecoder::default();
            let (text, frames) = split(&mut decoder, &stream, width);
            assert_eq!(
                text, b"version\r\na\xdb\xdd\x00\n",
                "{} at {width}",
                fixture.name
            );
            assert_eq!(
                frames,
                [expected.clone(), expected.clone()],
                "{} at {width}",
                fixture.name
            );
            assert_eq!(decoder.errors(), 0, "{} at {width}", fixture.name);
        }
    }
}

#[test]
fn an_invalid_frame_never_hides_the_next_frame_or_text() {
    let golden = golden();
    let hello = unhex(&golden.frames.iter().find(|f| f.valid).unwrap().hex);
    let hello_frame = Frame::decode_wire(&hello).unwrap();
    for fixture in golden.frames.iter().filter(|f| !f.valid) {
        let wire = unhex(&fixture.hex);
        let mut stream = wire.clone();
        stream.extend(&hello);
        stream.extend(b"version\n");
        for width in [1, 7, 4096] {
            let mut decoder = StreamDecoder::default();
            let (text, frames) = split(&mut decoder, &stream, width);
            assert!(text.ends_with(b"version\n"), "{} at {width}", fixture.name);
            assert_eq!(
                frames,
                std::slice::from_ref(&hello_frame),
                "{} at {width}",
                fixture.name
            );
            if fixture.error.as_deref() == Some("version") {
                assert_eq!(
                    decoder.errors(),
                    0,
                    "{}: a delimiter without ver 1 opens nothing",
                    fixture.name
                );
                assert!(
                    text.starts_with(&wire[1..wire.len() - 1]),
                    "{}",
                    fixture.name
                );
            } else {
                assert_eq!(decoder.errors(), 1, "{} at {width}", fixture.name);
            }
        }
    }
}

#[test]
fn a_frame_cut_short_by_the_host_resynchronises() {
    let golden = golden();
    let hello = unhex(&golden.frames.iter().find(|f| f.valid).unwrap().hex);
    let mut stream = b"x".to_vec();
    stream.extend(&hello[..6]);
    stream.extend(b"I (1) touchdeck: alive 0\n");
    stream.extend(&hello);
    stream.extend(b"done\n");
    let mut decoder = StreamDecoder::default();
    let (text, frames) = split(&mut decoder, &stream, 3);
    let text = String::from_utf8_lossy(&text);
    assert!(
        text.contains("I (1) touchdeck: alive 0\n") && text.ends_with("done\n"),
        "{text}"
    );
    assert_eq!(frames, [Frame::decode_wire(&hello).unwrap()]);
    assert_eq!(decoder.errors(), 1);
}

#[test]
fn every_stream_fixture_yields_its_frames_and_errors_at_any_read_size() {
    let golden = golden();
    assert!(!golden.streams.is_empty());
    for fixture in &golden.streams {
        let expected: Vec<Frame> = fixture
            .frames
            .iter()
            .map(|name| {
                let frame = golden.frames.iter().find(|f| &f.name == name).unwrap();
                Frame::decode_wire(&unhex(&frame.hex)).unwrap()
            })
            .collect();
        for width in [1, 3, 7, 64, 4096] {
            let mut decoder = StreamDecoder::default();
            let (_, frames) = split(&mut decoder, &unhex(&fixture.hex), width);
            assert_eq!(frames, expected, "{} at {width}", fixture.name);
            assert_eq!(
                decoder.errors(),
                fixture.errors,
                "{} at {width}",
                fixture.name
            );
        }
    }
}

#[test]
fn maximum_chunk_and_escape_heavy_frame_round_trip() {
    let chunk = Frame {
        role: Role::Request(219),
        body: Body::Chunk {
            xfer: 7,
            idx: 1,
            data: (0..CHUNK_DATA_MAX).map(|i| i as u8).collect(),
        },
    };
    let wire = chunk.encode().unwrap();
    assert_eq!(Frame::decode_wire(&wire).unwrap(), chunk);
    let heavy = Frame::message(Role::Request(255), [0xC0, 0xDB].repeat(PAYLOAD_MAX / 2));
    let wire = heavy.encode().unwrap();
    assert!(wire.len() <= WIRE_MAX);
    let mut decoder = StreamDecoder::default();
    let (_, frames) = split(&mut decoder, &wire, 1);
    assert_eq!(frames, [heavy]);
}
