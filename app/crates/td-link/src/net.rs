//! `net-v1` authenticated and encrypted LAN transport (`schema/protocol/net-v1.md`).

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use srp::client::SrpClient;
use srp::groups::G_3072;
use subtle::ConstantTimeEq;
use td_core::{Frame, Item, StreamDecoder};
use zeroize::Zeroize;

use crate::{LinkError, READ_TICK, Transport};

type HmacSha256 = Hmac<Sha256>;

pub const NET_DEFAULT_PORT: u16 = 47_820;
pub const NET_PROTO_VERSION: u8 = 1;
pub const DECK_ID_LEN: usize = 6;
pub const APP_ID_LEN: usize = 16;
pub const WINDOW_ID_LEN: usize = 16;
pub const SALT_LEN: usize = 16;
pub const NONCE32_LEN: usize = 32;
pub const SRP_PAD_LEN: usize = 384;
pub const LTK_LEN: usize = 32;
pub const WRAPPED_KEY_LEN: usize = 48;
pub const TAG_LEN: usize = 16;
pub const RECORD_PREFIX_LEN: usize = 2;
pub const MIN_RECORD_BODY_LEN: usize = 24;
pub const MAX_RECORD_BODY_LEN: usize = 1_048;
pub const MAX_HANDSHAKE_BODY_LEN: usize = 512;
pub const COUNTER_LIMIT: u64 = 1u64 << 32;
pub const PAIR_WINDOW_MS: u32 = 120_000;
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub const KIND_PAIR_REQUEST: u8 = 0x10;
pub const KIND_PAIR_OFFER: u8 = 0x11;
pub const KIND_PAIR_START: u8 = 0x12;
pub const KIND_PAIR_CHALLENGE: u8 = 0x13;
pub const KIND_PAIR_PROOF: u8 = 0x14;
pub const KIND_PAIR_ACCEPT: u8 = 0x15;
pub const KIND_PAIR_COMMIT: u8 = 0x16;
pub const KIND_PAIR_STORED: u8 = 0x17;
pub const KIND_SESSION_OPEN: u8 = 0x20;
pub const KIND_SESSION_CHALLENGE: u8 = 0x21;
pub const KIND_SESSION_PROOF: u8 = 0x22;
pub const KIND_SESSION_READY: u8 = 0x23;
pub const KIND_NET_ERROR: u8 = 0x7f;

pub const DIR_C2D: u8 = 0;
pub const DIR_D2C: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetErrorReason {
    Busy = 1,
    Version = 2,
    Args = 3,
    Full = 4,
    Auth = 5,
    Expired = 6,
    Cancelled = 7,
    Storage = 8,
}

impl NetErrorReason {
    pub fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Busy),
            2 => Some(Self::Version),
            3 => Some(Self::Args),
            4 => Some(Self::Full),
            5 => Some(Self::Auth),
            6 => Some(Self::Expired),
            7 => Some(Self::Cancelled),
            8 => Some(Self::Storage),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Version => "version",
            Self::Args => "args",
            Self::Full => "full",
            Self::Auth => "auth",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
            Self::Storage => "storage",
        }
    }
}

#[derive(Debug)]
pub enum NetError {
    Remote(NetErrorReason),
    Local {
        reason: NetErrorReason,
        detail: String,
    },
    Io(io::Error),
    Frame(td_core::FrameError),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Remote(reason) => write!(f, "handshake refused ({})", reason.as_str()),
            NetError::Local { reason, detail } => {
                write!(f, "net-v1 {} error: {detail}", reason.as_str())
            }
            NetError::Io(e) => write!(f, "net-v1 io: {e}"),
            NetError::Frame(e) => write!(f, "net-v1 frame error: {e}"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

impl From<td_core::FrameError> for NetError {
    fn from(e: td_core::FrameError) -> Self {
        NetError::Frame(e)
    }
}

pub fn pad_be(bytes: &[u8], width: usize) -> Vec<u8> {
    assert!(bytes.len() <= width, "integer exceeds pad width");
    let mut out = vec![0u8; width];
    let offset = width - bytes.len();
    out[offset..].copy_from_slice(bytes);
    out
}

pub fn pad_384(n: &BigUint) -> [u8; SRP_PAD_LEN] {
    let raw = min_be(n);
    assert!(raw.len() <= SRP_PAD_LEN, "integer overflow");
    let mut out = [0u8; SRP_PAD_LEN];
    out[SRP_PAD_LEN - raw.len()..].copy_from_slice(&raw);
    out
}

pub fn min_be(n: &BigUint) -> Vec<u8> {
    if *n == BigUint::from(0u8) {
        vec![0u8]
    } else {
        n.to_bytes_be()
    }
}

pub fn identity_bytes(deck_id: &[u8; DECK_ID_LEN], app_id: &[u8; APP_ID_LEN]) -> Vec<u8> {
    format!("touchdeck:{}:{}", hex::encode(deck_id), hex::encode(app_id)).into_bytes()
}

pub fn validate_code(code: &str) -> Result<(), NetError> {
    if code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid code".into(),
        })
    }
}

pub fn validate_app_name(name: &str) -> Result<(), NetError> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 || name.chars().any(|c| c.is_control()) {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid app_name".into(),
        });
    }
    Ok(())
}

fn check_private_exponent(exp: &[u8; 32]) -> Result<BigUint, NetError> {
    let val = BigUint::from_bytes_be(exp);
    if val == BigUint::from(0u8) {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "zero private exponent".into(),
        });
    }
    Ok(val)
}

pub fn validate_public_value(pub_bytes: &[u8; SRP_PAD_LEN]) -> Result<BigUint, NetError> {
    let n = &G_3072.n;
    let val = BigUint::from_bytes_be(pub_bytes);
    if val == BigUint::from(0u8) || &val >= n {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid public value".into(),
        });
    }
    Ok(val)
}

pub fn srp_multiplier_k() -> BigUint {
    let n_pad = pad_384(&G_3072.n);
    let g_pad = pad_384(&G_3072.g);
    let mut h = Sha512::new();
    h.update(n_pad);
    h.update(g_pad);
    BigUint::from_bytes_be(&h.finalize())
}

pub fn srp_scrambling_u(
    a_pub: &[u8; SRP_PAD_LEN],
    b_pub: &[u8; SRP_PAD_LEN],
) -> Result<BigUint, NetError> {
    validate_public_value(a_pub)?;
    validate_public_value(b_pub)?;
    let mut h = Sha512::new();
    h.update(a_pub);
    h.update(b_pub);
    let u = BigUint::from_bytes_be(&h.finalize());
    if u == BigUint::from(0u8) {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "zero scrambling parameter".into(),
        });
    }
    Ok(u)
}

pub fn srp_password_exponent(
    identity: &[u8],
    code: &str,
    salt: &[u8; SALT_LEN],
) -> Result<BigUint, NetError> {
    validate_code(code)?;
    let id_hash = SrpClient::<Sha512>::compute_identity_hash(identity, code.as_bytes());
    Ok(SrpClient::<Sha512>::compute_x(id_hash.as_slice(), salt))
}

pub fn srp_client_public(a_priv: &[u8; 32]) -> Result<[u8; SRP_PAD_LEN], NetError> {
    check_private_exponent(a_priv)?;
    let client = SrpClient::<Sha512>::new(&G_3072);
    let a_pub = client.compute_public_ephemeral(a_priv);
    let padded = pad_be(&a_pub, SRP_PAD_LEN);
    let mut out = [0u8; SRP_PAD_LEN];
    out.copy_from_slice(&padded);
    validate_public_value(&out)?;
    Ok(out)
}

pub struct ServerSrpPublic {
    pub x: BigUint,
    pub v: BigUint,
    pub b_pub: [u8; SRP_PAD_LEN],
}

pub fn srp_server_public(
    identity: &[u8],
    code: &str,
    salt: &[u8; SALT_LEN],
    b_priv: &[u8; 32],
) -> Result<ServerSrpPublic, NetError> {
    let b = check_private_exponent(b_priv)?;
    let x = srp_password_exponent(identity, code, salt)?;
    let n = &G_3072.n;
    let g = &G_3072.g;
    let v = g.modpow(&x, n);
    let k = srp_multiplier_k();
    let gb = g.modpow(&b, n);
    let b_val = ((&k * &v) + gb) % n;
    let b_pub = pad_384(&b_val);
    validate_public_value(&b_pub)?;
    Ok(ServerSrpPublic { x, v, b_pub })
}

pub struct ClientSrpSecret {
    pub a_pub: [u8; SRP_PAD_LEN],
    pub x: BigUint,
    pub u: BigUint,
    pub s: BigUint,
    pub k_srp: [u8; 64],
}

pub fn srp_client_secret(
    identity: &[u8],
    code: &str,
    salt: &[u8; SALT_LEN],
    a_priv: &[u8; 32],
    b_pub: &[u8; SRP_PAD_LEN],
) -> Result<ClientSrpSecret, NetError> {
    let a = check_private_exponent(a_priv)?;
    let a_pub = srp_client_public(a_priv)?;
    let b_val = validate_public_value(b_pub)?;
    let x = srp_password_exponent(identity, code, salt)?;
    let u = srp_scrambling_u(&a_pub, b_pub)?;
    let n = &G_3072.n;
    let g = &G_3072.g;
    let k = srp_multiplier_k();
    let gx = g.modpow(&x, n);
    let kgx = (&k * gx) % n;
    let base = (&b_val + n - kgx) % n;
    if base == BigUint::from(0u8) {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "zero client base".into(),
        });
    }
    let client = SrpClient::<Sha512>::new(&G_3072);
    let s = client.compute_premaster_secret(&b_val, &k, &x, &a, &u);
    if s == BigUint::from(0u8) {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "zero secret".into(),
        });
    }
    let s_min = min_be(&s);
    let digest = Sha512::digest(&s_min);
    let mut k_srp = [0u8; 64];
    k_srp.copy_from_slice(&digest);
    Ok(ClientSrpSecret {
        a_pub,
        x,
        u,
        s,
        k_srp,
    })
}

pub struct ServerSrpSecret {
    pub b_pub: [u8; SRP_PAD_LEN],
    pub x: BigUint,
    pub v: BigUint,
    pub u: BigUint,
    pub s: BigUint,
    pub k_srp: [u8; 64],
}

pub fn srp_server_secret(
    identity: &[u8],
    code: &str,
    salt: &[u8; SALT_LEN],
    b_priv: &[u8; 32],
    a_pub: &[u8; SRP_PAD_LEN],
) -> Result<ServerSrpSecret, NetError> {
    let b = check_private_exponent(b_priv)?;
    let ServerSrpPublic { x, v, b_pub } = srp_server_public(identity, code, salt, b_priv)?;
    let a_val = validate_public_value(a_pub)?;
    let u = srp_scrambling_u(a_pub, &b_pub)?;
    let n = &G_3072.n;
    let vu = v.modpow(&u, n);
    let base = (a_val * vu) % n;
    let s = base.modpow(&b, n);
    if s == BigUint::from(0u8) {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "zero secret".into(),
        });
    }
    let s_min = min_be(&s);
    let digest = Sha512::digest(&s_min);
    let mut k_srp = [0u8; 64];
    k_srp.copy_from_slice(&digest);
    Ok(ServerSrpSecret {
        b_pub,
        x,
        v,
        u,
        s,
        k_srp,
    })
}

pub fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    let d = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

pub fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key).expect("HMAC key");
    for p in parts {
        mac.update(p);
    }
    let d = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

pub fn hkdf_extract_and_derive<const N: usize>(
    salt: &[u8; 32],
    ikm: &[u8],
    labels: &[&str; N],
) -> ([u8; 32], [[u8; 32]; N]) {
    let (prk_arr, hk) = Hkdf::<Sha256>::extract(Some(salt), ikm);
    let mut prk = [0u8; 32];
    prk.copy_from_slice(&prk_arr);
    let mut derived = [[0u8; 32]; N];
    for (i, label) in labels.iter().enumerate() {
        let info = format!("touchdeck/net-v1/{label}");
        hk.expand(info.as_bytes(), &mut derived[i])
            .expect("HKDF-Expand 32 bytes");
    }
    (prk, derived)
}

pub fn validate_handshake_fields(kind: u8, fields: &[u8]) -> Result<(), NetError> {
    if kind == KIND_PAIR_REQUEST {
        if fields.len() < 18 || fields.len() > 81 {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: "invalid pair.request length".into(),
            });
        }
        let name_len = fields[16] as usize;
        let name_bytes = &fields[17..];
        if name_len != name_bytes.len() {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: "invalid pair.request name_len".into(),
            });
        }
        let name = std::str::from_utf8(name_bytes).map_err(|_| NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid UTF-8 in app_name".into(),
        })?;
        validate_app_name(name)?;
        return Ok(());
    }
    let expected = match kind {
        KIND_PAIR_OFFER => 26,
        KIND_PAIR_START => 416,
        KIND_PAIR_CHALLENGE => 432,
        KIND_PAIR_PROOF => 32,
        KIND_PAIR_ACCEPT => 80,
        KIND_PAIR_COMMIT => 32,
        KIND_PAIR_STORED => 32,
        KIND_SESSION_OPEN => 54,
        KIND_SESSION_CHALLENGE => 64,
        KIND_SESSION_PROOF => 32,
        KIND_SESSION_READY => 32,
        KIND_NET_ERROR => 1,
        _ => {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: format!("unknown handshake kind 0x{kind:02x}"),
            })
        }
    };
    if fields.len() != expected {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: format!(
                "invalid handshake length {} for kind 0x{kind:02x}",
                fields.len()
            ),
        });
    }
    if kind == KIND_NET_ERROR && NetErrorReason::from_u8(fields[0]).is_none() {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid net.error reason".into(),
        });
    }
    if kind == KIND_PAIR_OFFER {
        let rem = u32::from_be_bytes([fields[22], fields[23], fields[24], fields[25]]);
        if rem == 0 || rem > PAIR_WINDOW_MS {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: "invalid window duration".into(),
            });
        }
    }
    Ok(())
}

pub fn encode_handshake(kind: u8, fields: &[u8]) -> Result<Vec<u8>, NetError> {
    validate_handshake_fields(kind, fields)?;
    let body_len = (fields.len() + 2) as u16;
    let mut out = Vec::with_capacity(2 + fields.len() + 2);
    out.extend_from_slice(&body_len.to_be_bytes());
    out.push(kind);
    out.push(NET_PROTO_VERSION);
    out.extend_from_slice(fields);
    Ok(out)
}

pub fn encode_net_error(reason: NetErrorReason) -> Vec<u8> {
    encode_handshake(KIND_NET_ERROR, &[reason as u8]).expect("net.error encodes")
}

pub fn parse_handshake(
    wire: &[u8],
    expected_kind: Option<u8>,
) -> Result<(u8, &[u8]), NetError> {
    if wire.len() < 4 || wire.len() > 2 + MAX_HANDSHAKE_BODY_LEN {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid handshake size".into(),
        });
    }
    let declared = u16::from_be_bytes([wire[0], wire[1]]) as usize;
    if declared != wire.len() - 2 {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid handshake prefix".into(),
        });
    }
    let kind = wire[2];
    let version = wire[3];
    if version != NET_PROTO_VERSION {
        return Err(NetError::Local {
            reason: NetErrorReason::Version,
            detail: format!("unsupported handshake version {version}"),
        });
    }
    let fields = &wire[4..];
    validate_handshake_fields(kind, fields)?;
    if let Some(exp) = expected_kind {
        if kind != exp {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: format!("expected handshake kind 0x{exp:02x}, got 0x{kind:02x}"),
            });
        }
    }
    Ok((kind, fields))
}

#[derive(Clone)]
pub struct PairKeys {
    pub transcript: [u8; 32],
    pub prk: [u8; 32],
    pub client: [u8; 32],
    pub deck: [u8; 32],
    pub wrap: [u8; 32],
    pub commit: [u8; 32],
    pub client_proof: [u8; 32],
    pub deck_proof: [u8; 32],
}

impl Drop for PairKeys {
    fn drop(&mut self) {
        self.prk.zeroize();
        self.client.zeroize();
        self.deck.zeroize();
        self.wrap.zeroize();
        self.commit.zeroize();
    }
}

pub fn derive_pair_keys(
    k_srp: &[u8; 64],
    request_wire: &[u8],
    offer_wire: &[u8],
    start_wire: &[u8],
    challenge_wire: &[u8],
) -> Result<PairKeys, NetError> {
    parse_handshake(request_wire, Some(KIND_PAIR_REQUEST))?;
    parse_handshake(offer_wire, Some(KIND_PAIR_OFFER))?;
    parse_handshake(start_wire, Some(KIND_PAIR_START))?;
    parse_handshake(challenge_wire, Some(KIND_PAIR_CHALLENGE))?;

    let transcript = sha256(&[
        b"touchdeck/net-v1/pair\0",
        request_wire,
        offer_wire,
        start_wire,
        challenge_wire,
    ]);
    let (prk, [client, deck, wrap, commit]) = hkdf_extract_and_derive(
        &transcript,
        k_srp,
        &["pair/client", "pair/deck", "pair/wrap", "pair/commit"],
    );
    let client_proof = hmac_sha256(&client, &[&transcript]);
    let deck_proof = hmac_sha256(&deck, &[&transcript, &client_proof]);

    Ok(PairKeys {
        transcript,
        prk,
        client,
        deck,
        wrap,
        commit,
        client_proof,
        deck_proof,
    })
}

pub fn aes256_gcm_seal(
    key: &[u8; 32],
    nonce: &[u8; 12],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, NetError> {
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key");
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "AES-256-GCM encrypt failed".into(),
        })
}

pub fn aes256_gcm_open(
    key: &[u8; 32],
    nonce: &[u8; 12],
    sealed: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, NetError> {
    if sealed.len() < TAG_LEN {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "missing tag".into(),
        });
    }
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key");
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad,
            },
        )
        .map_err(|_| NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "AES-256-GCM tag verification failed".into(),
        })
}

pub fn wrap_key(keys: &PairKeys, ltk: &[u8; LTK_LEN]) -> Result<[u8; WRAPPED_KEY_LEN], NetError> {
    let mut aad = [0u8; 96];
    aad[0..32].copy_from_slice(&keys.transcript);
    aad[32..64].copy_from_slice(&keys.client_proof);
    aad[64..96].copy_from_slice(&keys.deck_proof);
    let sealed = aes256_gcm_seal(&keys.wrap, &[0u8; 12], ltk, &aad)?;
    let mut out = [0u8; WRAPPED_KEY_LEN];
    out.copy_from_slice(&sealed);
    Ok(out)
}

pub fn accept_key(keys: &PairKeys, acceptance_wire: &[u8]) -> Result<[u8; LTK_LEN], NetError> {
    let (_, fields) = parse_handshake(acceptance_wire, Some(KIND_PAIR_ACCEPT))?;
    if fields[0..32].ct_eq(&keys.deck_proof).unwrap_u8() != 1 {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "bad deck proof".into(),
        });
    }
    let mut aad = [0u8; 96];
    aad[0..32].copy_from_slice(&keys.transcript);
    aad[32..64].copy_from_slice(&keys.client_proof);
    aad[64..96].copy_from_slice(&keys.deck_proof);
    let pt = aes256_gcm_open(&keys.wrap, &[0u8; 12], &fields[32..], &aad)?;
    let mut ltk = [0u8; LTK_LEN];
    ltk.copy_from_slice(&pt);
    Ok(ltk)
}

pub fn commit_proof(keys: &PairKeys, ltk: &[u8; LTK_LEN]) -> [u8; 32] {
    let ltk_hash = sha256(&[ltk]);
    hmac_sha256(&keys.commit, &[b"commit", &keys.transcript, &ltk_hash])
}

pub fn stored_proof(keys: &PairKeys, ltk: &[u8; LTK_LEN]) -> [u8; 32] {
    let ltk_hash = sha256(&[ltk]);
    hmac_sha256(&keys.commit, &[b"stored", &keys.transcript, &ltk_hash])
}

#[derive(Clone)]
pub struct SessionKeys {
    pub transcript: [u8; 32],
    pub prk: [u8; 32],
    pub deck: [u8; 32],
    pub client: [u8; 32],
    pub ready: [u8; 32],
    pub c2d: [u8; 32],
    pub d2c: [u8; 32],
    pub deck_proof: [u8; 32],
    pub client_proof: [u8; 32],
    pub ready_proof: [u8; 32],
}

impl Drop for SessionKeys {
    fn drop(&mut self) {
        self.prk.zeroize();
        self.deck.zeroize();
        self.client.zeroize();
        self.ready.zeroize();
        self.c2d.zeroize();
        self.d2c.zeroize();
    }
}

pub fn derive_session_keys(
    ltk: &[u8; LTK_LEN],
    deck_id: &[u8; DECK_ID_LEN],
    app_id: &[u8; APP_ID_LEN],
    client_nonce: &[u8; NONCE32_LEN],
    deck_nonce: &[u8; NONCE32_LEN],
) -> SessionKeys {
    let transcript = sha256(&[
        b"touchdeck/net-v1/session\0",
        deck_id,
        app_id,
        client_nonce,
        deck_nonce,
    ]);
    let (prk, [deck, client, ready, c2d, d2c]) = hkdf_extract_and_derive(
        &transcript,
        ltk,
        &[
            "session/deck",
            "session/client",
            "session/ready",
            "session/c2d",
            "session/d2c",
        ],
    );
    let deck_proof = hmac_sha256(&deck, &[&transcript]);
    let client_proof = hmac_sha256(&client, &[&transcript, &deck_proof]);
    let ready_proof = hmac_sha256(&ready, &[&transcript, &client_proof]);

    SessionKeys {
        transcript,
        prk,
        deck,
        client,
        ready,
        c2d,
        d2c,
        deck_proof,
        client_proof,
        ready_proof,
    }
}

pub fn record_parameters(
    direction: u8,
    counter: u64,
    length: usize,
) -> Result<([u8; 12], [u8; 15]), NetError> {
    if direction != DIR_C2D && direction != DIR_D2C {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: "invalid direction".into(),
        });
    }
    if counter >= COUNTER_LIMIT {
        return Err(NetError::Local {
            reason: NetErrorReason::Auth,
            detail: "counter exhausted".into(),
        });
    }
    if !(MIN_RECORD_BODY_LEN..=MAX_RECORD_BODY_LEN).contains(&length) {
        return Err(NetError::Local {
            reason: NetErrorReason::Args,
            detail: format!("invalid record length {length}"),
        });
    }
    let mut nonce = [0u8; 12];
    nonce[4..12].copy_from_slice(&counter.to_be_bytes());

    let mut aad = [0u8; 15];
    aad[0..4].copy_from_slice(b"TDN1");
    aad[4] = direction;
    aad[5..13].copy_from_slice(&counter.to_be_bytes());
    aad[13..15].copy_from_slice(&(length as u16).to_be_bytes());

    Ok((nonce, aad))
}

pub struct RecordSender {
    key: [u8; 32],
    direction: u8,
    counter: u64,
    closed: bool,
}

impl RecordSender {
    pub fn new(key: &[u8; 32], direction: u8) -> Self {
        Self {
            key: *key,
            direction,
            counter: 0,
            closed: false,
        }
    }

    pub fn seal(&mut self, body: &[u8]) -> Result<Vec<u8>, NetError> {
        if self.closed {
            return Err(NetError::Local {
                reason: NetErrorReason::Auth,
                detail: "sender closed".into(),
            });
        }
        let res = (|| {
            Frame::decode_body(body)?;
            let length = body.len() + TAG_LEN;
            let (nonce, aad) = record_parameters(self.direction, self.counter, length)?;
            let sealed = aes256_gcm_seal(&self.key, &nonce, body, &aad)?;
            let mut wire = Vec::with_capacity(2 + sealed.len());
            wire.extend_from_slice(&(length as u16).to_be_bytes());
            wire.extend_from_slice(&sealed);
            self.counter += 1;
            Ok(wire)
        })();
        if res.is_err() {
            self.close();
        }
        res
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.key.zeroize();
    }
}

impl Drop for RecordSender {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct RecordReceiver {
    key: [u8; 32],
    direction: u8,
    counter: u64,
    closed: bool,
}

impl RecordReceiver {
    pub fn new(key: &[u8; 32], direction: u8) -> Self {
        Self {
            key: *key,
            direction,
            counter: 0,
            closed: false,
        }
    }

    pub fn open(&mut self, wire: &[u8]) -> Result<Vec<u8>, NetError> {
        if self.closed {
            return Err(NetError::Local {
                reason: NetErrorReason::Auth,
                detail: "receiver closed".into(),
            });
        }
        let res = (|| {
            if wire.len() < 2 + MIN_RECORD_BODY_LEN || wire.len() > 2 + MAX_RECORD_BODY_LEN {
                return Err(NetError::Local {
                    reason: NetErrorReason::Args,
                    detail: "invalid record size".into(),
                });
            }
            let length = u16::from_be_bytes([wire[0], wire[1]]) as usize;
            if length != wire.len() - 2 {
                return Err(NetError::Local {
                    reason: NetErrorReason::Args,
                    detail: "invalid record prefix".into(),
                });
            }
            let (nonce, aad) = record_parameters(self.direction, self.counter, length)?;
            let body = aes256_gcm_open(&self.key, &nonce, &wire[2..], &aad)?;
            Frame::decode_body(&body)?;
            self.counter += 1;
            Ok(body)
        })();
        if res.is_err() {
            self.close();
        }
        res
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.key.zeroize();
    }
}

impl Drop for RecordReceiver {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct WireSocket {
    pub stream: TcpStream,
    buf: Vec<u8>,
}

impl WireSocket {
    pub fn new(stream: TcpStream) -> Self {
        let _ = stream.set_nodelay(true);
        Self {
            stream,
            buf: Vec::with_capacity(1024),
        }
    }

    pub fn send(&mut self, wire: &[u8], timeout: Duration) -> Result<(), NetError> {
        let _ = self.stream.set_write_timeout(Some(timeout));
        self.stream.write_all(wire)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn read_item(
        &mut self,
        min_body: usize,
        max_body: usize,
        timeout: Duration,
    ) -> Result<Vec<u8>, NetError> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.buf.len() >= 2 {
                let size = u16::from_be_bytes([self.buf[0], self.buf[1]]) as usize;
                if size < min_body || size > max_body {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    return Err(NetError::Local {
                        reason: NetErrorReason::Args,
                        detail: format!("invalid item length {size}"),
                    });
                }
                if self.buf.len() >= 2 + size {
                    let item = self.buf[..2 + size].to_vec();
                    self.buf.drain(..2 + size);
                    return Ok(item);
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(NetError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline expired",
                )));
            }
            let rem = (deadline - now).max(Duration::from_millis(5));
            let _ = self.stream.set_read_timeout(Some(rem));
            let mut chunk = [0u8; 4096];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(NetError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "connection ended inside item",
                    )));
                }
                Ok(n) => {
                    self.buf.extend_from_slice(&chunk[..n]);
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) =>
                {
                    if Instant::now() >= deadline {
                        return Err(NetError::Io(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "deadline expired",
                        )));
                    }
                }
                Err(err) => return Err(NetError::Io(err)),
            }
        }
    }

    pub fn receive_handshake(
        &mut self,
        expected_kind: u8,
        timeout: Duration,
    ) -> Result<(Vec<u8>, Vec<u8>), NetError> {
        let item = self.read_item(3, MAX_HANDSHAKE_BODY_LEN, timeout)?;
        let (kind, fields) = parse_handshake(&item, None)?;
        if kind == KIND_NET_ERROR {
            let reason = NetErrorReason::from_u8(fields[0]).unwrap_or(NetErrorReason::Args);
            return Err(NetError::Remote(reason));
        }
        if kind != expected_kind {
            return Err(NetError::Local {
                reason: NetErrorReason::Args,
                detail: format!(
                    "expected handshake kind 0x{expected_kind:02x}, got 0x{kind:02x}"
                ),
            });
        }
        let fields = fields.to_vec();
        Ok((item, fields))
    }

    pub fn finish(&mut self, drain_timeout: Duration) {
        let _ = self.stream.shutdown(Shutdown::Write);
        self.buf.clear();
        let deadline = Instant::now() + drain_timeout;
        let mut scratch = [0u8; 512];
        while Instant::now() < deadline {
            let rem = (deadline - Instant::now()).max(Duration::from_millis(5));
            let _ = self.stream.set_read_timeout(Some(rem));
            match self.stream.read(&mut scratch) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

pub struct PairSession {
    pub sock: WireSocket,
    pub app_id: [u8; APP_ID_LEN],
    pub app_name: String,
    pub request_wire: Vec<u8>,
    pub offer_wire: Vec<u8>,
    pub deck_id: [u8; DECK_ID_LEN],
    pub window_id: [u8; WINDOW_ID_LEN],
    pub remaining_ms: u32,
    pub window_deadline: Instant,
}

#[derive(Debug, Clone)]
pub struct PairCredential {
    pub deck_id: [u8; DECK_ID_LEN],
    pub app_id: [u8; APP_ID_LEN],
    pub app_name: String,
    pub ltk: [u8; LTK_LEN],
}

impl PairSession {
    pub fn open(
        addr: SocketAddr,
        app_id: &[u8; APP_ID_LEN],
        app_name: &str,
        timeout: Duration,
    ) -> Result<Self, NetError> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        Self::from_stream(stream, app_id, app_name, timeout)
    }

    pub fn from_stream(
        stream: TcpStream,
        app_id: &[u8; APP_ID_LEN],
        app_name: &str,
        timeout: Duration,
    ) -> Result<Self, NetError> {
        validate_app_name(app_name)?;
        let mut sock = WireSocket::new(stream);
        let name_bytes = app_name.as_bytes();
        let mut req_fields = Vec::with_capacity(17 + name_bytes.len());
        req_fields.extend_from_slice(app_id);
        req_fields.push(name_bytes.len() as u8);
        req_fields.extend_from_slice(name_bytes);
        let request_wire = encode_handshake(KIND_PAIR_REQUEST, &req_fields)?;
        sock.send(&request_wire, timeout)?;

        let (offer_wire, fields) = sock.receive_handshake(KIND_PAIR_OFFER, timeout)?;
        let mut deck_id = [0u8; DECK_ID_LEN];
        deck_id.copy_from_slice(&fields[0..6]);
        let mut window_id = [0u8; WINDOW_ID_LEN];
        window_id.copy_from_slice(&fields[6..22]);
        let remaining_ms = u32::from_be_bytes([fields[22], fields[23], fields[24], fields[25]]);
        let window_deadline = Instant::now() + Duration::from_millis(u64::from(remaining_ms));

        Ok(Self {
            sock,
            app_id: *app_id,
            app_name: app_name.to_owned(),
            request_wire,
            offer_wire,
            deck_id,
            window_id,
            remaining_ms,
            window_deadline,
        })
    }

    /// Submit one 6-digit code attempt on this pairing session.
    /// Note: per `net-v1.md`, the first two `auth` errors on `pair.proof` keep the socket
    /// open and return `Err(NetError::Remote(NetErrorReason::Auth))` so the user can submit
    /// another code on the same `PairSession` without opening a new window.
    pub fn try_code<F>(
        &mut self,
        code: &str,
        mut save_pending: F,
    ) -> Result<PairCredential, NetError>
    where
        F: FnMut(&PairCredential) -> Result<(), NetError>,
    {
        validate_code(code)?;
        let now = Instant::now();
        if now >= self.window_deadline {
            return Err(NetError::Remote(NetErrorReason::Expired));
        }
        let timeout = (self.window_deadline - now).min(HANDSHAKE_TIMEOUT);

        let mut a_priv = [0u8; 32];
        loop {
            getrandom::getrandom(&mut a_priv).expect("OS CSPRNG");
            if a_priv.iter().any(|&b| b != 0) {
                break;
            }
        }
        let mut client_nonce = [0u8; NONCE32_LEN];
        getrandom::getrandom(&mut client_nonce).expect("OS CSPRNG");

        let a_pub = srp_client_public(&a_priv)?;
        let mut start_fields = Vec::with_capacity(SRP_PAD_LEN + NONCE32_LEN);
        start_fields.extend_from_slice(&a_pub);
        start_fields.extend_from_slice(&client_nonce);
        let start_wire = encode_handshake(KIND_PAIR_START, &start_fields)?;
        self.sock.send(&start_wire, timeout)?;

        let (challenge_wire, ch_fields) =
            self.sock.receive_handshake(KIND_PAIR_CHALLENGE, timeout)?;
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&ch_fields[0..16]);
        let mut b_pub = [0u8; SRP_PAD_LEN];
        b_pub.copy_from_slice(&ch_fields[16..400]);

        let ident = identity_bytes(&self.deck_id, &self.app_id);
        let mut secret = srp_client_secret(&ident, code, &salt, &a_priv, &b_pub)?;
        a_priv.zeroize();

        let keys = derive_pair_keys(
            &secret.k_srp,
            &self.request_wire,
            &self.offer_wire,
            &start_wire,
            &challenge_wire,
        )?;
        secret.k_srp.zeroize();

        let proof_wire = encode_handshake(KIND_PAIR_PROOF, &keys.client_proof)?;
        self.sock.send(&proof_wire, timeout)?;

        let (accept_wire, _) = self.sock.receive_handshake(KIND_PAIR_ACCEPT, timeout)?;
        let ltk = accept_key(&keys, &accept_wire)?;

        let cred = PairCredential {
            deck_id: self.deck_id,
            app_id: self.app_id,
            app_name: self.app_name.clone(),
            ltk,
        };
        save_pending(&cred)?;

        let commit = commit_proof(&keys, &ltk);
        let commit_wire = encode_handshake(KIND_PAIR_COMMIT, &commit)?;
        self.sock.send(&commit_wire, timeout)?;

        let (_, stored_fields) = self.sock.receive_handshake(KIND_PAIR_STORED, timeout)?;
        let expected_stored = stored_proof(&keys, &ltk);
        if stored_fields.ct_eq(&expected_stored).unwrap_u8() != 1 {
            return Err(NetError::Local {
                reason: NetErrorReason::Auth,
                detail: "bad stored proof".into(),
            });
        }
        Ok(cred)
    }
}

pub struct NetTransport {
    pub sock: WireSocket,
    pub sender: RecordSender,
    pub receiver: RecordReceiver,
    pub deck_id: [u8; DECK_ID_LEN],
}

impl NetTransport {
    pub fn connect(
        addr: SocketAddr,
        deck_id: &[u8; DECK_ID_LEN],
        app_id: &[u8; APP_ID_LEN],
        ltk: &[u8; LTK_LEN],
        timeout: Duration,
    ) -> Result<Self, NetError> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        Self::from_stream(stream, deck_id, app_id, ltk, timeout)
    }

    pub fn from_stream(
        stream: TcpStream,
        deck_id: &[u8; DECK_ID_LEN],
        app_id: &[u8; APP_ID_LEN],
        ltk: &[u8; LTK_LEN],
        timeout: Duration,
    ) -> Result<Self, NetError> {
        let mut sock = WireSocket::new(stream);
        let mut client_nonce = [0u8; NONCE32_LEN];
        getrandom::getrandom(&mut client_nonce).expect("OS CSPRNG");

        let mut open_fields = [0u8; 54];
        open_fields[0..6].copy_from_slice(deck_id);
        open_fields[6..22].copy_from_slice(app_id);
        open_fields[22..54].copy_from_slice(&client_nonce);
        let open_wire = encode_handshake(KIND_SESSION_OPEN, &open_fields)?;
        sock.send(&open_wire, timeout)?;

        let (_, ch_fields) = sock.receive_handshake(KIND_SESSION_CHALLENGE, timeout)?;
        let mut deck_nonce = [0u8; NONCE32_LEN];
        deck_nonce.copy_from_slice(&ch_fields[0..32]);

        let keys = derive_session_keys(ltk, deck_id, app_id, &client_nonce, &deck_nonce);
        if ch_fields[32..64].ct_eq(&keys.deck_proof).unwrap_u8() != 1 {
            return Err(NetError::Local {
                reason: NetErrorReason::Auth,
                detail: "bad session deck proof".into(),
            });
        }

        let proof_wire = encode_handshake(KIND_SESSION_PROOF, &keys.client_proof)?;
        sock.send(&proof_wire, timeout)?;

        let (_, ready_fields) = sock.receive_handshake(KIND_SESSION_READY, timeout)?;
        if ready_fields.ct_eq(&keys.ready_proof).unwrap_u8() != 1 {
            return Err(NetError::Local {
                reason: NetErrorReason::Auth,
                detail: "bad session ready proof".into(),
            });
        }

        let sender = RecordSender::new(&keys.c2d, DIR_C2D);
        let receiver = RecordReceiver::new(&keys.d2c, DIR_D2C);
        Ok(Self {
            sock,
            sender,
            receiver,
            deck_id: *deck_id,
        })
    }

    pub fn write_frame(&mut self, frame: &Frame) -> Result<(), LinkError> {
        let body = frame
            .encode_body()
            .map_err(|e| LinkError::Io(e.to_string()))?;
        let wire = self
            .sender
            .seal(&body)
            .map_err(|e| LinkError::Io(e.to_string()))?;
        self.sock
            .send(&wire, Duration::from_secs(5))
            .map_err(|e| LinkError::Io(e.to_string()))
    }

    pub fn read_frame(&mut self, timeout: Duration) -> Result<Option<Frame>, LinkError> {
        match self
            .sock
            .read_item(MIN_RECORD_BODY_LEN, MAX_RECORD_BODY_LEN, timeout)
        {
            Ok(wire) => {
                let body = self
                    .receiver
                    .open(&wire)
                    .map_err(|e| LinkError::Io(e.to_string()))?;
                let frame = Frame::decode_body(&body).map_err(|e| LinkError::Io(e.to_string()))?;
                Ok(Some(frame))
            }
            Err(NetError::Io(err))
                if matches!(
                    err.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                Ok(None)
            }
            Err(err) => Err(LinkError::Io(err.to_string())),
        }
    }
}

struct NetReader {
    sock: WireSocket,
    receiver: RecordReceiver,
    slip_buf: Vec<u8>,
}

impl Drop for NetReader {
    fn drop(&mut self) {
        let _ = self.sock.stream.shutdown(Shutdown::Both);
    }
}

impl Read for NetReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.slip_buf.is_empty() {
            match self
                .sock
                .read_item(MIN_RECORD_BODY_LEN, MAX_RECORD_BODY_LEN, READ_TICK)
            {
                Ok(wire) => {
                    let body = self.receiver.open(&wire).map_err(|e| {
                        let _ = self.sock.stream.shutdown(Shutdown::Both);
                        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                    })?;
                    let frame = Frame::decode_body(&body).map_err(|e| {
                        let _ = self.sock.stream.shutdown(Shutdown::Both);
                        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                    })?;
                    let slip = frame.encode().map_err(|e| {
                        let _ = self.sock.stream.shutdown(Shutdown::Both);
                        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                    })?;
                    self.slip_buf.extend_from_slice(&slip);
                }
                Err(NetError::Io(err)) if err.kind() == io::ErrorKind::UnexpectedEof => {
                    return Ok(0);
                }
                Err(NetError::Io(err)) => return Err(err),
                Err(err) => {
                    let _ = self.sock.stream.shutdown(Shutdown::Both);
                    return Err(io::Error::new(io::ErrorKind::InvalidData, err.to_string()));
                }
            }
        }
        let n = out.len().min(self.slip_buf.len());
        out[..n].copy_from_slice(&self.slip_buf[..n]);
        self.slip_buf.drain(..n);
        Ok(n)
    }
}

struct NetWriter {
    stream: TcpStream,
    sender: RecordSender,
    decoder: StreamDecoder,
}

impl Write for NetWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        for item in self.decoder.feed(buf, Instant::now()) {
            if let Item::Frame(frame) = item {
                let body = frame.encode_body().map_err(|e| {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                })?;
                let wire = self.sender.seal(&body).map_err(|e| {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                })?;
                self.stream.write_all(&wire)?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

impl Transport for NetTransport {
    fn split(self: Box<Self>) -> io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        let writer_stream = self.sock.stream.try_clone()?;
        let reader = NetReader {
            sock: self.sock,
            receiver: self.receiver,
            slip_buf: Vec::new(),
        };
        let writer = NetWriter {
            stream: writer_stream,
            sender: self.sender,
            decoder: StreamDecoder::default(),
        };
        Ok((Box::new(reader), Box::new(writer)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn load_vectors() -> serde_json::Value {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("schema")
            .join("protocol")
            .join("net-v1-vectors.json");
        let text = std::fs::read_to_string(&root)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", root.display()));
        serde_json::from_str(&text).expect("parse net-v1-vectors.json")
    }

    fn unhex(s: &str) -> Vec<u8> {
        hex::decode(s).expect("valid hex")
    }

    fn unhex_arr<const N: usize>(s: &str) -> [u8; N] {
        let v = unhex(s);
        assert_eq!(v.len(), N);
        let mut out = [0u8; N];
        out.copy_from_slice(&v);
        out
    }

    #[test]
    fn matches_all_net_v1_vectors() {
        let v = load_vectors();
        let inputs = &v["inputs"];
        let pake = &v["pake"];

        let deck_id: [u8; 6] = unhex_arr(inputs["deckId"].as_str().unwrap());
        let app_id: [u8; 16] = unhex_arr(inputs["appId"].as_str().unwrap());
        let salt: [u8; 16] = unhex_arr(inputs["salt"].as_str().unwrap());
        let a_priv: [u8; 32] = unhex_arr(inputs["a"].as_str().unwrap());
        let b_priv: [u8; 32] = unhex_arr(inputs["b"].as_str().unwrap());
        let window_id: [u8; 16] = unhex_arr(inputs["windowId"].as_str().unwrap());
        let client_nonce: [u8; 32] = unhex_arr(inputs["clientNonce"].as_str().unwrap());
        let deck_nonce: [u8; 32] = unhex_arr(inputs["deckNonce"].as_str().unwrap());
        let ltk: [u8; 32] = unhex_arr(inputs["ltk"].as_str().unwrap());
        let sess_client_nonce: [u8; 32] =
            unhex_arr(inputs["sessionClientNonce"].as_str().unwrap());
        let sess_deck_nonce: [u8; 32] = unhex_arr(inputs["sessionDeckNonce"].as_str().unwrap());
        let code = inputs["code"].as_str().unwrap();
        let app_name = inputs["appName"].as_str().unwrap();

        let ident = identity_bytes(&deck_id, &app_id);
        assert_eq!(
            std::str::from_utf8(&ident).unwrap(),
            pake["identity"].as_str().unwrap()
        );

        let k = srp_multiplier_k();
        assert_eq!(
            hex::encode(min_be(&k)),
            v["group"]["k"].as_str().unwrap()
        );

        let server_sec_init = srp_server_public(&ident, code, &salt, &b_priv).unwrap();
        assert_eq!(
            hex::encode(min_be(&server_sec_init.x)),
            pake["x"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(pad_384(&server_sec_init.v)),
            pake["v"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(server_sec_init.b_pub),
            pake["B"].as_str().unwrap()
        );

        let client_sec =
            srp_client_secret(&ident, code, &salt, &a_priv, &server_sec_init.b_pub).unwrap();
        let server_sec =
            srp_server_secret(&ident, code, &salt, &b_priv, &client_sec.a_pub).unwrap();
        assert_eq!(hex::encode(client_sec.a_pub), pake["A"].as_str().unwrap());
        assert_eq!(
            hex::encode(min_be(&client_sec.u)),
            pake["u"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(pad_384(&client_sec.s)),
            pake["S"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(pad_384(&server_sec.s)),
            pake["server_S"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(client_sec.k_srp),
            pake["K_srp"].as_str().unwrap()
        );
        assert_eq!(client_sec.k_srp, server_sec.k_srp);

        // Leading-zero case
        let lz = &v["leading_zero_case"];
        let lz_a: [u8; 32] = unhex_arr(lz["a"].as_str().unwrap());
        let lz_salt: [u8; 16] = unhex_arr(lz["salt"].as_str().unwrap());
        let lz_srv = srp_server_public(&ident, code, &lz_salt, &b_priv).unwrap();
        assert_eq!(hex::encode(lz_srv.b_pub), lz["B"].as_str().unwrap());
        let lz_cli = srp_client_secret(&ident, code, &lz_salt, &lz_a, &lz_srv.b_pub).unwrap();
        assert_eq!(lz_cli.a_pub[0], 0);
        assert_eq!(hex::encode(lz_cli.a_pub), lz["A"].as_str().unwrap());
        assert_eq!(hex::encode(min_be(&lz_cli.u)), lz["u"].as_str().unwrap());
        assert_eq!(hex::encode(pad_384(&lz_cli.s)), lz["S"].as_str().unwrap());
        assert_eq!(hex::encode(lz_cli.k_srp), lz["K_srp"].as_str().unwrap());

        // Pairing wire & keys
        let pairing = &v["pairing"];
        let pwire = &pairing["wire"];
        let mut req_fields = Vec::new();
        req_fields.extend_from_slice(&app_id);
        req_fields.push(app_name.len() as u8);
        req_fields.extend_from_slice(app_name.as_bytes());
        let req_wire = encode_handshake(KIND_PAIR_REQUEST, &req_fields).unwrap();
        assert_eq!(hex::encode(&req_wire), pwire["request"].as_str().unwrap());

        let mut offer_fields = Vec::new();
        offer_fields.extend_from_slice(&deck_id);
        offer_fields.extend_from_slice(&window_id);
        offer_fields.extend_from_slice(&120_000u32.to_be_bytes());
        let offer_wire = encode_handshake(KIND_PAIR_OFFER, &offer_fields).unwrap();
        assert_eq!(hex::encode(&offer_wire), pwire["offer"].as_str().unwrap());

        let mut start_fields = Vec::new();
        start_fields.extend_from_slice(&client_sec.a_pub);
        start_fields.extend_from_slice(&client_nonce);
        let start_wire = encode_handshake(KIND_PAIR_START, &start_fields).unwrap();
        assert_eq!(hex::encode(&start_wire), pwire["start"].as_str().unwrap());

        let mut ch_fields = Vec::new();
        ch_fields.extend_from_slice(&salt);
        ch_fields.extend_from_slice(&server_sec.b_pub);
        ch_fields.extend_from_slice(&deck_nonce);
        let ch_wire = encode_handshake(KIND_PAIR_CHALLENGE, &ch_fields).unwrap();
        assert_eq!(hex::encode(&ch_wire), pwire["challenge"].as_str().unwrap());

        let pkeys = derive_pair_keys(
            &client_sec.k_srp,
            &req_wire,
            &offer_wire,
            &start_wire,
            &ch_wire,
        )
        .unwrap();
        assert_eq!(
            hex::encode(pkeys.transcript),
            pairing["transcript"].as_str().unwrap()
        );
        assert_eq!(hex::encode(pkeys.prk), pairing["prk"].as_str().unwrap());
        assert_eq!(
            hex::encode(pkeys.client),
            pairing["client"].as_str().unwrap()
        );
        assert_eq!(hex::encode(pkeys.deck), pairing["deck"].as_str().unwrap());
        assert_eq!(hex::encode(pkeys.wrap), pairing["wrap"].as_str().unwrap());
        assert_eq!(
            hex::encode(pkeys.commit),
            pairing["commit"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(pkeys.client_proof),
            pairing["clientProof"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(pkeys.deck_proof),
            pairing["deckProof"].as_str().unwrap()
        );

        let wrapped = wrap_key(&pkeys, &ltk).unwrap();
        assert_eq!(
            hex::encode(wrapped),
            pairing["wrapped_key"].as_str().unwrap()
        );

        let mut accept_fields = Vec::new();
        accept_fields.extend_from_slice(&pkeys.deck_proof);
        accept_fields.extend_from_slice(&wrapped);
        let accept_wire = encode_handshake(KIND_PAIR_ACCEPT, &accept_fields).unwrap();
        assert_eq!(hex::encode(&accept_wire), pwire["accept"].as_str().unwrap());
        assert_eq!(accept_key(&pkeys, &accept_wire).unwrap(), ltk);

        let commit_wire =
            encode_handshake(KIND_PAIR_COMMIT, &commit_proof(&pkeys, &ltk)).unwrap();
        assert_eq!(hex::encode(&commit_wire), pwire["commit"].as_str().unwrap());

        let stored_wire =
            encode_handshake(KIND_PAIR_STORED, &stored_proof(&pkeys, &ltk)).unwrap();
        assert_eq!(hex::encode(&stored_wire), pwire["stored"].as_str().unwrap());

        // Session keys & wire
        let sess = &v["session"];
        let skeys =
            derive_session_keys(&ltk, &deck_id, &app_id, &sess_client_nonce, &sess_deck_nonce);
        assert_eq!(
            hex::encode(skeys.transcript),
            sess["transcript"].as_str().unwrap()
        );
        assert_eq!(hex::encode(skeys.prk), sess["prk"].as_str().unwrap());
        assert_eq!(hex::encode(skeys.deck), sess["deck"].as_str().unwrap());
        assert_eq!(hex::encode(skeys.client), sess["client"].as_str().unwrap());
        assert_eq!(hex::encode(skeys.ready), sess["ready"].as_str().unwrap());
        assert_eq!(hex::encode(skeys.c2d), sess["c2d"].as_str().unwrap());
        assert_eq!(hex::encode(skeys.d2c), sess["d2c"].as_str().unwrap());
        assert_eq!(
            hex::encode(skeys.deck_proof),
            sess["deckProof"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(skeys.client_proof),
            sess["clientProof"].as_str().unwrap()
        );
        assert_eq!(
            hex::encode(skeys.ready_proof),
            sess["readyProof"].as_str().unwrap()
        );

        // Records
        let mut c2d_send = RecordSender::new(&skeys.c2d, DIR_C2D);
        let mut c2d_recv = RecordReceiver::new(&skeys.c2d, DIR_C2D);
        let mut d2c_send = RecordSender::new(&skeys.d2c, DIR_D2C);
        let mut d2c_recv = RecordReceiver::new(&skeys.d2c, DIR_D2C);

        for rec in v["records"].as_array().unwrap() {
            let dir = rec["direction"].as_str().unwrap();
            let pt = unhex(rec["plaintext"].as_str().unwrap());
            let expected_wire = unhex(rec["wire"].as_str().unwrap());
            if dir == "c2d" {
                let wire = c2d_send.seal(&pt).unwrap();
                assert_eq!(wire, expected_wire);
                assert_eq!(c2d_recv.open(&wire).unwrap(), pt);
            } else {
                let wire = d2c_send.seal(&pt).unwrap();
                assert_eq!(wire, expected_wire);
                assert_eq!(d2c_recv.open(&wire).unwrap(), pt);
            }
        }

        // Tampered record
        let tampered_wire = unhex(v["tampered_record"]["wire"].as_str().unwrap());
        let mut fresh_recv = RecordReceiver::new(&skeys.c2d, DIR_C2D);
        assert!(fresh_recv.open(&tampered_wire).is_err());
    }

    #[test]
    fn rejects_invalid_public_ephemeral_and_replayed_records() {
        let zero = [0u8; SRP_PAD_LEN];
        assert!(validate_public_value(&zero).is_err());

        let n_pad = pad_384(&G_3072.n);
        assert!(validate_public_value(&n_pad).is_err());

        let key = [0x11u8; 32];
        let mut sender = RecordSender::new(&key, DIR_C2D);
        let mut receiver = RecordReceiver::new(&key, DIR_C2D);

        let frame0 = Frame::message(td_core::Role::Request(1), b"{}".to_vec())
            .encode_body()
            .unwrap();
        let frame1 = Frame::message(td_core::Role::Request(2), b"{}".to_vec())
            .encode_body()
            .unwrap();
        let r0 = sender.seal(&frame0).unwrap();
        let r1 = sender.seal(&frame1).unwrap();

        // Out-of-order (delivering r1 when counter is 0) must fail and close the receiver
        assert!(receiver.open(&r1).is_err());
        assert!(receiver.open(&r0).is_err());

        // On a fresh receiver, r0 succeeds, then replay of r0 fails and closes receiver
        let mut receiver2 = RecordReceiver::new(&key, DIR_C2D);
        assert_eq!(receiver2.open(&r0).unwrap(), frame0);
        assert!(receiver2.open(&r0).is_err());
    }

    #[test]
    fn mock_deck_net_v1_pairs_transfers_events_busy_and_reconnects() {
        use crate::mock::{MOCK_DECK_ID, MockDeck};
        use crate::xfer::{get_layout, put_layout};
        use crate::{Incoming, Session, SessionConfig};
        use td_core::{Layout, Orientation};

        let deck = MockDeck::new();
        let addr = deck.net_addr();
        let app_id = [0x42u8; 16];
        let timeout = Duration::from_secs(5);

        // Open pairing session
        let mut pair = PairSession::open(addr, &app_id, "Rust test app", timeout).unwrap();
        assert_eq!(pair.deck_id, MOCK_DECK_ID);

        // Wrong code #1 -> Remote(Auth), same session stays open!
        let mut saved = false;
        let err1 = pair
            .try_code("000000", |_| {
                saved = true;
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(err1, NetError::Remote(NetErrorReason::Auth)));
        assert!(!saved);

        // Correct code on retry #2 -> succeeds and stores credential!
        let right_code = deck.net_pairing_code();
        let cred = pair
            .try_code(&right_code, |_| {
                saved = true;
                Ok(())
            })
            .unwrap();
        assert!(saved);
        assert_eq!(cred.deck_id, MOCK_DECK_ID);
        assert_eq!(deck.net_paired_count(), 1);
        drop(pair);

        // Wait briefly for owner guard on pairing socket to release
        std::thread::sleep(Duration::from_millis(50));

        // Open authenticated NetTransport + unified Session
        let net = NetTransport::connect(addr, &cred.deck_id, &app_id, &cred.ltk, timeout).unwrap();
        let (session, events) = Session::open(
            Box::new(net),
            SessionConfig {
                ping_every: None,
                ping_timeout: Duration::from_secs(2),
            },
        )
        .unwrap();

        let hello = session.hello(timeout).unwrap();
        assert!(hello.is_deck());

        // Second TCP connection while session is active must be refused with Busy
        let busy_err =
            match NetTransport::connect(addr, &cred.deck_id, &app_id, &cred.ltk, timeout) {
                Ok(_) => panic!("expected busy error on second connection"),
                Err(err) => err,
            };
        assert!(matches!(busy_err, NetError::Remote(NetErrorReason::Busy)));

        // Push & pull a layout over Wi-Fi Session
        let builtin =
            Layout::parse(include_bytes!("../../../../schema/examples/builtin.json")).unwrap();
        put_layout(&session, Orientation::Landscape, &builtin).unwrap();
        let pulled = get_layout(&session, Orientation::Landscape).unwrap();
        assert_eq!(Layout::parse(&pulled.data).unwrap().name, builtin.name);

        // Emit a press event and receive it on the Wi-Fi Session
        deck.emit_press("main", "mute", "tap");
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut got_press = false;
        while Instant::now() < deadline {
            if let Ok(Incoming::Event(ev)) = events.recv_timeout(Duration::from_millis(100)) {
                if ev.get("m").and_then(|v| v.as_str()) == Some("press")
                    && ev.get("tile").and_then(|v| v.as_str()) == Some("mute")
                {
                    got_press = true;
                    break;
                }
            }
        }
        assert!(got_press, "expected press event over Wi-Fi");

        // Check net.state over Wi-Fi
        let state_reply = session
            .call(&serde_json::json!({"m": "net.state"}), timeout)
            .unwrap();
        assert_eq!(state_reply.fields["paired"], 1);
        assert_eq!(state_reply.fields["session"]["app_name"], "Rust test app");

        // Close session and reconnect without pairing again
        session.close();
        std::thread::sleep(Duration::from_millis(50));

        let net2 = NetTransport::connect(addr, &cred.deck_id, &app_id, &cred.ltk, timeout).unwrap();
        let (session2, _events2) = Session::open(Box::new(net2), SessionConfig::default()).unwrap();
        assert!(session2.hello(timeout).unwrap().is_deck());

        // net.forget clears pairings and closes the Wi-Fi session
        let forget_reply = session2
            .call(&serde_json::json!({"m": "net.forget"}), timeout)
            .unwrap();
        assert_eq!(forget_reply.fields["paired"], 0);
        session2.close();
        assert_eq!(deck.net_paired_count(), 0);
    }

    #[test]
    fn node_net_ref_client_interoperates_with_rust_mock_deck() {
        use crate::mock::MockDeck;
        use std::process::Command;

        let deck = MockDeck::new();
        let addr = deck.net_addr();
        let code = deck.net_pairing_code();

        let ref_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("schema")
            .join("protocol")
            .join("net-ref");

        let script = r#"
import assert from 'node:assert/strict';
import { pairClient, connectSession } from './client.mjs';

const host = process.env.TD_HOST;
const port = Number(process.env.TD_PORT);
const code = process.env.TD_CODE;

const cred = await pairClient({
  host,
  port,
  appName: 'Node interop test',
  getCode: async () => code,
});
assert.equal(cred.ltk.length, 32);

const session = await connectSession({ host, port, credential: cred });
const pong = await session.request({ m: 'ping', n: 7 });
assert.equal(pong.ok, true);
const netState = await session.request({ m: 'net.state' });
assert.equal(netState.ok, true);
assert.equal(netState.paired, 1);
session.close();
"#;

        let output = Command::new("node")
            .current_dir(&ref_dir)
            .args(["--input-type=module", "-e", script])
            .env("TD_HOST", addr.ip().to_string())
            .env("TD_PORT", addr.port().to_string())
            .env("TD_CODE", &code)
            .output()
            .expect("run node net-ref interop");

        assert!(
            output.status.success(),
            "node net-ref interop failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(deck.net_paired_count(), 1);
    }
}