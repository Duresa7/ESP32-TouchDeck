//! What a Device's touch asks of its Integrations: drags (`v2.input`) and taps on a renderer's control (`press`
//! with `part`). Everything here comes from the Device, so it is untrusted: each event is checked against the
//! link contract, and what is kept per session is bounded, whatever the Device sends.
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};
use td_core::{Event, InputPhase};

/// The most Tile streams one batch holds. A Device has one finger, so a real batch holds one.
pub(crate) const PENDING_MAX: usize = 64;
/// Taps on renderer controls, and logged gestures, each allowed per Device per second.
pub(crate) const PER_SECOND: usize = 10;

type StreamOf = (String, String, String);

/// One drag snapshot that passed the checks, its value clamped to 0..1.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Input {
    pub page: String,
    pub tile: String,
    pub stream: String,
    pub phase: InputPhase,
    pub value: f64,
    pub seq: u32,
}

impl Input {
    fn key(&self) -> StreamOf {
        (self.page.clone(), self.tile.clone(), self.stream.clone())
    }

    fn parse(event: &Value) -> Option<Input> {
        let Ok(Event::Input {
            page,
            tile,
            stream,
            phase,
            value,
            seq,
            ..
        }) = serde_json::from_value(event.clone())
        else {
            return None;
        };
        (layout_id(&page) && layout_id(&tile) && stream == "value" && value.is_finite()).then(
            || Input {
                page,
                tile,
                stream,
                phase,
                value: value.clamp(0.0, 1.0),
                seq,
            },
        )
    }
}

/// A Layout id as the Layout schema allows it: `^[a-z0-9_]{1,15}$`.
fn layout_id(text: &str) -> bool {
    (1..=15).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// A renderer part as link-v2 allows it: `^[a-z_]{1,15}$`.
fn part_name(text: &str) -> bool {
    (1..=15).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

/// At most `PER_SECOND` events in any one second.
#[derive(Default)]
struct Rate(VecDeque<Instant>);

impl Rate {
    fn allow(&mut self, now: Instant) -> bool {
        while self
            .0
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(1))
        {
            self.0.pop_front();
        }
        if self.0.len() >= PER_SECOND {
            return false;
        }
        self.0.push_back(now);
        true
    }
}

/// One Device session's drags. It keeps the newest snapshot per Tile stream until the next batch, drops any
/// event whose `seq` doesn't rise, and remembers whether a gesture's set failed so the gesture logs once.
#[derive(Default)]
pub(crate) struct InputHub {
    last_seq: Option<u32>,
    pending: BTreeMap<StreamOf, Input>,
    failed: HashMap<StreamOf, String>,
    logs: Rate,
}

impl InputHub {
    /// Takes one `v2.input` event. A malformed one, a replayed or older `seq`, or a new Tile stream past
    /// `PENDING_MAX` is dropped.
    pub fn push(&mut self, event: &Value) {
        let Some(input) = Input::parse(event) else {
            return;
        };
        if self.last_seq.is_some_and(|last| input.seq <= last) {
            return;
        }
        self.last_seq = Some(input.seq);
        let key = input.key();
        if self.pending.len() >= PENDING_MAX && !self.pending.contains_key(&key) {
            return;
        }
        self.pending.insert(key, input);
    }

    /// The newest snapshot of each Tile stream since the last batch.
    pub fn take(&mut self) -> Vec<Input> {
        std::mem::take(&mut self.pending).into_values().collect()
    }

    /// Records how setting `input` went. At a gesture's `end` it returns the gesture's outcome, the first
    /// failure if any set failed, unless this Device already logged `PER_SECOND` gestures in the last second.
    pub fn settled(
        &mut self,
        input: &Input,
        result: Result<(), String>,
        now: Instant,
    ) -> Option<Result<(), String>> {
        let key = input.key();
        if let Err(detail) = result
            && (self.failed.len() < PENDING_MAX || self.failed.contains_key(&key))
        {
            self.failed.entry(key.clone()).or_insert(detail);
        }
        if input.phase != InputPhase::End {
            return None;
        }
        let outcome = self.failed.remove(&key).map_or(Ok(()), Err);
        self.logs.allow(now).then_some(outcome)
    }
}

/// One Device session's taps on renderer controls: well formed, `seq` rising, at most `PER_SECOND` a second.
#[derive(Default)]
pub(crate) struct PressGate {
    last_seq: Option<u32>,
    rate: Rate,
}

impl PressGate {
    pub fn accept(&mut self, event: &Value, now: Instant) -> bool {
        let Ok(Event::Press {
            page,
            tile,
            part: Some(part),
            seq,
            ..
        }) = serde_json::from_value(event.clone())
        else {
            return false;
        };
        if !(layout_id(&page) && layout_id(&tile) && part_name(&part))
            || self.last_seq.is_some_and(|last| seq <= last)
        {
            return false;
        }
        self.last_seq = Some(seq);
        self.rate.allow(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(seq: u64, value: f64) -> Value {
        json!({"m": "v2.input", "page": "main", "tile": "volume", "stream": "value", "phase": "move",
            "seq": seq, "value": value, "t": 1})
    }

    fn end(seq: u64, value: f64) -> Value {
        let mut event = input(seq, value);
        event["phase"] = json!("end");
        event
    }

    fn press(seq: u64, part: &str) -> Value {
        json!({"m": "press", "page": "main", "tile": "media", "kind": "widget", "part": part, "seq": seq,
            "t": 1})
    }

    #[test]
    fn moves_coalesce_to_the_newest_clamped_value_and_replayed_seqs_drop() {
        let mut hub = InputHub::default();
        hub.push(&input(1, 0.2));
        hub.push(&input(3, 2.0));
        hub.push(&input(2, 0.1));
        let batch = hub.take();
        assert_eq!(batch.len(), 1);
        assert_eq!((batch[0].value, batch[0].seq), (1.0, 3));

        hub.push(&input(3, 0.1));
        assert!(hub.take().is_empty());

        hub.push(&end(4, -1.0));
        let batch = hub.take();
        assert_eq!((batch[0].value, batch[0].phase), (0.0, InputPhase::End));
    }

    #[test]
    fn a_flood_is_bounded_and_malformed_input_is_dropped() {
        let mut hub = InputHub::default();
        for seq in 1..10_000 {
            let mut event = input(seq, 0.5);
            event["tile"] = json!(format!("t{seq}"));
            hub.push(&event);
        }
        assert_eq!(hub.take().len(), PENDING_MAX);

        let mut next = 10_000;
        for (field, value) in [
            ("page", json!("x".repeat(10_000))),
            ("page", json!("Main")),
            ("tile", json!("")),
            ("tile", json!("vol\u{202e}ume")),
            ("stream", json!("muted")),
            ("phase", json!("begin")),
            ("seq", json!(u64::MAX)),
            ("seq", json!(-1)),
            ("value", Value::Null),
            ("value", json!("0.5")),
            ("m", json!("press")),
        ] {
            next += 1;
            let mut event = input(next, 0.5);
            event[field] = value;
            hub.push(&event);
        }
        assert!(hub.take().is_empty());
        let mut no_seq = input(0, 0.5);
        no_seq.as_object_mut().unwrap().remove("seq");
        hub.push(&no_seq);
        assert!(hub.take().is_empty());
    }

    #[test]
    fn a_gesture_reports_its_first_failure_once_at_its_end() {
        let mut hub = InputHub::default();
        let now = Instant::now();
        hub.push(&input(1, 0.5));
        let moved = hub.take().remove(0);
        assert_eq!(hub.settled(&moved, Err("first".into()), now), None);
        assert_eq!(hub.settled(&moved, Err("second".into()), now), None);
        hub.push(&end(2, 0.6));
        let ended = hub.take().remove(0);
        assert_eq!(hub.settled(&ended, Ok(()), now), Some(Err("first".into())));

        hub.push(&end(3, 0.7));
        let ended = hub.take().remove(0);
        assert_eq!(hub.settled(&ended, Ok(()), now), Some(Ok(())));
    }

    #[test]
    fn logged_gestures_are_limited_per_second() {
        let mut hub = InputHub::default();
        let now = Instant::now();
        let mut logged = 0;
        for seq in 1..=50 {
            hub.push(&end(seq, 0.5));
            let ended = hub.take().remove(0);
            logged += usize::from(hub.settled(&ended, Ok(()), now).is_some());
        }
        assert_eq!(logged, PER_SECOND);
        hub.push(&end(51, 0.5));
        let ended = hub.take().remove(0);
        assert!(
            hub.settled(&ended, Ok(()), now + Duration::from_secs(1))
                .is_some()
        );
    }

    #[test]
    fn taps_on_parts_are_well_formed_rising_and_limited_to_ten_a_second() {
        let mut gate = PressGate::default();
        let now = Instant::now();
        assert!(gate.accept(&press(1, "next"), now));
        assert!(!gate.accept(&press(1, "next"), now));
        for seq in 2..=10 {
            assert!(gate.accept(&press(seq, "next"), now));
        }
        assert!(!gate.accept(&press(11, "next"), now));
        assert!(gate.accept(&press(12, "next"), now + Duration::from_secs(1)));

        let later = now + Duration::from_secs(2);
        for (seq, part) in [
            (13, json!("Next")),
            (14, json!("next!")),
            (15, json!("")),
            (16, json!("a_part_name_too_long")),
            (17, json!(7)),
            (18, Value::Null),
        ] {
            let mut event = press(seq, "next");
            event["part"] = part;
            assert!(!gate.accept(&event, later), "{event}");
        }
        let mut no_part = press(19, "next");
        no_part.as_object_mut().unwrap().remove("part");
        assert!(!gate.accept(&no_part, later));
        let mut bad_tile = press(20, "next");
        bad_tile["tile"] = json!("x".repeat(64));
        assert!(!gate.accept(&bad_tile, later));
        assert!(gate.accept(&press(21, "play_pause"), later));
    }
}
