//! Which Integration values each Device needs, and sending them over `v2.live.set`.
//!
//! The rules come from `schema/protocol/link-v2.md` § "Layout references and live values" and from
//! `docs/companion-api.md` § "Live tile bindings". The router is plain state: the caller hands it the Device's
//! session, its live caps and the newest Integration samples, so tests drive it with a mock Device.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::{Duration, Instant};

use td_core::layout::{Action, Page, StreamType};
use td_core::{Layout, Orientation};
use td_host::integrations::LiveSample;
use td_link::live::{self, LiveError, LiveReason, LiveSet, LiveValue};
use td_link::{Session, xfer};

/// How long a Device's `layout state` reply may take.
const LAYOUT_STATE_TIMEOUT: Duration = Duration::from_secs(2);
/// The shortest gap between two values of one stream. A source that samples once a second lands just under a
/// second apart, so the gap leaves it room.
const STREAM_GAP: Duration = Duration::from_millis(900);
/// How long the router waits to read a Device's active Layout again after a failed read.
const LAYOUT_RETRY: Duration = Duration::from_secs(5);
/// The longest wait after repeated `rate` refusals.
const BACKOFF_MAX: Duration = Duration::from_secs(8);
const BUILTIN: &[u8] = include_bytes!("../../../../../schema/examples/builtin.json");

/// One Integration stream: the Integration's id, the account it reads (empty for one-account Integrations) and the
/// stream id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamKey {
    pub integration: String,
    pub account: String,
    pub stream: String,
}

/// One Tile stream of a Device's active Layout and the Integration stream that feeds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub page: String,
    pub tile: String,
    pub stream: String,
    pub kind: StreamType,
    pub source: StreamKey,
}

/// The Tile streams a Layout binds to Integration streams. A Tile binds through its `widget` action: `source` names
/// the Integration and `account` the account. Its `value` stream reads the Integration stream `field`, and any other
/// stream reads the Integration stream of the same id.
pub fn bindings(layout: &Layout) -> Vec<Binding> {
    let mut out = Vec::new();
    for page in &layout.pages {
        let Page::Grid {
            id: page, tiles, ..
        } = page
        else {
            continue;
        };
        for tile in tiles {
            let Action::Widget {
                source,
                field,
                account,
            } = &tile.action
            else {
                continue;
            };
            for (stream, kind) in tile.streams.iter().flatten() {
                out.push(Binding {
                    page: page.clone(),
                    tile: tile.id.clone(),
                    stream: stream.clone(),
                    kind: *kind,
                    source: StreamKey {
                        integration: source.clone(),
                        account: account.clone(),
                        stream: if stream == "value" {
                            field.clone()
                        } else {
                            stream.clone()
                        },
                    },
                });
            }
        }
    }
    out
}

/// The live limits a Device advertised in `caps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveCaps {
    pub live_hz: u32,
    pub live_total_hz: u32,
    pub series: u32,
    pub text_bytes: u32,
}

impl LiveCaps {
    /// The caps of a Device that draws graph Tiles, or None.
    pub fn of(caps: &crate::devices::Capabilities) -> Option<LiveCaps> {
        if !caps.tile_kinds.iter().any(|kind| kind == "graph") {
            return None;
        }
        let caps = LiveCaps {
            live_hz: caps.live_hz,
            live_total_hz: caps.live_total_hz,
            series: caps.series,
            text_bytes: caps.text_bytes,
        };
        (caps.live_hz > 0 && caps.live_total_hz > 0).then_some(caps)
    }

    fn stream_gap(&self) -> Duration {
        STREAM_GAP.max(Duration::from_millis(u64::from(
            1000_u32.div_ceil(self.live_hz),
        )))
    }

    /// The value as this Device can take it: a series cut to its newest `series` numbers and text cut to
    /// `text_bytes` on a character boundary. None when nothing of it fits.
    fn fit(&self, value: &LiveValue) -> Option<LiveValue> {
        let fitted = match value {
            LiveValue::Series(values) => {
                let keep = values.len().min(self.series as usize);
                LiveValue::Series(values[values.len() - keep..].to_vec())
            }
            LiveValue::Text(text) => {
                let mut end = text.len().min(self.text_bytes as usize);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                LiveValue::Text(text[..end].to_owned())
            }
            other => other.clone(),
        };
        fitted
            .fits(self.series as usize, self.text_bytes as usize)
            .then_some(fitted)
    }
}

/// What the router last did for one Tile stream.
#[derive(Debug, Default)]
struct Track {
    /// The sample the Device holds.
    delivered: Option<u64>,
    /// A send whose reply never came: its `n` and sample, so the same sample goes again as an identical retry.
    unanswered: Option<(u32, u64)>,
    last_sent: Option<Instant>,
}

/// The Layout a Device shows and its bindings.
#[derive(Debug)]
struct Active {
    reference: String,
    bindings: Vec<Binding>,
}

#[derive(Debug, Default)]
struct DeviceState {
    session: u64,
    active: Option<Active>,
    tracks: HashMap<(String, String, String), Track>,
    next_n: u32,
    /// The cost of each request in the last second, oldest first.
    spent: VecDeque<(Instant, u32)>,
    blocked_until: Option<Instant>,
    rate_strikes: u32,
}

/// What one delivery round did on one Device, per Integration.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub sent: HashMap<String, u64>,
    pub errors: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Router {
    devices: HashMap<String, DeviceState>,
}

impl Router {
    /// The Integration streams some Device's active Layout binds. Only these are read from the Integrations.
    pub fn wanted(&self) -> BTreeSet<StreamKey> {
        self.devices
            .values()
            .filter_map(|device| device.active.as_ref())
            .flat_map(|active| active.bindings.iter().map(|b| b.source.clone()))
            .collect()
    }

    /// The Device showed a new Layout. Its bindings are read again and every value goes again.
    pub fn layout_changed(&mut self, device: &str) {
        if let Some(state) = self.devices.get_mut(device) {
            state.active = None;
        }
    }

    /// Drops the Devices that are no longer connected.
    pub fn retain(&mut self, connected: &BTreeSet<String>) {
        self.devices.retain(|id, _| connected.contains(id));
    }

    /// Sends one Device the values its Layout binds that changed since it last got them. `session` identifies the
    /// link session: a new one clears the Device's live values, so everything goes again.
    pub fn deliver(
        &mut self,
        device: &str,
        session_id: u64,
        session: &Session,
        caps: LiveCaps,
        samples: &HashMap<StreamKey, LiveSample>,
        now: Instant,
    ) -> Report {
        let state = self.devices.entry(device.to_owned()).or_default();
        if state.session != session_id {
            *state = DeviceState {
                session: session_id,
                next_n: state.next_n,
                ..DeviceState::default()
            };
        }
        let mut report = Report::default();
        if state.blocked_until.is_some_and(|until| until > now) {
            return report;
        }
        if state.active.is_none() {
            match read_active(session) {
                Ok(active) => {
                    state.tracks.clear();
                    state.active = Some(active);
                }
                Err(_) => {
                    state.blocked_until = Some(now + LAYOUT_RETRY);
                    return report;
                }
            }
        }
        while state
            .spent
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) >= Duration::from_secs(1))
        {
            state.spent.pop_front();
        }
        let mut budget = caps
            .live_total_hz
            .saturating_sub(state.spent.iter().map(|(_, cost)| cost).sum());
        let gap = caps.stream_gap();
        let Some(active) = state.active.as_ref() else {
            return report;
        };
        let mut due: Vec<(&Binding, &LiveSample)> = active
            .bindings
            .iter()
            .filter_map(|binding| {
                let sample = samples.get(&binding.source)?;
                let track = state.tracks.get(&key(binding));
                let fresh = track.is_none_or(|t| t.delivered != Some(sample.sample));
                let rested = track
                    .and_then(|t| t.last_sent)
                    .is_none_or(|at| now.duration_since(at) >= gap);
                (fresh && rested).then_some((binding, sample))
            })
            .collect();
        due.sort_by_key(|(binding, _)| state.tracks.get(&key(binding)).and_then(|t| t.last_sent));
        let reference = active.reference.clone();
        for (binding, sample) in due {
            let integration = binding.source.integration.clone();
            let Some(value) = caps
                .fit(&sample.value)
                .filter(|value| value.kind() == binding.kind)
            else {
                report.errors.insert(
                    integration,
                    format!(
                        "Tile \"{}\" takes a {} value",
                        binding.tile,
                        binding.kind.name()
                    ),
                );
                continue;
            };
            let track = state.tracks.entry(key(binding)).or_default();
            let n = match track.unanswered {
                Some((n, unanswered)) if unanswered == sample.sample => n,
                _ => {
                    state.next_n = state.next_n.saturating_add(1);
                    state.next_n
                }
            };
            let set = LiveSet {
                layout: &reference,
                page: &binding.page,
                tile: &binding.tile,
                stream: &binding.stream,
                n,
                ttl_ms: sample.ttl_ms.clamp(live::TTL_MIN_MS, live::TTL_MAX_MS),
                value: &value,
            };
            let cost = live::cost(set.to_payload().len());
            if cost > budget && !state.spent.is_empty() {
                break;
            }
            budget = budget.saturating_sub(cost);
            state.spent.push_back((now, cost));
            track.last_sent = Some(now);
            match live::set_live(session, &set) {
                Ok(_) => {
                    track.delivered = Some(sample.sample);
                    track.unanswered = None;
                    state.rate_strikes = 0;
                    *report.sent.entry(integration).or_default() += 1;
                }
                Err(LiveError::Refused {
                    reason: LiveReason::Rate,
                    retry_ms,
                    ..
                }) => {
                    track.unanswered = None;
                    state.rate_strikes += 1;
                    let wait =
                        Duration::from_millis(u64::from(retry_ms.unwrap_or(1000).clamp(1, 1000)))
                            * 2_u32.pow(state.rate_strikes.min(4) - 1);
                    state.blocked_until = Some(now + wait.min(BACKOFF_MAX));
                    break;
                }
                Err(LiveError::Refused {
                    reason:
                        LiveReason::Layout
                        | LiveReason::Page
                        | LiveReason::Tile
                        | LiveReason::Stream
                        | LiveReason::Type,
                    ..
                }) => {
                    state.active = None;
                    break;
                }
                Err(LiveError::Refused { reason, error, .. }) => {
                    track.delivered = Some(sample.sample);
                    track.unanswered = None;
                    report
                        .errors
                        .insert(integration, format!("{error} ({})", reason.name()));
                }
                Err(LiveError::Link(error)) => {
                    track.unanswered = Some((n, sample.sample));
                    report.errors.insert(integration, error.to_string());
                    break;
                }
            }
        }
        report
    }
}

fn key(binding: &Binding) -> (String, String, String) {
    (
        binding.page.clone(),
        binding.tile.clone(),
        binding.stream.clone(),
    )
}

/// Reads which Layout the Device shows: the built-in one, or the stored one, whose exact bytes name it.
fn read_active(session: &Session) -> Result<Active, String> {
    let state = session
        .layout_state(LAYOUT_STATE_TIMEOUT)
        .map_err(|error| error.to_string())?;
    let (reference, layout) = if state.source == "builtin" {
        (live::BUILTIN.to_owned(), Layout::parse(BUILTIN))
    } else {
        let orientation: Orientation = state.orientation;
        let report = xfer::get_layout(session, orientation).map_err(|error| error.to_string())?;
        (live::layout_ref(&report.data), Layout::parse(&report.data))
    };
    let layout = layout.map_err(|errors| format!("{} Layout problems", errors.len()))?;
    Ok(Active {
        reference,
        bindings: bindings(&layout),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use td_core::layout::Tile;
    use td_link::{MockDeck, SessionConfig};

    use super::*;

    const GRAPH: &[u8] = include_bytes!("../../../../../schema/examples/graph.json");
    const CAPS: LiveCaps = LiveCaps {
        live_hz: 2,
        live_total_hz: 16,
        series: 32,
        text_bytes: 0,
    };

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
        assert_eq!(
            session.identity(Duration::from_secs(1)).unwrap().protocol,
            2
        );
        session
    }

    fn show(session: &Session, layout: &Layout) {
        xfer::put_layout(session, Orientation::Landscape, layout).unwrap();
    }

    fn key(integration: &str, stream: &str) -> StreamKey {
        StreamKey {
            integration: integration.into(),
            account: String::new(),
            stream: stream.into(),
        }
    }

    fn sample(sample: u64, value: LiveValue) -> LiveSample {
        LiveSample {
            account: String::new(),
            stream: String::new(),
            sample,
            ttl_ms: 5_000,
            value,
        }
    }

    fn numbers(entries: &[(&str, u64, f64)]) -> HashMap<StreamKey, LiveSample> {
        entries
            .iter()
            .map(|(stream, n, v)| (key("this_pc", stream), sample(*n, LiveValue::Number(*v))))
            .collect()
    }

    /// One page of graph Tiles, each bound to the `test` Integration stream it names.
    fn graphs(streams: &[(&str, StreamType)]) -> Layout {
        let mut layout = Layout::parse(GRAPH).unwrap();
        let Page::Grid {
            tiles, rows, cols, ..
        } = &mut layout.pages[0]
        else {
            unreachable!("graph.json starts with a grid page")
        };
        (*rows, *cols) = (8, 8);
        let template = tiles[0].clone();
        *tiles = streams
            .iter()
            .enumerate()
            .map(|(i, (field, kind))| Tile {
                id: format!("t{i}"),
                label: format!("T{i}"),
                row: (i / 8) as u8,
                col: (i % 8) as u8,
                streams: Some(BTreeMap::from([("value".to_owned(), *kind)])),
                action: Action::Widget {
                    source: "test".into(),
                    field: (*field).into(),
                    account: String::new(),
                },
                ..template.clone()
            })
            .collect();
        layout
    }

    fn tiles(device: &MockDeck) -> Vec<String> {
        device
            .live_sets()
            .iter()
            .map(|set| set["tile"].as_str().unwrap().to_owned())
            .collect()
    }

    fn ns(device: &MockDeck) -> Vec<u64> {
        device
            .live_sets()
            .iter()
            .map(|set| set["n"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn a_tile_binds_its_widget_actions_integration_stream() {
        let graph = Layout::parse(GRAPH).unwrap();
        assert_eq!(
            bindings(&graph),
            [Binding {
                page: "page".into(),
                tile: "cpu".into(),
                stream: "value".into(),
                kind: StreamType::Number,
                source: key("this_pc", "cpu_pct"),
            }]
        );
        let mut layout = graphs(&[("player", StreamType::Text)]);
        let Page::Grid { tiles, .. } = &mut layout.pages[0] else {
            unreachable!("graphs() builds a grid page")
        };
        tiles[0].streams = Some(BTreeMap::from([
            ("title".to_owned(), StreamType::Text),
            ("value".to_owned(), StreamType::Number),
        ]));
        let streams: Vec<_> = bindings(&layout)
            .into_iter()
            .map(|b| (b.stream, b.source.stream))
            .collect();
        assert_eq!(
            streams,
            [
                ("title".to_owned(), "title".to_owned()),
                ("value".to_owned(), "player".to_owned())
            ]
        );
    }

    #[test]
    fn only_bound_streams_and_only_new_samples_go() {
        let device = MockDeck::v2();
        let session = open(&device);
        show(&session, &Layout::parse(GRAPH).unwrap());
        let mut router = Router::default();
        let t0 = Instant::now();
        let first = numbers(&[
            ("cpu_pct", 1, 25.0),
            ("ram_pct", 1, 50.0),
            ("gpu_pct", 1, 5.0),
        ]);
        let report = router.deliver("d", 1, &session, CAPS, &first, t0);
        assert_eq!(report.sent, HashMap::from([("this_pc".to_owned(), 1)]));
        assert_eq!(router.wanted(), BTreeSet::from([key("this_pc", "cpu_pct")]));
        router.deliver("d", 1, &session, CAPS, &first, t0 + Duration::from_secs(1));
        assert_eq!(tiles(&device), ["cpu"]);
        let same_value = numbers(&[("cpu_pct", 2, 25.0), ("ram_pct", 2, 51.0)]);
        router.deliver(
            "d",
            1,
            &session,
            CAPS,
            &same_value,
            t0 + Duration::from_secs(2),
        );
        assert_eq!(tiles(&device), ["cpu", "cpu"]);
        let cpu = device.live("page", "cpu", "value").unwrap();
        assert_eq!(cpu.samples, [25.0, 25.0]);
        assert_eq!(device.live_sets()[0]["ttl_ms"], 5_000);
    }

    #[test]
    fn samples_between_rounds_coalesce_and_each_stream_waits_its_gap() {
        let device = MockDeck::v2();
        let session = open(&device);
        show(&session, &Layout::parse(GRAPH).unwrap());
        let mut router = Router::default();
        let t0 = Instant::now();
        router.deliver("d", 1, &session, CAPS, &numbers(&[("cpu_pct", 1, 1.0)]), t0);
        let early = numbers(&[("cpu_pct", 2, 2.0)]);
        router.deliver(
            "d",
            1,
            &session,
            CAPS,
            &early,
            t0 + Duration::from_millis(300),
        );
        assert_eq!(device.live_sets().len(), 1);
        let latest = numbers(&[("cpu_pct", 3, 3.0)]);
        router.deliver(
            "d",
            1,
            &session,
            CAPS,
            &latest,
            t0 + Duration::from_millis(900),
        );
        assert_eq!(
            device.live("page", "cpu", "value").unwrap().samples,
            [1.0, 3.0]
        );
    }

    #[test]
    fn a_rate_refusal_holds_the_device_until_its_window_ends() {
        let device = MockDeck::v2();
        let session = open(&device);
        let fields: Vec<String> = (0..20).map(|i| format!("s{i}")).collect();
        let streams: Vec<(&str, StreamType)> = fields
            .iter()
            .map(|f| (f.as_str(), StreamType::Number))
            .collect();
        show(&session, &graphs(&streams));
        let generous = LiveCaps {
            live_total_hz: 64,
            ..CAPS
        };
        let samples: HashMap<_, _> = fields
            .iter()
            .map(|f| (key("test", f), sample(1, LiveValue::Number(1.0))))
            .collect();
        let mut router = Router::default();
        let t0 = Instant::now();
        let report = router.deliver("d", 1, &session, generous, &samples, t0);
        assert_eq!(
            report.sent["test"],
            u64::from(td_link::mock::MOCK_LIVE_TOTAL_HZ)
        );
        let held = router.deliver(
            "d",
            1,
            &session,
            generous,
            &samples,
            t0 + Duration::from_millis(1),
        );
        assert!(held.sent.is_empty());
        std::thread::sleep(Duration::from_millis(1_050));
        let later = router.deliver(
            "d",
            1,
            &session,
            generous,
            &samples,
            t0 + Duration::from_millis(1_050),
        );
        assert_eq!(later.sent["test"], 4);
        assert_eq!(device.live_sets().len(), 20);
    }

    #[test]
    fn a_new_session_or_layout_sends_every_value_again_with_rising_n() {
        let device = MockDeck::v2();
        let session = open(&device);
        let graph = Layout::parse(GRAPH).unwrap();
        show(&session, &graph);
        let mut router = Router::default();
        let t0 = Instant::now();
        let samples = numbers(&[("cpu_pct", 1, 7.0)]);
        router.deliver("d", 1, &session, CAPS, &samples, t0);
        session.identity(Duration::from_secs(1)).unwrap();
        assert!(device.live("page", "cpu", "value").is_none());
        router.deliver(
            "d",
            2,
            &session,
            CAPS,
            &samples,
            t0 + Duration::from_secs(1),
        );
        assert_eq!(device.live("page", "cpu", "value").unwrap().samples, [7.0]);

        let mut renamed = graph.clone();
        renamed.name = "Graph 2".into();
        show(&session, &renamed);
        router.layout_changed("d");
        router.deliver(
            "d",
            2,
            &session,
            CAPS,
            &samples,
            t0 + Duration::from_secs(2),
        );
        assert_eq!(device.live("page", "cpu", "value").unwrap().samples, [7.0]);
        assert_eq!(device.live_sets()[2]["layout"], device.live_layout().0);

        renamed.name = "Graph 3".into();
        show(&session, &renamed);
        let newer = numbers(&[("cpu_pct", 2, 8.0)]);
        router.deliver("d", 2, &session, CAPS, &newer, t0 + Duration::from_secs(3));
        router.deliver("d", 2, &session, CAPS, &newer, t0 + Duration::from_secs(4));
        assert_eq!(device.live("page", "cpu", "value").unwrap().samples, [8.0]);
        let n = ns(&device);
        assert!(n.windows(2).all(|pair| pair[0] < pair[1]), "{n:?}");
    }

    #[test]
    fn an_unanswered_send_repeats_with_the_same_n() {
        let device = MockDeck::v2();
        let session = open(&device);
        show(&session, &Layout::parse(GRAPH).unwrap());
        let mut router = Router::default();
        let t0 = Instant::now();
        router.deliver("d", 1, &session, CAPS, &numbers(&[("cpu_pct", 1, 3.0)]), t0);
        device.delay_replies(Duration::from_millis(2_300));
        let samples = numbers(&[("cpu_pct", 2, 4.0)]);
        let lost = router.deliver(
            "d",
            1,
            &session,
            CAPS,
            &samples,
            t0 + Duration::from_secs(1),
        );
        assert!(lost.errors.contains_key("this_pc"));
        device.delay_replies(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(400));
        let retried = router.deliver(
            "d",
            1,
            &session,
            CAPS,
            &samples,
            t0 + Duration::from_secs(2),
        );
        assert_eq!(retried.sent["this_pc"], 1);
        assert_eq!(
            device.live("page", "cpu", "value").unwrap().samples,
            [3.0, 4.0]
        );
        assert_eq!(ns(&device), [1, 2]);
        let newer = numbers(&[("cpu_pct", 3, 5.0)]);
        router.deliver("d", 1, &session, CAPS, &newer, t0 + Duration::from_secs(3));
        assert_eq!(ns(&device), [1, 2, 3]);
    }

    #[test]
    fn a_request_over_256_bytes_costs_more_of_the_device_budget() {
        let device = MockDeck::v2();
        let session = open(&device);
        show(
            &session,
            &graphs(&[("a", StreamType::Series), ("b", StreamType::Series)]),
        );
        let long: Vec<f64> = (0..40).map(|i| f64::from(i) + 0.123_456_789).collect();
        let samples: HashMap<_, _> = ["a", "b"]
            .into_iter()
            .map(|f| (key("test", f), sample(1, LiveValue::Series(long.clone()))))
            .collect();
        let tight = LiveCaps {
            live_total_hz: 4,
            ..CAPS
        };
        let mut router = Router::default();
        let t0 = Instant::now();
        router.deliver("d", 1, &session, tight, &samples, t0);
        let sets = device.live_sets();
        assert_eq!(sets.len(), 1);
        let mut request = sets[0].as_object().unwrap().clone();
        request.remove("at_ms");
        let bytes = serde_json::to_vec(&Value::Object(request)).unwrap().len();
        assert_eq!(live::cost(bytes), 3, "{bytes} bytes");
        let tile = sets[0]["tile"].as_str().unwrap();
        let shown = device.live("page", tile, "value").unwrap();
        assert_eq!(shown.samples, long[8..]);
        router.deliver(
            "d",
            1,
            &session,
            tight,
            &samples,
            t0 + Duration::from_secs(1),
        );
        assert_eq!(tiles(&device).len(), 2);
    }

    #[test]
    fn a_value_the_tile_cannot_take_is_reported_and_never_sent() {
        let device = MockDeck::v2();
        let session = open(&device);
        show(&session, &Layout::parse(GRAPH).unwrap());
        let mut router = Router::default();
        let t0 = Instant::now();
        let huge = numbers(&[("cpu_pct", 1, 2e9)]);
        let report = router.deliver("d", 1, &session, CAPS, &huge, t0);
        assert!(report.errors["this_pc"].contains("number"));
        let text: HashMap<_, _> = [(
            key("this_pc", "cpu_pct"),
            sample(2, LiveValue::Text("hi".into())),
        )]
        .into();
        router.deliver("d", 1, &session, CAPS, &text, t0 + Duration::from_secs(1));
        assert!(device.live_sets().is_empty());
    }
}
