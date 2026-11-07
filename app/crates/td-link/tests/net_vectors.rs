//! net-v2 against `schema/protocol/net-v2-vectors.json`, the values the reference implementation
//! and the firmware also check.
//!
//! TODO(#283): add the shared negative vectors and X25519 edge cases once they merge.

use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha512};
use td_link::net::wire::{
    self, C2D, D2C, PAIR_ACCEPT, PAIR_CHALLENGE, PAIR_COMMIT, PAIR_OFFER, PAIR_PROOF, PAIR_REQUEST,
    PAIR_START, PAIR_STORED, PairKeys, RecordReceiver, RecordSender, SESSION_CHALLENGE,
    SESSION_OPEN, SESSION_PROOF, SESSION_READY, SessionKeys, SrpServer,
};
use td_link::net::{Address, Credential, NetCarrier, NetError, Refusal};
use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519};

fn vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../schema/protocol/net-v2-vectors.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("the vectors file")).unwrap()
}

fn bytes(value: &Value) -> Vec<u8> {
    let text = value.as_str().expect("a hex string");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn array<const N: usize>(value: &Value) -> [u8; N] {
    bytes(value).try_into().expect("a fixed-size value")
}

fn hex(value: &[u8]) -> String {
    wire::hex(value)
}

#[test]
fn srp_matches_the_pake_vectors() {
    let v = vectors();
    let (inputs, pake) = (&v["inputs"], &v["pake"]);
    let device_id: [u8; 6] = array(&inputs["deviceId"]);
    let companion_id: [u8; 16] = array(&inputs["companionId"]);
    let salt: [u8; 16] = array(&inputs["salt"]);
    let code = inputs["code"].as_str().unwrap();
    let identity = wire::identity(&device_id, &companion_id);
    assert_eq!(identity, pake["identity"].as_str().unwrap().as_bytes());

    let a_pub = wire::srp_client_public(&array(&inputs["a"])).unwrap();
    assert_eq!(hex(&a_pub), pake["A"].as_str().unwrap());
    let server = SrpServer::new(&identity, code, &salt, &array(&inputs["b"])).unwrap();
    assert_eq!(hex(&server.b_pub), pake["B"].as_str().unwrap());
    let u = wire::srp_u(&a_pub, &server.b_pub).unwrap();
    assert_eq!(hex(&wire::min(&u)), pake["u"].as_str().unwrap());

    let k_srp = pake["K_srp"].as_str().unwrap();
    let s = bytes(&pake["S"]);
    assert_eq!(hex(&Sha512::digest(&s)), k_srp, "K_srp = H512(MIN(S))");
    let client =
        wire::srp_client_key(&identity, code, &salt, &array(&inputs["a"]), &server.b_pub).unwrap();
    assert_eq!(hex(&client), k_srp);
    assert_eq!(hex(&server.key(&a_pub).unwrap()), k_srp);
}

#[test]
fn pairing_matches_the_wire_keys_and_proofs() {
    let v = vectors();
    let (inputs, pairing) = (&v["inputs"], &v["pairing"]);
    let wire_items = &pairing["wire"];
    let device_id: [u8; 6] = array(&inputs["deviceId"]);
    let companion_id: [u8; 16] = array(&inputs["companionId"]);
    let name = inputs["companionName"].as_str().unwrap();
    let ltk: [u8; 32] = array(&inputs["ltk"]);

    let request = wire::handshake(
        PAIR_REQUEST,
        &[&companion_id[..], &[name.len() as u8], name.as_bytes()].concat(),
    )
    .unwrap();
    assert_eq!(hex(&request), wire_items["request"].as_str().unwrap());
    let offer = bytes(&wire_items["offer"]);
    let (kind, fields) = wire::parse_handshake(&offer).unwrap();
    assert_eq!(kind, PAIR_OFFER);
    assert_eq!(fields[..6], device_id);
    assert_eq!(fields[6..22], bytes(&inputs["windowId"])[..]);

    let a_pub = wire::srp_client_public(&array(&inputs["a"])).unwrap();
    let start = wire::handshake(
        PAIR_START,
        &[&a_pub[..], &bytes(&inputs["clientNonce"])].concat(),
    )
    .unwrap();
    assert_eq!(hex(&start), wire_items["start"].as_str().unwrap());
    let challenge = bytes(&wire_items["challenge"]);
    assert_eq!(wire::parse_handshake(&challenge).unwrap().0, PAIR_CHALLENGE);

    let k_srp: [u8; 64] = array(&v["pake"]["K_srp"]);
    let keys = PairKeys::derive(&k_srp, &request, &offer, &start, &challenge).unwrap();
    for (name, got) in [
        ("transcript", keys.transcript),
        ("prk", keys.prk),
        ("client", keys.client),
        ("deck", keys.deck),
        ("wrap", keys.wrap),
        ("commit", keys.commit),
        ("clientProof", keys.client_proof),
        ("deckProof", keys.deck_proof),
    ] {
        assert_eq!(hex(&got), pairing[name].as_str().unwrap(), "{name}");
    }
    assert_eq!(
        hex(&keys.wrap_key(&ltk)),
        pairing["wrapped_key"].as_str().unwrap()
    );

    let proof = wire::handshake(PAIR_PROOF, &keys.client_proof).unwrap();
    assert_eq!(hex(&proof), wire_items["proof"].as_str().unwrap());
    let accept = bytes(&wire_items["accept"]);
    let (kind, fields) = wire::parse_handshake(&accept).unwrap();
    assert_eq!(kind, PAIR_ACCEPT);
    assert_eq!(keys.accept(fields).unwrap(), ltk);
    let commit = wire::handshake(PAIR_COMMIT, &keys.commit_proof(&ltk)).unwrap();
    assert_eq!(hex(&commit), wire_items["commit"].as_str().unwrap());
    let stored = wire::handshake(PAIR_STORED, &keys.stored_proof(&ltk)).unwrap();
    assert_eq!(hex(&stored), wire_items["stored"].as_str().unwrap());

    let mut tampered = fields.to_vec();
    tampered[40] ^= 1;
    assert!(
        keys.accept(&tampered).is_err(),
        "a changed wrapped key fails"
    );
    tampered = fields.to_vec();
    tampered[0] ^= 1;
    assert!(
        keys.accept(&tampered).is_err(),
        "a changed device proof fails"
    );
}

#[test]
fn session_matches_the_shares_keys_and_wire() {
    let v = vectors();
    let (inputs, session) = (&v["inputs"], &v["session"]);
    let device_id: [u8; 6] = array(&inputs["deviceId"]);
    let companion_id: [u8; 16] = array(&inputs["companionId"]);
    let client_secret: [u8; 32] = array(&inputs["clientEphemeral"]);
    let client_share = x25519(client_secret, X25519_BASEPOINT_BYTES);
    let device_share = x25519(array(&inputs["deviceEphemeral"]), X25519_BASEPOINT_BYTES);
    assert_eq!(
        hex(&client_share),
        session["client_share"].as_str().unwrap()
    );
    assert_eq!(
        hex(&device_share),
        session["device_share"].as_str().unwrap()
    );
    let z = x25519(client_secret, device_share);
    assert_eq!(hex(&z), session["Z"].as_str().unwrap());

    let keys = SessionKeys::derive(
        &array(&inputs["ltk"]),
        &device_id,
        &companion_id,
        &client_share,
        &device_share,
        &z,
    )
    .unwrap();
    for (name, got) in [
        ("transcript", keys.transcript),
        ("prk", keys.prk),
        ("deck", keys.deck),
        ("client", keys.client),
        ("ready", keys.ready),
        ("c2d", keys.c2d),
        ("d2c", keys.d2c),
        ("deckProof", keys.deck_proof),
        ("clientProof", keys.client_proof),
        ("readyProof", keys.ready_proof),
    ] {
        assert_eq!(hex(&got), session[name].as_str().unwrap(), "{name}");
    }
    let w = &session["wire"];
    let open = wire::handshake(
        SESSION_OPEN,
        &[&device_id[..], &companion_id, &client_share].concat(),
    )
    .unwrap();
    assert_eq!(hex(&open), w["open"].as_str().unwrap());
    let challenge = wire::handshake(
        SESSION_CHALLENGE,
        &[&device_share[..], &keys.deck_proof].concat(),
    )
    .unwrap();
    assert_eq!(hex(&challenge), w["challenge"].as_str().unwrap());
    let proof = wire::handshake(SESSION_PROOF, &keys.client_proof).unwrap();
    assert_eq!(hex(&proof), w["proof"].as_str().unwrap());
    let ready = wire::handshake(SESSION_READY, &keys.ready_proof).unwrap();
    assert_eq!(hex(&ready), w["ready"].as_str().unwrap());
}

fn direction(value: &Value) -> u8 {
    match value.as_str().unwrap() {
        "c2d" => C2D,
        "d2c" => D2C,
        other => panic!("unknown direction {other}"),
    }
}

fn record_key(v: &Value, dir: u8) -> [u8; 32] {
    array(&v["session"][if dir == C2D { "c2d" } else { "d2c" }])
}

#[test]
fn records_seal_and_open_as_the_vectors_say() {
    let v = vectors();
    for record in v["records"].as_array().unwrap() {
        let name = record["name"].as_str().unwrap();
        let dir = direction(&record["direction"]);
        let counter = record["counter"].as_u64().unwrap();
        let plaintext = bytes(&record["plaintext"]);
        let key = record_key(&v, dir);
        let (nonce, aad) =
            wire::record_parameters(dir, counter, plaintext.len() + wire::TAG_LEN).unwrap();
        assert_eq!(
            hex(&nonce),
            record["nonce"].as_str().unwrap(),
            "{name} nonce"
        );
        assert_eq!(hex(&aad), record["aad"].as_str().unwrap(), "{name} aad");
        let sealed = RecordSender::at(&key, dir, counter)
            .seal(&plaintext)
            .unwrap();
        assert_eq!(
            hex(&sealed),
            record["wire"].as_str().unwrap(),
            "{name} wire"
        );
        let opened = RecordReceiver::at(&key, dir, counter)
            .open(&sealed)
            .unwrap();
        assert_eq!(opened, plaintext, "{name} opens");
    }
}

#[test]
fn a_tampered_record_closes_the_receiver() {
    let v = vectors();
    let tampered = &v["tampered_record"];
    assert_eq!(tampered["expected"], "close");
    let dir = direction(&tampered["direction"]);
    let counter = tampered["counter"].as_u64().unwrap();
    let key = record_key(&v, dir);
    let mut receiver = RecordReceiver::at(&key, dir, counter);
    assert!(receiver.open(&bytes(&tampered["wire"])).is_err());
    let genuine = v["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| direction(&r["direction"]) == dir && r["counter"].as_u64() == Some(counter))
        .expect("a genuine record at the same counter");
    assert!(
        receiver.open(&bytes(&genuine["wire"])).is_err(),
        "the closed direction stays closed"
    );
}

#[test]
fn the_error_items_match() {
    let v = vectors();
    assert_eq!(
        hex(&wire::error_item(Refusal::Busy)),
        v["errors"]["busy"].as_str().unwrap()
    );
    assert_eq!(
        hex(&wire::error_item(Refusal::Closed)),
        v["errors"]["closed"].as_str().unwrap()
    );
}

#[test]
fn a_non_contributory_share_closes_the_handshake() {
    let v = vectors();
    let inputs = &v["inputs"];
    let zero: [u8; 32] = array(&v["non_contributory_share"]["device_share"]);
    assert_eq!(v["non_contributory_share"]["expected"], "close");
    let device_id: [u8; 6] = array(&inputs["deviceId"]);
    let companion_id: [u8; 16] = array(&inputs["companionId"]);
    let client_secret: [u8; 32] = array(&inputs["clientEphemeral"]);
    let z = x25519(client_secret, zero);
    let client_share = x25519(client_secret, X25519_BASEPOINT_BYTES);
    assert!(
        SessionKeys::derive(
            &array(&inputs["ltk"]),
            &device_id,
            &companion_id,
            &client_share,
            &zero,
            &z
        )
        .is_err()
    );

    // A scripted Device that answers session.open with the zero share.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = Address::from(listener.local_addr().unwrap());
    let device = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut open = [0u8; 58];
        std::io::Read::read_exact(&mut stream, &mut open).unwrap();
        let challenge =
            wire::handshake(SESSION_CHALLENGE, &[&zero[..], &[0x55u8; 32]].concat()).unwrap();
        stream.write_all(&challenge).unwrap();
        let started = Instant::now();
        let mut rest = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stream, &mut rest);
        (rest, started.elapsed())
    });
    let credential = Credential {
        device_id,
        companion_id,
        ltk: array(&inputs["ltk"]),
    };
    let result = NetCarrier::connect(&addr, &credential, Duration::from_secs(5));
    assert!(
        matches!(result, Err(NetError::Auth(_))),
        "{:?}",
        result.err()
    );
    let (rest, waited) = device.join().unwrap();
    assert!(rest.is_empty(), "no session.proof follows a zero share");
    assert!(waited < Duration::from_secs(2), "the client closed at once");
}
