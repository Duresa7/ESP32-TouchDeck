use serde_json::json;
use std::time::Duration;
use td_link::{MockDeck, Session, SessionConfig};
const TIMEOUT: Duration = Duration::from_secs(1);
fn open(device: &MockDeck) -> Session {
    Session::open(
        Box::new(device.connect()),
        SessionConfig {
            ping_every: None,
            ..SessionConfig::default()
        },
    )
    .unwrap()
    .0
}
#[test]
fn legacy_firmware_remains_usable_without_v2_requests() {
    let device = MockDeck::new();
    let session = open(&device);
    let identity = session.identity(TIMEOUT).unwrap();
    assert_eq!(identity.protocol, 1);
    assert!(identity.name.is_none() && identity.capabilities.is_none() && identity.id.is_none());
    assert!(session.set_name("Desk", TIMEOUT).is_err());
    assert!(session.hello(TIMEOUT).unwrap().is_deck());
    assert!(device.received().iter().all(|v| v["m"] == "hello"));
}
#[test]
fn negotiated_name_round_trips_and_survives_reconnect() {
    let device = MockDeck::v2();
    {
        let session = open(&device);
        let identity = session.identity(TIMEOUT).unwrap();
        assert!(identity.matches_usb_serial(device.usb_serial()));
        assert!(!identity.matches_usb_serial("02:00:00:00:00:02"));
        assert_eq!(identity.capabilities.unwrap().tile_kinds, ["static"]);
        assert_eq!(
            session.set_name("  Studio \u{1f39b}  ", TIMEOUT).unwrap(),
            "  Studio \u{1f39b}  "
        );
        assert_eq!(session.name(TIMEOUT).unwrap(), "  Studio \u{1f39b}  ");
    }
    assert_eq!(open(&device).name(TIMEOUT).unwrap(), "  Studio \u{1f39b}  ");
}
#[test]
fn invalid_names_preserve_the_committed_name() {
    let device = MockDeck::v2();
    let session = open(&device);
    session.identity(TIMEOUT).unwrap();
    for name in [
        "".to_owned(),
        " \u{2003}".into(),
        "bad\nname".into(),
        "bad\0name".into(),
        "x".repeat(49),
        "\u{1f39b}".repeat(13),
    ] {
        assert!(session.set_name(&name, TIMEOUT).is_err());
        let reply = session
            .call(&json!({"m":"v2.name.set","name":name}), TIMEOUT)
            .unwrap();
        assert!(!reply.ok);
        assert_eq!(reply.fields["reason"], "args");
    }
    assert_eq!(session.name(TIMEOUT).unwrap(), "Open TouchDeck");
    assert_eq!(
        session.set_name(&"x".repeat(48), TIMEOUT).unwrap(),
        "x".repeat(48)
    );
}
#[test]
fn selection_and_unsupported_requests_have_stable_errors() {
    let device = MockDeck::v2();
    let session = open(&device);
    let request = json!({"m":"v2.asset.stat"});
    assert_eq!(
        session.call(&request, TIMEOUT).unwrap().fields["reason"],
        "not-negotiated"
    );
    session.identity(TIMEOUT).unwrap();
    assert_eq!(
        session.call(&request, TIMEOUT).unwrap().fields["reason"],
        "unsupported"
    );
    session.hello(TIMEOUT).unwrap();
    assert_eq!(
        session.call(&request, TIMEOUT).unwrap().fields["reason"],
        "not-negotiated"
    );
}

#[test]
fn discovery_matches_normalized_usb_serial_and_refuses_mismatch() {
    use td_link::discover::probe_identity_transport;
    let device = MockDeck::v2();
    for serial in [device.usb_serial(), "02-00-00-00-00-01", "020000000001"] {
        let identity = probe_identity_transport(Box::new(device.connect()), Some(serial)).unwrap();
        assert_eq!(identity.id.as_deref(), Some("020000000001"));
    }
    for serial in [None, Some("020000000002")] {
        assert!(probe_identity_transport(Box::new(device.connect()), serial).is_err());
    }
    let mut identity = open(&device).identity(TIMEOUT).unwrap();
    identity.id = Some("aabbccddeeff".into());
    assert!(identity.matches_usb_serial("AA:BB:CC:DD:EE:FF"));
    let legacy = MockDeck::new();
    assert_eq!(
        probe_identity_transport(Box::new(legacy.connect()), None)
            .unwrap()
            .protocol,
        1
    );
}
