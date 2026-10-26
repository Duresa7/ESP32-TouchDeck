//! Layout transfers over the link, in M2-05's shapes and Appendix G item G1's limits.
//!
//! `layout.put {orientation, bytes, crc32}` and `layout.get {orientation}` each open a
//! transfer, and the reply names it with `xfer`. The sender writes chunks of up to 1021 bytes,
//! at most 4 ahead of the last ack. Each chunk is a request frame with a fresh `seq`, and the
//! receiver answers it with an ack frame that echoes that `seq` and names the next index it
//! wants. A chunk without an ack after 200 ms goes again, 5 times in all, and then the
//! transfer aborts. `xfer.end {xfer}` closes it: after a put the deck checks the CRC-32,
//! swaps the file in, and replies with its `layout state` fields. After a get the app checks
//! the CRC-32 itself.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use td_core::frame::CHUNK_DATA_MAX;
use td_core::layout::UPLOAD_MAX;
use td_core::{Layout, Orientation, ReplyError};

use crate::session::{LinkError, Session, Transfer, XferFrame};

pub const WINDOW: u16 = 4;
pub const RETRY_AFTER: Duration = Duration::from_millis(200);
pub const TRANSMISSIONS: u32 = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const END_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a receiver waits for the next chunk before it gives up on the sender.
const IDLE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XferError {
    Link(LinkError),
    TooLarge(usize),
    Aborted { idx: u16 },
    Stalled { received: usize, bytes: usize },
    Crc { expected: u32, got: u32 },
    Size { expected: usize, got: usize },
}

impl fmt::Display for XferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XferError::Link(error) => error.fmt(f),
            XferError::TooLarge(bytes) => write!(
                f,
                "layout is {bytes} bytes; the deck takes at most {UPLOAD_MAX}"
            ),
            XferError::Aborted { idx } => write!(
                f,
                "chunk {idx} went {TRANSMISSIONS} times without an ack; transfer aborted"
            ),
            XferError::Stalled { received, bytes } => write!(
                f,
                "the deck stopped sending after {received} of {bytes} bytes"
            ),
            XferError::Crc { expected, got } => {
                write!(f, "CRC-32 is {got:08x}, the deck said {expected:08x}")
            }
            XferError::Size { expected, got } => {
                write!(f, "received {got} bytes, the deck said {expected}")
            }
        }
    }
}

impl std::error::Error for XferError {}

impl From<LinkError> for XferError {
    fn from(error: LinkError) -> Self {
        XferError::Link(error)
    }
}

impl From<ReplyError> for XferError {
    fn from(error: ReplyError) -> Self {
        XferError::Link(LinkError::Reply(error))
    }
}

/// The deck's `layout state` fields, which `xfer.end` returns after a put.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutState {
    pub source: String,
    pub name: String,
    pub orientation: Orientation,
    pub pages: u32,
    pub tiles: u32,
    #[serde(default)]
    pub icons_loaded: Option<u32>,
    #[serde(default)]
    pub icons_missing: Option<u32>,
    #[serde(default)]
    pub bytes: Option<u32>,
    #[serde(default)]
    pub load_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PutReport {
    pub state: LayoutState,
    pub bytes: usize,
    pub chunks: u16,
    pub retransmits: u32,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GetReport {
    #[serde(skip)]
    pub data: Vec<u8>,
    pub bytes: usize,
    pub crc32: u32,
    pub elapsed_ms: u64,
}

#[derive(Deserialize)]
struct Opened {
    xfer: u8,
    #[serde(default)]
    bytes: Option<usize>,
    #[serde(default)]
    crc32: Option<u32>,
}

/// CRC-32 as zlib and Ethernet compute it: polynomial `0x04C11DB7` reflected, initial value
/// and final XOR `0xFFFFFFFF`.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn orientation_name(orientation: Orientation) -> &'static str {
    match orientation {
        Orientation::Landscape => "landscape",
        Orientation::Portrait => "portrait",
    }
}

fn chunk_count(bytes: usize) -> u16 {
    u16::try_from(bytes.div_ceil(CHUNK_DATA_MAX)).expect("an upload fits 65535 chunks")
}

/// Pushes a layout into the deck's slot for `orientation` in its canonical compact form.
pub fn put_layout(
    session: &Session,
    orientation: Orientation,
    layout: &Layout,
) -> Result<PutReport, XferError> {
    put_bytes(session, orientation, &layout.to_canonical_json())
}

/// Pushes raw layout bytes. The app sends only canonical layouts through [`put_layout`];
/// this path exists for tests and the CLI.
pub fn put_bytes(
    session: &Session,
    orientation: Orientation,
    data: &[u8],
) -> Result<PutReport, XferError> {
    if data.len() > UPLOAD_MAX {
        return Err(XferError::TooLarge(data.len()));
    }
    let started = Instant::now();
    let transfer = session.transfer()?;
    let request = json!({
        "m": "layout.put",
        "orientation": orientation_name(orientation),
        "bytes": data.len(),
        "crc32": crc32(data),
    });
    let opened: Opened = session.call(&request, REQUEST_TIMEOUT)?.fields()?;
    let (chunks, retransmits) = send_chunks(&transfer, opened.xfer, data)?;
    let state: LayoutState = session
        .call(&json!({"m": "xfer.end", "xfer": opened.xfer}), END_TIMEOUT)?
        .fields()?;
    Ok(PutReport {
        state,
        bytes: data.len(),
        chunks,
        retransmits,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn send_chunks(transfer: &Transfer, xfer: u8, data: &[u8]) -> Result<(u16, u32), XferError> {
    let count = chunk_count(data.len());
    let chunk = |idx: u16| {
        let start = usize::from(idx) * CHUNK_DATA_MAX;
        &data[start..(start + CHUNK_DATA_MAX).min(data.len())]
    };
    let (mut base, mut ahead) = (0u16, 0u16);
    let mut sent: HashMap<u16, (Instant, u32)> = HashMap::new();
    let mut retransmits = 0;
    while base < count {
        while ahead < count && ahead < base + WINDOW {
            transfer.send_chunk(xfer, ahead, chunk(ahead))?;
            sent.insert(ahead, (Instant::now(), 1));
            ahead += 1;
        }
        let due = (base..ahead)
            .filter_map(|idx| sent.get(&idx).map(|(at, _)| *at + RETRY_AFTER))
            .min()
            .expect("an unacked chunk is in flight");
        let wait = due.saturating_duration_since(Instant::now());
        match transfer.frames.recv_timeout(wait) {
            Ok(XferFrame::Ack { xfer: id, next }) if id == xfer => {
                if next > base {
                    base = next.min(ahead);
                    sent.retain(|idx, _| *idx >= base);
                }
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let now = Instant::now();
                for idx in base..ahead {
                    let Some((at, times)) = sent.get_mut(&idx) else {
                        continue;
                    };
                    if *at + RETRY_AFTER > now {
                        continue;
                    }
                    if *times >= TRANSMISSIONS {
                        return Err(XferError::Aborted { idx });
                    }
                    transfer.send_chunk(xfer, idx, chunk(idx))?;
                    *at = now;
                    *times += 1;
                    retransmits += 1;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(XferError::Link(LinkError::Closed(
                    "the transfer channel closed".into(),
                )));
            }
        }
    }
    Ok((count, retransmits))
}

/// Pulls the layout the deck stores for `orientation` and checks its size and CRC-32.
pub fn get_layout(session: &Session, orientation: Orientation) -> Result<GetReport, XferError> {
    let started = Instant::now();
    let transfer = session.transfer()?;
    let opened: Opened = session
        .call(
            &json!({"m": "layout.get", "orientation": orientation_name(orientation)}),
            REQUEST_TIMEOUT,
        )?
        .fields()?;
    let bytes = opened.bytes.ok_or_else(|| {
        XferError::from(ReplyError::Fields("layout.get reply has no bytes".into()))
    })?;
    let expected_crc = opened.crc32.ok_or_else(|| {
        XferError::from(ReplyError::Fields("layout.get reply has no crc32".into()))
    })?;
    if bytes > UPLOAD_MAX {
        return Err(XferError::TooLarge(bytes));
    }
    let count = chunk_count(bytes);
    let mut data = Vec::with_capacity(bytes);
    let mut next = 0u16;
    while next < count {
        let frame = transfer
            .frames
            .recv_timeout(IDLE_TIMEOUT)
            .map_err(|_| XferError::Stalled {
                received: data.len(),
                bytes,
            })?;
        let XferFrame::Chunk {
            seq,
            xfer,
            idx,
            data: chunk,
        } = frame
        else {
            continue;
        };
        if xfer != opened.xfer {
            continue;
        }
        if idx == next {
            if data.len() + chunk.len() > bytes {
                return Err(XferError::Size {
                    expected: bytes,
                    got: data.len() + chunk.len(),
                });
            }
            data.extend_from_slice(&chunk);
            next += 1;
        }
        transfer.send_ack(seq, xfer, next)?;
    }
    session
        .call(&json!({"m": "xfer.end", "xfer": opened.xfer}), END_TIMEOUT)?
        .fields::<Value>()?;
    if data.len() != bytes {
        return Err(XferError::Size {
            expected: bytes,
            got: data.len(),
        });
    }
    let got = crc32(&data);
    if got != expected_crc {
        return Err(XferError::Crc {
            expected: expected_crc,
            got,
        });
    }
    Ok(GetReport {
        data,
        bytes,
        crc32: got,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MockDeck, SessionConfig};

    fn dev() -> Layout {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../schema/examples/dev.json"),
        )
        .unwrap();
        Layout::parse(&bytes).unwrap()
    }

    fn open(deck: &MockDeck) -> Session {
        Session::open(
            Box::new(deck.connect()),
            SessionConfig {
                ping_every: None,
                ..SessionConfig::default()
            },
        )
        .unwrap()
        .0
    }

    fn counts(layout: &Layout) -> (u32, u32) {
        let tiles = layout
            .pages
            .iter()
            .map(|page| match page {
                td_core::layout::Page::Grid { tiles, .. } => tiles.len() as u32,
                td_core::layout::Page::Dashboard { .. } => 0,
            })
            .sum();
        (layout.pages.len() as u32, tiles)
    }

    #[test]
    fn crc32_matches_the_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn a_put_lands_in_order_and_replies_with_the_layout_state() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let layout = dev();
        let report = put_layout(&session, Orientation::Landscape, &layout).unwrap();
        let (pages, tiles) = counts(&layout);
        assert_eq!(
            (
                report.state.source.as_str(),
                report.state.name.as_str(),
                report.state.pages,
                report.state.tiles
            ),
            ("storage", layout.name.as_str(), pages, tiles)
        );
        assert_eq!(report.retransmits, 0);
        assert_eq!(report.chunks, chunk_count(layout.to_canonical_json().len()));
        assert_eq!(
            deck.layout(Orientation::Landscape).unwrap(),
            layout.to_canonical_json()
        );
        let put = deck
            .received()
            .into_iter()
            .find(|m| m["m"] == "layout.put")
            .unwrap();
        assert_eq!(put["crc32"], crc32(&layout.to_canonical_json()));
    }

    #[test]
    fn a_lost_chunk_goes_again() {
        let deck = MockDeck::new();
        deck.lose_chunks([1, 1, 3]);
        let session = open(&deck);
        let layout = dev();
        let report = put_layout(&session, Orientation::Landscape, &layout).unwrap();
        assert!(report.retransmits >= 2, "{report:?}");
        assert_eq!(
            deck.layout(Orientation::Landscape).unwrap(),
            layout.to_canonical_json()
        );
    }

    #[test]
    fn five_lost_transmissions_abort_the_put() {
        let deck = MockDeck::new();
        deck.lose_chunks([0; 5]);
        let session = open(&deck);
        assert_eq!(
            put_layout(&session, Orientation::Landscape, &dev()),
            Err(XferError::Aborted { idx: 0 })
        );
        assert_eq!(deck.layout(Orientation::Landscape), None);
        assert!(session.transfer().is_ok(), "the aborted transfer let go");
    }

    #[test]
    fn a_bad_crc_is_refused_and_keeps_the_old_layout() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let layout = dev();
        put_layout(&session, Orientation::Landscape, &layout).unwrap();
        deck.corrupt_next_put();
        let error = put_layout(&session, Orientation::Landscape, &layout).unwrap_err();
        assert!(error.to_string().contains("crc32"), "{error}");
        assert_eq!(
            deck.layout(Orientation::Landscape).unwrap(),
            layout.to_canonical_json()
        );
    }

    #[test]
    fn an_oversize_layout_never_leaves_the_app() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let data = vec![b' '; UPLOAD_MAX + 1];
        assert_eq!(
            put_bytes(&session, Orientation::Landscape, &data),
            Err(XferError::TooLarge(UPLOAD_MAX + 1))
        );
        assert!(deck.received().is_empty());
    }

    #[test]
    fn an_invalid_layout_is_refused_at_xfer_end() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let error = put_bytes(&session, Orientation::Landscape, b"{\"schema\":2}").unwrap_err();
        assert!(error.to_string().contains("shape."), "{error}");
        assert_eq!(deck.layout(Orientation::Landscape), None);
    }

    #[test]
    fn a_get_returns_the_bytes_the_put_stored() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let layout = dev();
        put_layout(&session, Orientation::Portrait, &layout).unwrap();
        let report = get_layout(&session, Orientation::Portrait).unwrap();
        assert_eq!(report.data, layout.to_canonical_json());
        assert_eq!(report.crc32, crc32(&report.data));
        assert!(
            get_layout(&session, Orientation::Landscape)
                .unwrap_err()
                .to_string()
                .contains("no stored layout")
        );
    }

    #[test]
    fn a_get_survives_lost_chunks() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let layout = dev();
        put_layout(&session, Orientation::Landscape, &layout).unwrap();
        deck.lose_sent_chunks([0, 2]);
        let report = get_layout(&session, Orientation::Landscape).unwrap();
        assert_eq!(report.data, layout.to_canonical_json());
    }

    #[test]
    fn a_32_kb_put_and_get_round_trip() {
        let deck = MockDeck::new();
        let session = open(&deck);
        let mut layout = dev();
        let pad = |layout: &mut Layout, text: String| {
            if let td_core::layout::Page::Grid { tiles, .. } = &mut layout.pages[1]
                && let td_core::layout::Action::Launch { cwd, .. } = &mut tiles[0].action
            {
                *cwd = text;
            }
        };
        pad(&mut layout, String::new());
        let base = layout.to_canonical_json().len();
        pad(&mut layout, "a".repeat(UPLOAD_MAX - base));
        let bytes = layout.to_canonical_json();
        assert_eq!(bytes.len(), UPLOAD_MAX);
        let put = put_layout(&session, Orientation::Landscape, &layout).unwrap();
        assert_eq!(put.chunks, 33);
        assert_eq!(
            get_layout(&session, Orientation::Landscape).unwrap().data,
            bytes
        );
    }
}
