use std::collections::BTreeSet;
use std::time::Instant;

use serde_json::Value;
use td_core::{Body, Event, Frame, InputPhase, Item, Role, StreamDecoder};

fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../../../../schema/protocol/golden-frames-v2.json"
    ))
    .unwrap()
}

fn unhex(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn v2_messages_use_the_existing_frame_codec_byte_for_byte() {
    let fixtures = fixtures();
    assert_eq!(fixtures["protocol"], td_core::frame::VERSION);
    assert_eq!(fixtures["application_protocol"], 2);
    assert_eq!(fixtures["crc"], "CRC-16/CCITT-FALSE");
    for (key, expected) in [
        ("payload_bytes", td_core::frame::PAYLOAD_MAX),
        ("chunk_data_bytes", td_core::frame::CHUNK_DATA_MAX),
        ("decoded_body_bytes", td_core::frame::BODY_MAX),
        ("wire_bytes", td_core::frame::WIRE_MAX),
    ] {
        assert_eq!(fixtures["limits"][key], expected);
    }
    let mut names = BTreeSet::new();
    let mut messages = BTreeSet::new();
    for fixture in fixtures["frames"].as_array().unwrap() {
        let name = fixture["name"].as_str().unwrap();
        assert!(names.insert(name), "duplicate fixture {name}");
        assert_eq!(fixture["valid"], true);
        let decoded = &fixture["decoded"];
        assert_eq!(decoded["ver"], td_core::frame::VERSION);
        let seq = u8::try_from(decoded["seq"].as_u64().unwrap()).unwrap();
        let flags = decoded["flags"].as_u64().unwrap();
        let role = match flags {
            0 => Role::Request(seq),
            1 => Role::Response(seq),
            2 if seq == 0 => Role::Event,
            _ => panic!("invalid role in {name}"),
        };
        let payload = unhex(&decoded["payload_hex"]);
        assert_eq!(decoded["len"], payload.len(), "{name}");
        let body = match decoded["type"].as_u64().unwrap() {
            1 => {
                let message: Value = serde_json::from_slice(&payload).unwrap();
                assert_eq!(message, fixture["message"], "{name}");
                if flags == 0 || (flags == 1 && message["ok"] == true) {
                    messages.insert((message["m"].as_str().unwrap().to_owned(), flags));
                }
                Body::Message(payload)
            }
            2 => Body::Chunk {
                xfer: payload[0],
                idx: u16::from_le_bytes([payload[1], payload[2]]),
                data: payload[3..].to_vec(),
            },
            3 => {
                assert_eq!(payload.len(), 3);
                Body::Ack {
                    xfer: payload[0],
                    next: u16::from_le_bytes([payload[1], payload[2]]),
                }
            }
            _ => panic!("invalid frame type in {name}"),
        };
        let expected = Frame { role, body };
        let wire = unhex(&fixture["hex"]);
        assert_eq!(expected.encode().unwrap(), wire, "{name}");
        assert_eq!(Frame::decode_wire(&wire).unwrap(), expected, "{name}");
        assert!(wire.len() <= td_core::frame::WIRE_MAX);
    }
    for message in [
        "hello",
        "v2.name.get",
        "v2.name.set",
        "v2.asset.stat",
        "v2.asset.list",
        "v2.asset.put",
        "v2.asset.end",
        "v2.asset.delete",
        "v2.live.set",
        "v2.photo.put",
        "v2.photo.end",
        "v2.idle.get",
        "v2.idle.set",
        "v2.clock.set",
    ] {
        for flags in [0, 1] {
            assert!(messages.contains(&(message.to_owned(), flags)), "{message}");
        }
    }
}

#[test]
fn v2_bulk_traffic_keeps_tap_events_at_every_read_size() {
    let fixtures = fixtures();
    for stream in fixtures["streams"].as_array().unwrap() {
        let expected: Vec<_> = stream["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| {
                let fixture = fixtures["frames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|fixture| fixture["name"] == *name)
                    .unwrap();
                Frame::decode_wire(&unhex(&fixture["hex"])).unwrap()
            })
            .collect();
        for width in [1, 2, 7, 64, 4096] {
            let mut decoder = StreamDecoder::default();
            let mut actual = Vec::new();
            for chunk in unhex(&stream["hex"]).chunks(width) {
                for item in decoder.feed(chunk, Instant::now()) {
                    match item {
                        Item::Frame(frame) => actual.push(frame),
                        Item::Text(bytes) => assert!(bytes.is_empty()),
                    }
                }
            }
            assert_eq!(actual, expected, "{} at {width}", stream["name"]);
            assert_eq!(stream["errors"], decoder.errors());
        }
    }
}

fn frame<'a>(fixtures: &'a Value, name: &str) -> &'a Value {
    fixtures["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["name"] == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

#[test]
fn device_events_round_trip_through_the_event_type_byte_for_byte() {
    let fixtures = fixtures();
    let mut events = Vec::new();
    for fixture in fixtures["frames"].as_array().unwrap() {
        if fixture["decoded"]["flags"] != 2 {
            continue;
        }
        let payload = unhex(&fixture["decoded"]["payload_hex"]);
        let event: Event = serde_json::from_slice(&payload).unwrap();
        assert_eq!(serde_json::to_vec(&event).unwrap(), payload, "{}", fixture["name"]);
        events.push(event);
    }
    assert!(events.contains(&Event::Press {
        page: "media".into(),
        tile: "np".into(),
        kind: "widget".into(),
        part: Some("next".into()),
        seq: 42,
        t: 5123456,
    }));
    for (phase, value, seq, t) in [
        (InputPhase::Move, 0.4213, 118, 5234112),
        (InputPhase::End, 0.42, 119, 5234160),
    ] {
        assert!(events.contains(&Event::Input {
            page: "media".into(),
            tile: "vol".into(),
            stream: "value".into(),
            phase,
            value,
            seq,
            t,
        }));
    }
    let tap = frame(&fixtures, "tap-event");
    let tap: Event = serde_json::from_slice(&unhex(&tap["decoded"]["payload_hex"])).unwrap();
    assert!(matches!(tap, Event::Press { part: None, .. }));
}

#[test]
fn media_hellos_fit_one_payload_with_the_new_caps() {
    let fixtures = fixtures();
    for (name, gif) in [
        ("hello-media-max-response", false),
        ("hello-media-gif-max-response", true),
    ] {
        let payload = unhex(&frame(&fixtures, name)["decoded"]["payload_hex"]);
        assert!(payload.len() <= td_core::frame::PAYLOAD_MAX, "{name}");
        let hello: Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(hello["name"], "\"".repeat(48), "{name}");
        assert_eq!(hello["version"], "\"".repeat(32), "{name}");
        let caps = &hello["caps"];
        assert_eq!(caps["free_bytes"], u32::MAX, "{name}");
        assert_eq!(
            [&caps["text_bytes"], &caps["streams"], &caps["live_images"]],
            [128, 4, 4],
            "{name}"
        );
        assert_eq!(caps["image"], serde_json::json!([256, 256]), "{name}");
        let has = |field: &str, item: &str| {
            caps[field]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == item)
        };
        assert!(has("tile_kinds", "bar") && has("tile_kinds", "now-playing"), "{name}");
        assert!(has("formats", "jpeg"), "{name}");
        assert_eq!(caps.get("anim").is_some(), gif, "{name}");
    }
}

#[test]
fn live_text_keeps_its_escapes_and_multibyte_characters_within_text_bytes() {
    let fixtures = fixtures();
    let fixture = frame(&fixtures, "v2.live.set-text-multibyte-request");
    let text = String::from_utf8(unhex(&fixture["decoded"]["payload_hex"])).unwrap();
    let slash = '\\';
    for escape in [format!("{slash}\"Halo{slash}\""), format!(" {slash}{slash} ")] {
        assert!(text.contains(&escape), "{escape}");
    }
    let value = fixture["message"]["value"].as_str().unwrap();
    assert!(value.len() <= 128);
    for width in 2..=4 {
        assert!(value.chars().any(|c| c.len_utf8() == width), "{width}");
        assert!(text.chars().any(|c| c.len_utf8() == width), "{width}");
    }
    assert_eq!(
        frame(&fixtures, "v2.live.set-cover-error-asset")["message"]["reason"],
        "asset"
    );
    assert_eq!(
        frame(&fixtures, "v2.asset.put-jpeg-request")["message"]["format"],
        "jpeg"
    );
}
