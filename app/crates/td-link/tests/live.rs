use std::thread;
use std::time::Duration;

use serde_json::Value;
use td_core::{Layout, Orientation};
use td_link::live::{LiveSet, cost, layout_ref, set_live};
use td_link::mock::{MOCK_LIVE_HZ, MOCK_SERIES};
use td_link::{LiveError, LiveReason, LiveValue, MockDeck, Session, SessionConfig, xfer};

const TIMEOUT: Duration = Duration::from_secs(1);
const GRAPH: &[u8] = include_bytes!("../../../../schema/examples/graph.json");

fn open(device: &MockDeck) -> Session {
    let session = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap()
    .0;
    assert_eq!(session.identity(TIMEOUT).unwrap().protocol, 2);
    session
}

/// A v2 mock showing the graph example Layout, and the `layout` live requests name it by.
fn graph_deck() -> (MockDeck, Session, String) {
    let device = MockDeck::v2();
    let session = open(&device);
    let layout = Layout::parse(GRAPH).unwrap();
    xfer::put_layout(&session, Orientation::Landscape, &layout).unwrap();
    let reference = layout_ref(&layout.to_canonical_json());
    assert_eq!(device.live_layout().0, reference);
    (device, session, reference)
}

fn send(
    session: &Session,
    layout: &str,
    tile: &str,
    n: u32,
    value: &LiveValue,
) -> Result<u32, LiveError> {
    set_live(
        session,
        &LiveSet {
            layout,
            page: "page",
            tile,
            stream: "value",
            n,
            ttl_ms: 10_000,
            value,
        },
    )
}

fn refused(result: Result<u32, LiveError>) -> LiveReason {
    result.unwrap_err().reason().unwrap().clone()
}

#[test]
fn live_requests_match_the_golden_frames_byte_for_byte() {
    let all: Value = serde_json::from_str(include_str!(
        "../../../../schema/protocol/golden-frames-v2.json"
    ))
    .unwrap();
    let mut checked = 0;
    for fixture in all["frames"].as_array().unwrap() {
        let message = &fixture["message"];
        if message["m"] != "v2.live.set" || message.get("ok").is_some() {
            continue;
        }
        let raw = &message["value"];
        let value = match message["type"].as_str().unwrap() {
            "number" => LiveValue::Number(raw.as_f64().unwrap()),
            "series" => LiveValue::Series(
                raw.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect(),
            ),
            "text" => LiveValue::Text(raw.as_str().unwrap().into()),
            _ => LiveValue::Image(raw.as_str().map(Into::into)),
        };
        let set = LiveSet {
            layout: message["layout"].as_str().unwrap(),
            page: message["page"].as_str().unwrap(),
            tile: message["tile"].as_str().unwrap(),
            stream: message["stream"].as_str().unwrap(),
            n: message["n"].as_u64().unwrap() as u32,
            ttl_ms: message["ttl_ms"].as_u64().unwrap() as u32,
            value: &value,
        };
        let hex = fixture["decoded"]["payload_hex"].as_str().unwrap();
        let expected: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(set.to_payload(), expected, "{}", fixture["name"]);
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn reasons_round_trip_and_long_requests_cost_more() {
    for name in [
        "layout",
        "page",
        "tile",
        "stream",
        "type",
        "size",
        "asset",
        "sequence",
        "rate",
        "args",
        "memory",
        "not-negotiated",
        "unsupported",
        "new-reason",
    ] {
        assert_eq!(LiveReason::parse(name).name(), name);
    }
    assert_eq!(
        (cost(0), cost(1), cost(256), cost(257), cost(600)),
        (1, 1, 1, 2, 3)
    );
    assert!(LiveValue::Number(1e9).fits(32, 0));
    assert!(!LiveValue::Number(1e9 + 1.0).fits(32, 0));
    assert!(!LiveValue::Number(f64::NAN).fits(32, 0));
    assert!(!LiveValue::Series(Vec::new()).fits(32, 0));
    assert!(!LiveValue::Series(vec![0.0; 33]).fits(32, 0));
    assert!(!LiveValue::Text("a".into()).fits(32, 0));
}

#[test]
fn a_number_appends_a_sample_and_a_series_replaces_the_history() {
    let (device, session, layout) = graph_deck();
    for n in 1..=2 {
        assert_eq!(
            send(
                &session,
                &layout,
                "cpu",
                n,
                &LiveValue::Number(f64::from(n) * 10.5)
            ),
            Ok(n)
        );
    }
    let cpu = device.live("page", "cpu", "value").unwrap();
    assert_eq!(
        (cpu.n, cpu.samples, cpu.stale),
        (2, vec![10.5, 21.0], false)
    );
    let series = LiveValue::Series((0..40).map(f64::from).take(MOCK_SERIES).collect());
    assert_eq!(send(&session, &layout, "net", 7, &series), Ok(7));
    assert_eq!(
        device.live("page", "net", "value").unwrap().samples.len(),
        MOCK_SERIES
    );
    let too_long = LiveValue::Series(vec![1.0; MOCK_SERIES + 1]);
    assert_eq!(
        refused(send(&session, &layout, "net", 8, &too_long)),
        LiveReason::Size
    );
    assert_eq!(device.live_sets().len(), 3);
    assert!(
        device
            .live_sets()
            .iter()
            .all(|set| set["at_ms"].as_u64().is_some())
    );
}

#[test]
fn the_device_refuses_values_for_another_layout_tile_stream_or_type() {
    let (_device, session, layout) = graph_deck();
    let one = LiveValue::Number(1.0);
    assert_eq!(
        refused(send(&session, "builtin", "cpu", 1, &one)),
        LiveReason::Layout
    );
    assert_eq!(
        refused(send(&session, &layout, "gone", 1, &one)),
        LiveReason::Tile
    );
    let stream = set_live(
        &session,
        &LiveSet {
            layout: &layout,
            page: "page",
            tile: "cpu",
            stream: "title",
            n: 1,
            ttl_ms: 10_000,
            value: &one,
        },
    );
    assert_eq!(refused(stream), LiveReason::Stream);
    let page = set_live(
        &session,
        &LiveSet {
            layout: &layout,
            page: "other",
            tile: "cpu",
            stream: "value",
            n: 1,
            ttl_ms: 10_000,
            value: &one,
        },
    );
    assert_eq!(refused(page), LiveReason::Page);
    let series = LiveValue::Series(vec![1.0]);
    assert_eq!(
        refused(send(&session, &layout, "cpu", 1, &series)),
        LiveReason::Type
    );
    let huge = LiveValue::Number(2e9);
    assert_eq!(
        refused(send(&session, &layout, "cpu", 1, &huge)),
        LiveReason::Args
    );
    assert_eq!(send(&session, &layout, "cpu", 1, &one), Ok(1));
}

#[test]
fn sequence_numbers_rise_and_an_identical_retry_succeeds() {
    let (device, session, layout) = graph_deck();
    let five = LiveValue::Number(5.0);
    assert_eq!(send(&session, &layout, "cpu", 4, &five), Ok(4));
    assert_eq!(send(&session, &layout, "cpu", 4, &five), Ok(4));
    let six = LiveValue::Number(6.0);
    assert_eq!(
        refused(send(&session, &layout, "cpu", 4, &six)),
        LiveReason::Sequence
    );
    assert_eq!(
        refused(send(&session, &layout, "cpu", 3, &six)),
        LiveReason::Sequence
    );
    assert_eq!(device.live("page", "cpu", "value").unwrap().samples, [5.0]);
    assert_eq!(device.live_sets().len(), 1);

    session.identity(TIMEOUT).unwrap();
    assert!(device.live("page", "cpu", "value").is_none());
    assert_eq!(send(&session, &layout, "cpu", 1, &six), Ok(1));
}

#[test]
fn a_third_update_in_one_window_is_refused_with_a_retry() {
    let (device, session, layout) = graph_deck();
    session.identity(TIMEOUT).unwrap();
    for n in 1..=MOCK_LIVE_HZ {
        assert_eq!(
            send(&session, &layout, "cpu", n, &LiveValue::Number(1.0)),
            Ok(n)
        );
    }
    let over = send(&session, &layout, "cpu", 9, &LiveValue::Number(2.0)).unwrap_err();
    let LiveError::Refused {
        reason: LiveReason::Rate,
        retry_ms: Some(retry),
        ..
    } = over
    else {
        panic!("expected rate, got {over:?}");
    };
    assert!((1..=1000).contains(&retry));
    assert_eq!(device.live("page", "cpu", "value").unwrap().n, MOCK_LIVE_HZ);
    assert_eq!(
        send(&session, &layout, "net", 1, &LiveValue::Series(vec![1.0])),
        Ok(1)
    );
    thread::sleep(Duration::from_millis(u64::from(retry)));
    assert_eq!(
        send(&session, &layout, "cpu", 9, &LiveValue::Number(2.0)),
        Ok(9)
    );
}

#[test]
fn a_value_past_its_ttl_is_stale_until_a_fresh_one_arrives() {
    let (device, session, layout) = graph_deck();
    let short = |n, v| LiveSet {
        layout: &layout,
        page: "page",
        tile: "cpu",
        stream: "value",
        n,
        ttl_ms: 1_000,
        value: v,
    };
    assert_eq!(
        set_live(&session, &short(1, &LiveValue::Number(3.0))),
        Ok(1)
    );
    assert!(!device.live("page", "cpu", "value").unwrap().stale);
    thread::sleep(Duration::from_millis(1_100));
    let stale = device.live("page", "cpu", "value").unwrap();
    assert!(stale.stale);
    assert_eq!(stale.samples, [3.0]);
    assert_eq!(
        set_live(&session, &short(2, &LiveValue::Number(4.0))),
        Ok(2)
    );
    let fresh = device.live("page", "cpu", "value").unwrap();
    assert_eq!((fresh.stale, fresh.samples), (false, vec![3.0, 4.0]));
}

#[test]
fn pushing_a_layout_clears_its_live_values() {
    let (device, session, layout) = graph_deck();
    assert_eq!(
        send(&session, &layout, "cpu", 1, &LiveValue::Number(1.0)),
        Ok(1)
    );
    let mut renamed = Layout::parse(GRAPH).unwrap();
    renamed.name = "Graph 2".into();
    xfer::put_layout(&session, Orientation::Landscape, &renamed).unwrap();
    assert!(device.live("page", "cpu", "value").is_none());
    assert_eq!(
        refused(send(&session, &layout, "cpu", 2, &LiveValue::Number(1.0))),
        LiveReason::Layout
    );
    let next = layout_ref(&renamed.to_canonical_json());
    assert_eq!(
        send(&session, &next, "cpu", 1, &LiveValue::Number(1.0)),
        Ok(1)
    );
}
