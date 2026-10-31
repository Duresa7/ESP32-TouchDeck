use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::{Value, json};
use td_core::{InputPhase, Layout, Orientation};
use td_link::asset::{AssetFormat, AssetReason, asset_id, delete_asset, list_assets, put_asset};
use td_link::live::{LiveSet, layout_ref, set_live};
use td_link::{Incoming, LiveError, LiveReason, LiveValue, MockDeck, Session, SessionConfig, xfer};

const TIMEOUT: Duration = Duration::from_secs(1);
const MEDIA: &[u8] = include_bytes!("../../../../schema/examples/media.json");

fn open(device: &MockDeck) -> (Session, Receiver<Incoming>) {
    let (session, events) = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap();
    assert_eq!(session.identity(TIMEOUT).unwrap().protocol, 2);
    (session, events)
}

/// A v2 mock with the volume and media caps, showing the media example Layout.
fn media_deck() -> (MockDeck, Session, Receiver<Incoming>, String) {
    let device = MockDeck::v2();
    device.set_media_capabilities(true);
    let (session, events) = open(&device);
    let layout = Layout::parse(MEDIA).unwrap();
    xfer::put_layout(&session, Orientation::Landscape, &layout).unwrap();
    let reference = layout_ref(&layout.to_canonical_json());
    (device, session, events, reference)
}

/// A JPEG with only the segments the Device's structure check reads: SOI, a frame header, a scan, EOI.
fn jpeg(width: u16, height: u16, frame: u8, pad: usize) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    bytes.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
    bytes.extend_from_slice(&[0xFF, frame, 0x00, 0x11, 0x08]);
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    bytes.extend_from_slice(&[
        0xFF, 0xDA, 0x00, 0x0C, 3, 1, 0, 2, 0x11, 3, 0x11, 0, 0x3F, 0,
    ]);
    bytes.extend(std::iter::repeat_n(0x5A, pad));
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

fn set(
    session: &Session,
    layout: &str,
    tile: &str,
    stream: &str,
    n: u32,
    value: &LiveValue,
) -> Result<u32, LiveError> {
    set_live(
        session,
        &LiveSet {
            layout,
            page: "media",
            tile,
            stream,
            n,
            ttl_ms: 30_000,
            value,
        },
    )
}

fn refused(result: Result<u32, LiveError>) -> LiveReason {
    result.unwrap_err().reason().unwrap().clone()
}

fn next_event(events: &Receiver<Incoming>) -> Value {
    loop {
        match events.recv_timeout(TIMEOUT).unwrap() {
            Incoming::Event(event) => return event,
            _ => continue,
        }
    }
}

#[test]
fn text_values_are_kept_up_to_text_bytes_and_controls_are_refused() {
    let (device, session, _events, layout) = media_deck();
    let title = LiveValue::Text("Beyoncé – \"Halo\" 東京 🎵".into());
    assert_eq!(set(&session, &layout, "playing", "title", 1, &title), Ok(1));
    let shown = device.live("media", "playing", "title").unwrap();
    assert_eq!(shown.value, json!("Beyoncé – \"Halo\" 東京 🎵"));
    assert!(shown.samples.is_empty());

    let at_limit = LiveValue::Text("é".repeat(64));
    assert_eq!(
        set(&session, &layout, "playing", "artist", 1, &at_limit),
        Ok(1)
    );
    let over = LiveValue::Text(format!("{}a", "é".repeat(64)));
    assert_eq!(
        refused(set(&session, &layout, "playing", "artist", 2, &over)),
        LiveReason::Size
    );
    let control = LiveValue::Text("Tab\there".into());
    assert_eq!(
        refused(set(&session, &layout, "playing", "artist", 2, &control)),
        LiveReason::Args
    );
    assert_eq!(
        set(
            &session,
            &layout,
            "playing",
            "artist",
            2,
            &LiveValue::Text(String::new())
        ),
        Ok(2)
    );
    assert_eq!(
        refused(set(&session, &layout, "playing", "state", 1, &title)),
        LiveReason::Type
    );
}

#[test]
fn without_media_caps_text_is_a_type_the_device_cannot_draw() {
    let device = MockDeck::v2();
    let (session, _events) = open(&device);
    let caps = session.identity(TIMEOUT).unwrap().capabilities.unwrap();
    assert_eq!(
        (caps.text_bytes, caps.streams, caps.live_images),
        (0, 1, None)
    );
    let layout = Layout::parse(MEDIA).unwrap();
    assert!(xfer::put_layout(&session, Orientation::Landscape, &layout).is_err());
}

#[test]
fn a_cover_is_a_jpeg_asset_the_device_already_holds() {
    let (device, session, _events, layout) = media_deck();
    let cover = jpeg(174, 174, 0xC0, 4000);
    let put = put_asset(&session, &cover, AssetFormat::Jpeg).unwrap();
    assert!(put.sent);
    let listed = list_assets(&session).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].format, AssetFormat::Jpeg);

    let image = LiveValue::Image(Some(format!("sha256:{}", put.id)));
    assert_eq!(set(&session, &layout, "playing", "cover", 1, &image), Ok(1));
    assert_eq!(
        device.live("media", "playing", "cover").unwrap().value,
        json!(format!("sha256:{}", put.id))
    );
    let missing = LiveValue::Image(Some(format!("sha256:{}", "0".repeat(64))));
    assert_eq!(
        refused(set(&session, &layout, "playing", "cover", 2, &missing)),
        LiveReason::Asset
    );
    let icon = td_link::xfer::ICON_BYTES;
    let mut tdi = b"TDI1".to_vec();
    tdi.resize(icon, 7);
    let tdi = put_asset(&session, &tdi, AssetFormat::Tdi1).unwrap();
    let static_icon = LiveValue::Image(Some(format!("sha256:{}", tdi.id)));
    assert_eq!(
        refused(set(&session, &layout, "playing", "cover", 2, &static_icon)),
        LiveReason::Asset
    );
    let malformed = LiveValue::Image(Some(put.id.clone()));
    assert_eq!(
        refused(set(&session, &layout, "playing", "cover", 2, &malformed)),
        LiveReason::Args
    );

    let error = delete_asset(&session, &put.id).unwrap_err();
    assert_eq!(error.reason(), Some(&AssetReason::InUse));
    assert_eq!(
        set(
            &session,
            &layout,
            "playing",
            "cover",
            2,
            &LiveValue::Image(None)
        ),
        Ok(2)
    );
    assert!(delete_asset(&session, &put.id).unwrap());
}

#[test]
fn jpeg_assets_are_baseline_within_the_image_cap() {
    let (device, session, _events, _layout) = media_deck();
    let caps = session.identity(TIMEOUT).unwrap().capabilities.unwrap();
    assert_eq!(caps.image, [256, 256]);
    assert_eq!(caps.asset_bytes, 32768);
    let progressive = put_asset(&session, &jpeg(64, 64, 0xC2, 10), AssetFormat::Jpeg).unwrap_err();
    assert_eq!(progressive.reason(), Some(&AssetReason::Format));
    let wide = put_asset(&session, &jpeg(257, 64, 0xC0, 10), AssetFormat::Jpeg).unwrap_err();
    assert_eq!(wide.reason(), Some(&AssetReason::Size));
    let large = put_asset(&session, &jpeg(64, 64, 0xC0, 32768), AssetFormat::Jpeg).unwrap_err();
    assert_eq!(large.reason(), Some(&AssetReason::Size));
    let mut truncated = jpeg(64, 64, 0xC0, 10);
    truncated.truncate(truncated.len() - 2);
    let truncated = put_asset(&session, &truncated, AssetFormat::Jpeg).unwrap_err();
    assert_eq!(truncated.reason(), Some(&AssetReason::Format));
    let bytes = jpeg(256, 256, 0xC0, 32768 - jpeg(256, 256, 0xC0, 0).len());
    assert_eq!(bytes.len(), 32768);
    let fits = put_asset(&session, &bytes, AssetFormat::Jpeg).unwrap();
    assert_eq!(fits.id, asset_id(&bytes));
    assert_eq!(device.assets().keys().collect::<Vec<_>>(), [&fits.id]);
}

#[test]
fn a_layout_over_the_live_image_cap_is_refused_whole() {
    let (device, session, _events, layout) = media_deck();
    let mut over: Value = serde_json::from_slice(MEDIA).unwrap();
    let page = &mut over["pages"][0];
    page["rows"] = json!(8);
    page["cols"] = json!(8);
    let playing = page["tiles"][2].clone();
    let tiles = page["tiles"].as_array_mut().unwrap();
    for i in 0..4 {
        let mut copy = playing.clone();
        copy["id"] = json!(format!("np{i}"));
        copy["row"] = json!(2 + i);
        copy["col"] = json!(0);
        copy["colSpan"] = json!(1);
        copy["rowSpan"] = json!(1);
        tiles.push(copy);
    }
    let five = Layout::parse(&serde_json::to_vec(&over).unwrap()).unwrap();
    assert!(xfer::put_layout(&session, Orientation::Landscape, &five).is_err());
    assert_eq!(device.live_layout().0, layout, "the Layout on screen stays");
    over["pages"][0]["tiles"].as_array_mut().unwrap().pop();
    let four = Layout::parse(&serde_json::to_vec(&over).unwrap()).unwrap();
    xfer::put_layout(&session, Orientation::Landscape, &four).unwrap();
}

#[test]
fn drags_send_rounded_input_snapshots_only_on_a_v2_session() {
    let (device, session, events, _layout) = media_deck();
    assert_eq!(
        device.emit_input("media", "volume", "value", InputPhase::Move, 0.421_349),
        Some(1)
    );
    assert_eq!(
        device.emit_input("media", "volume", "value", InputPhase::End, 0.42),
        Some(2)
    );
    let moved = next_event(&events);
    assert_eq!(
        (
            &moved["m"],
            &moved["page"],
            &moved["tile"],
            &moved["stream"],
            &moved["phase"],
            &moved["value"],
            &moved["seq"]
        ),
        (
            &json!("v2.input"),
            &json!("media"),
            &json!("volume"),
            &json!("value"),
            &json!("move"),
            &json!(0.4213),
            &json!(1)
        )
    );
    assert!(moved["t"].is_u64());
    assert_eq!(next_event(&events)["phase"], "end");
    drop(session);

    let (v1, _) = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap();
    assert!(v1.hello(TIMEOUT).unwrap().is_deck());
    assert_eq!(
        device.emit_input("media", "volume", "value", InputPhase::Move, 0.5),
        None
    );
}

#[test]
fn a_tap_in_a_part_names_it_only_on_a_v2_session() {
    let (device, session, events, _layout) = media_deck();
    let seq = device.emit_press_part("media", "playing", "widget", "next");
    let press = next_event(&events);
    assert_eq!(
        press,
        json!({"m": "press", "page": "media", "tile": "playing", "kind": "widget", "part": "next", "seq": seq, "t": press["t"]})
    );
    drop(session);

    let (v1, v1_events) = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap();
    assert!(v1.hello(TIMEOUT).unwrap().is_deck());
    let next = device.emit_press_part("media", "playing", "widget", "next");
    assert_eq!(next, seq + 1);
    let press = next_event(&v1_events);
    assert_eq!(press["seq"], next);
    assert!(press.get("part").is_none());
}
