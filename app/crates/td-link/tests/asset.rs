use serde_json::Value;
use std::time::Duration;
use td_core::{Frame, Reply, Role};
use td_link::asset::{
    AssetCommitted, AssetPage, AssetRequest, AssetSpace, PutOpened, asset_id, asset_space,
    delete_asset, list_asset_page, list_assets, put_asset, reply_fields,
};
use td_link::{AssetError, AssetFormat, AssetReason, MockDeck, Session, SessionConfig};

const TIMEOUT: Duration = Duration::from_secs(1);
const ICON_BYTES: usize = 12300;

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

fn icon(seed: u8) -> Vec<u8> {
    let mut file = b"TDI1".to_vec();
    file.extend_from_slice(&[64, 0, 64, 0, 1, 0, 0, 0]);
    file.extend((12..ICON_BYTES).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)));
    file
}

fn fixtures() -> Vec<Value> {
    let all: Value = serde_json::from_str(include_str!(
        "../../../../schema/protocol/golden-frames-v2.json"
    ))
    .unwrap();
    all["frames"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["name"].as_str().unwrap().starts_with("v2.asset."))
        .cloned()
        .collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn asset_requests_match_the_golden_frames_byte_for_byte() {
    let mut checked = 0;
    for fixture in fixtures() {
        let decoded = &fixture["decoded"];
        if decoded["flags"] != 0 {
            continue;
        }
        let message = &fixture["message"];
        let format = message["format"].as_str().and_then(AssetFormat::parse);
        let request = match message["m"].as_str().unwrap() {
            "v2.asset.stat" => AssetRequest::Stat,
            "v2.asset.list" => AssetRequest::List {
                after: message["after"].as_str(),
            },
            "v2.asset.put" => AssetRequest::Put {
                id: message["id"].as_str().unwrap(),
                bytes: message["bytes"].as_u64().unwrap() as usize,
                format: format.unwrap(),
            },
            "v2.asset.end" => AssetRequest::End {
                xfer: message["xfer"].as_u64().unwrap() as u8,
            },
            "v2.asset.delete" => AssetRequest::Delete {
                id: message["id"].as_str().unwrap(),
            },
            other => panic!("unexpected request {other}"),
        };
        let payload = request.to_payload();
        let name = fixture["name"].as_str().unwrap();
        assert_eq!(
            payload,
            unhex(decoded["payload_hex"].as_str().unwrap()),
            "{name}"
        );
        let seq = decoded["seq"].as_u64().unwrap() as u8;
        let wire = Frame::message(Role::Request(seq), payload)
            .encode()
            .unwrap();
        assert_eq!(wire, unhex(fixture["hex"].as_str().unwrap()), "{name}");
        checked += 1;
    }
    assert_eq!(
        checked, 6,
        "stat, list, put, end, put-present and delete requests"
    );
}

#[test]
fn asset_replies_in_the_golden_frames_parse_to_typed_values() {
    let mut refusals = Vec::new();
    for fixture in fixtures() {
        if fixture["decoded"]["flags"] != 1 {
            continue;
        }
        let reply: Reply =
            serde_json::from_slice(&unhex(fixture["decoded"]["payload_hex"].as_str().unwrap()))
                .unwrap();
        let name = fixture["name"].as_str().unwrap();
        if !reply.ok {
            let error = reply_fields::<Value>(&reply).unwrap_err();
            let reason = error.reason().unwrap().clone();
            assert!(!matches!(reason, AssetReason::Other(_)), "{name}");
            assert_eq!(reason.name(), fixture["message"]["reason"], "{name}");
            refusals.push(reason);
            continue;
        }
        match name {
            "v2.asset.stat-response" => {
                let space: AssetSpace = reply_fields(&reply).unwrap();
                assert_eq!(
                    space.free_bytes,
                    space.total_bytes - space.used_bytes - space.reserved_bytes
                );
            }
            "v2.asset.list-response" => {
                let page: AssetPage = reply_fields(&reply).unwrap();
                assert_eq!(page.items[0].format, AssetFormat::Gif);
                assert_eq!(page.next, None);
            }
            "v2.asset.put-response" => {
                let opened: PutOpened = reply_fields(&reply).unwrap();
                assert_eq!((opened.present, opened.xfer), (false, Some(1)));
            }
            "v2.asset.put-present-response" => {
                let opened: PutOpened = reply_fields(&reply).unwrap();
                assert_eq!((opened.present, opened.xfer), (true, None));
            }
            "v2.asset.end-response" => {
                let committed: AssetCommitted = reply_fields(&reply).unwrap();
                assert_eq!(committed.bytes, 43);
            }
            "v2.asset.delete-response" => {
                assert_eq!(reply.fields["deleted"], true);
            }
            other => panic!("unexpected response {other}"),
        }
    }
    assert!(refusals.contains(&AssetReason::Full));
    assert!(refusals.contains(&AssetReason::Hash));
    assert!(refusals.contains(&AssetReason::InUse));
}

#[test]
fn a_second_put_of_the_same_asset_sends_no_payload_bytes() {
    let device = MockDeck::v2();
    let session = open(&device);
    let file = icon(1);
    let first = put_asset(&session, &file, AssetFormat::Tdi1).unwrap();
    assert!(first.sent);
    assert_eq!(first.id, asset_id(&file));
    assert_eq!(first.chunks, 13);
    assert_eq!(device.chunk_bytes(), ICON_BYTES);
    assert_eq!(device.assets()[&first.id], file);

    let second = put_asset(&session, &file, AssetFormat::Tdi1).unwrap();
    assert!(!second.sent);
    assert_eq!(second.chunks, 0);
    assert_eq!(
        device.chunk_bytes(),
        ICON_BYTES,
        "no chunk went the second time"
    );
    let names: Vec<String> = device
        .received()
        .iter()
        .map(|m| m["m"].as_str().unwrap().to_owned())
        .filter(|m| m.starts_with("v2.asset."))
        .collect();
    assert_eq!(names, ["v2.asset.put", "v2.asset.end", "v2.asset.put"]);
}

#[test]
fn a_full_device_refuses_the_put_with_a_typed_error_and_stores_nothing() {
    let device = MockDeck::v2();
    device.set_asset_capacity(2 * ICON_BYTES as u64);
    let session = open(&device);
    put_asset(&session, &icon(1), AssetFormat::Tdi1).unwrap();
    put_asset(&session, &icon(2), AssetFormat::Tdi1).unwrap();
    let space = asset_space(&session).unwrap();
    assert_eq!(space.free_bytes, 0);
    let sent = device.chunk_bytes();

    let error = put_asset(&session, &icon(3), AssetFormat::Tdi1).unwrap_err();
    assert_eq!(error.reason(), Some(&AssetReason::Full));
    assert!(error.to_string().contains("full"), "{error}");
    assert_eq!(device.chunk_bytes(), sent);
    assert_eq!(device.assets().len(), 2);

    assert!(delete_asset(&session, &asset_id(&icon(1))).unwrap());
    assert!(asset_space(&session).unwrap().free_bytes >= ICON_BYTES as u64);
    put_asset(&session, &icon(3), AssetFormat::Tdi1).unwrap();
}

#[test]
fn listing_pages_four_at_a_time_and_delete_reports_what_it_removed() {
    let device = MockDeck::v2();
    let session = open(&device);
    let mut ids: Vec<String> = (1..=6)
        .map(|seed| {
            put_asset(&session, &icon(seed), AssetFormat::Tdi1)
                .unwrap()
                .id
        })
        .collect();
    ids.sort();
    let first = list_asset_page(&session, None).unwrap();
    assert_eq!(first.items.len(), 4);
    assert_eq!(first.next.as_deref(), Some(ids[3].as_str()));
    let second = list_asset_page(&session, first.next.as_deref()).unwrap();
    assert_eq!(second.items.len(), 2);
    assert_eq!(second.next, None);
    let listed: Vec<String> = list_assets(&session)
        .unwrap()
        .into_iter()
        .map(|item| {
            assert_eq!(
                (item.bytes, item.format),
                (ICON_BYTES as u64, AssetFormat::Tdi1)
            );
            item.id
        })
        .collect();
    assert_eq!(listed, ids);

    assert!(delete_asset(&session, &ids[0]).unwrap());
    assert!(!delete_asset(&session, &ids[0]).unwrap());
    assert_eq!(list_assets(&session).unwrap().len(), 5);
    assert!(matches!(
        delete_asset(&session, "NOT-AN-ID"),
        Err(AssetError::Id(_))
    ));
}

#[test]
fn a_put_while_another_transfer_is_open_is_busy() {
    let device = MockDeck::v2();
    let session = open(&device);
    let held = icon(9);
    let request = AssetRequest::Put {
        id: &asset_id(&held),
        bytes: held.len(),
        format: AssetFormat::Tdi1,
    };
    let opened: PutOpened = reply_fields(
        &session
            .call_payload(request.name(), request.to_payload(), TIMEOUT)
            .unwrap(),
    )
    .unwrap();
    assert!(!opened.present);
    let error = put_asset(&session, &icon(10), AssetFormat::Tdi1).unwrap_err();
    assert_eq!(error.reason(), Some(&AssetReason::Busy));
    let end = AssetRequest::End {
        xfer: opened.xfer.unwrap(),
    };
    let incomplete = reply_fields::<Value>(
        &session
            .call_payload(end.name(), end.to_payload(), TIMEOUT)
            .unwrap(),
    )
    .unwrap_err();
    assert_eq!(incomplete.reason(), Some(&AssetReason::Incomplete));
    // A new selection discards the open upload, so the slot is free again.
    session.identity(TIMEOUT).unwrap();
    put_asset(&session, &icon(10), AssetFormat::Tdi1).unwrap();
}

#[test]
fn assets_need_protocol_2() {
    let device = MockDeck::v2();
    let session = Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap()
    .0;
    session.hello(TIMEOUT).unwrap();
    let error = asset_space(&session).unwrap_err();
    assert_eq!(error.reason(), Some(&AssetReason::NotNegotiated));
    assert!(error.to_string().contains("protocol 2"), "{error}");
}

#[test]
fn an_empty_or_oversize_file_never_leaves_the_computer() {
    let device = MockDeck::v2();
    let session = open(&device);
    assert!(matches!(
        put_asset(&session, &[], AssetFormat::Tdi1),
        Err(AssetError::Size(0))
    ));
    let error = put_asset(&session, &vec![0; ICON_BYTES + 1], AssetFormat::Tdi1).unwrap_err();
    assert_eq!(error.reason(), Some(&AssetReason::Size));
    assert_eq!(device.chunk_bytes(), 0);
}
