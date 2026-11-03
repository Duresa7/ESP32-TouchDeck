//! A Device's taps on renderer controls and its drags, run on the Integrations' Controls. They come only from a
//! Device's touch (or a test-hooks MockDeck); no API route reaches them. A Control starts no process, so nothing
//! here asks for approval. Every value comes from the Device and is checked before it binds anything.
use super::{ActionSpec, Actions, ApprovalRequest, TAP_WAIT};
use crate::devices::{DeviceId, Input, InputHandler, LinkState};
use crate::features::integrations::Controls;
use crate::live::{Live, StreamKey, stream_source};
use std::sync::{Arc, Weak};
use std::time::Duration;
use td_core::layout::{StreamType, Tile, TileRender};
use td_host::actions::Plan;
use td_host::integrations::ControlError;

/// How long a tap's action may take before the Control gives up.
pub(crate) const ACT_TIMEOUT: Duration = Duration::from_secs(2);
const UNAVAILABLE: &str = "This Tile's control isn't available.";
const GESTURE_DONE: &str = "Volume changed.";

/// Whether an Integration stream takes a Device's input, from the Integration's `settable` flag.
pub(crate) type Settable = Arc<dyn Fn(&StreamKey) -> bool + Send + Sync>;

pub(super) struct Wiring {
    live: Live,
    controls: Controls,
    settable: Settable,
}

impl Actions {
    /// Lets a Device's taps on renderer controls and its drags reach the Integrations' Controls. Until this is
    /// called they do nothing.
    pub(crate) fn attach(&self, live: Live, controls: Controls, settable: Settable) {
        let _ = self.0.controls.set(Wiring {
            live,
            controls,
            settable,
        });
        self.0
            .devices
            .input_handler(Arc::new(Inputs(Arc::downgrade(&self.0))));
    }

    /// The `since_ms` of the Device's link session, if it is connected over v2, the only protocol that sends
    /// `part` and `v2.input`.
    fn v2_session(&self, device: &DeviceId) -> Option<u64> {
        match self.0.devices.get(device)?.link {
            LinkState::Connected {
                since_ms,
                protocol: 2,
                ..
            } => Some(since_ms),
            _ => None,
        }
    }

    /// Runs a tap that lifted inside a renderer control. The actor's gate already checked its shape, its `seq`
    /// and the rate.
    pub(super) fn part(&self, device: &DeviceId, event: &serde_json::Value) {
        let text = |name: &str| event[name].as_str().unwrap_or_default();
        let (page, tile, part) = (text("page"), text("tile"), text("part"));
        let (Some(wiring), Some(session)) = (self.0.controls.get(), self.v2_session(device)) else {
            return;
        };
        let request = integration_request(device, page, tile);
        let found = match wiring.live.tile(device, session, page, tile) {
            Some(found) => Ok(Some(found)),
            None => self.read_tile(device, page, tile),
        };
        let found = match found {
            Ok(found) => found,
            Err(reason) => return self.logged(&request, "failed", reason, None, None),
        };
        let Some(Plan::Integration {
            source,
            account,
            action,
        }) = found.and_then(|found| Plan::part(&found, part))
        else {
            return self.logged(&request, "failed", UNAVAILABLE.into(), None, None);
        };
        let Some(control) = wiring.controls.get(&source) else {
            return self.logged(&request, "failed", UNAVAILABLE.into(), None, None);
        };
        match control.act(&account, &action, ACT_TIMEOUT) {
            Ok(()) => self.logged(&request, "ok", done(&action).into(), None, None),
            Err(error) => self.logged(&request, "failed", detail(error), None, None),
        }
    }

    /// The Tile as the Device's own Layout has it, read over the link, for a tap the live router's Layout can't
    /// answer yet.
    fn read_tile(&self, device: &DeviceId, page: &str, tile: &str) -> Result<Option<Tile>, String> {
        match self.0.links.within(device, TAP_WAIT, |session| {
            self.resolve(device, session, page, tile)
        }) {
            Some(Ok(found)) => found.map(|found| found.map(|(tile, _)| tile)),
            Some(Err(link)) => Err(link.to_string()),
            None => Err(
                "The Device stayed busy with a Layout or Asset transfer, so the tap was dropped."
                    .into(),
            ),
        }
    }

    fn set(&self, device: &DeviceId, input: &Input) -> Option<Result<(), String>> {
        let wiring = self.0.controls.get()?;
        let session = self.v2_session(device)?;
        let tile = wiring
            .live
            .tile(device, session, &input.page, &input.tile)?;
        let takes_input = tile.input
            && tile.render == Some(TileRender::Bar)
            && tile
                .streams
                .as_ref()
                .and_then(|streams| streams.get(&input.stream))
                == Some(&StreamType::Number);
        if !takes_input {
            return None;
        }
        let source = stream_source(&tile, &input.stream)?;
        let control = wiring.controls.get(&source.integration);
        let Some(control) = control.filter(|_| (wiring.settable)(&source)) else {
            return Some(Err(UNAVAILABLE.into()));
        };
        Some(
            control
                .set(&source.account, &source.stream, input.value)
                .map_err(detail),
        )
    }

    fn gesture(&self, device: &DeviceId, input: &Input, outcome: Result<(), String>) {
        let request = integration_request(device, &input.page, &input.tile);
        match outcome {
            Ok(()) => self.logged(&request, "ok", GESTURE_DONE.into(), None, None),
            Err(reason) => self.logged(&request, "failed", reason, None, None),
        }
    }
}

struct Inputs(Weak<super::Inner>);

impl InputHandler for Inputs {
    fn set(&self, device: &DeviceId, input: &Input) -> Option<Result<(), String>> {
        Actions(self.0.upgrade()?).set(device, input)
    }
    fn gesture(&self, device: &DeviceId, input: &Input, outcome: Result<(), String>) {
        if let Some(inner) = self.0.upgrade() {
            Actions(inner).gesture(device, input, outcome);
        }
    }
}

fn integration_request(device: &DeviceId, page: &str, tile: &str) -> ApprovalRequest {
    ApprovalRequest {
        id: String::new(),
        device: device.clone(),
        page: page.into(),
        tile: tile.into(),
        hash: String::new(),
        spec: ActionSpec {
            kind: "integration".into(),
            program: String::new(),
            args: vec![],
            cwd: String::new(),
            shell: false,
        },
        created_ms: crate::now_ms(),
    }
}

/// The action log's words for a part that ran, from `docs/copy.md`.
fn done(action: &str) -> &'static str {
    match action {
        "mute" => "Mute changed.",
        "previous" => "Previous track.",
        "next" => "Next track.",
        _ => "Playback changed.",
    }
}

/// The action log's words for a Control's refusal: its own words when its backend failed, else `docs/copy.md`'s.
fn detail(error: ControlError) -> String {
    match error {
        ControlError::Unavailable(words) if !words.is_empty() => words,
        _ => UNAVAILABLE.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{AllowedAction, GrantStore};
    use super::*;
    use crate::api::{Clock, Events};
    use crate::devices::{BoardKind, Config, DeviceManager};
    use crate::features::integrations::Service;
    use crate::link::Links;
    use crate::live::fake::FakeControl;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::time::Instant;
    use td_core::InputPhase;
    use td_host::integrations::Control;
    use td_link::MockDeck;

    const DEVICE: &str = "mock0001";
    const MEDIA: &[u8] = include_bytes!("../../../../../schema/examples/media.json");

    struct NoGrants;
    impl GrantStore for NoGrants {
        fn load(&self) -> std::io::Result<Vec<AllowedAction>> {
            Ok(Vec::new())
        }
        fn save(&self, _: &[AllowedAction]) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct Running {
        actions: Actions,
        deck: MockDeck,
        devices: DeviceManager,
        live: Option<Live>,
        volume: Arc<FakeControl>,
        media: Arc<FakeControl>,
    }

    impl Drop for Running {
        fn drop(&mut self) {
            self.live.take();
            self.devices.stop();
        }
    }

    type Entry = (String, String, String, String);

    impl Running {
        fn log(&self) -> Vec<Entry> {
            self.actions
                .log(None)
                .into_iter()
                .map(|l| (l.tile, l.kind, l.status, l.detail))
                .collect()
        }

        fn drag(&self, tile: &str, values: &[f64]) {
            for (i, value) in values.iter().enumerate() {
                let phase = if i + 1 == values.len() {
                    InputPhase::End
                } else {
                    InputPhase::Move
                };
                self.deck.emit_input("media", tile, "value", phase, *value);
                std::thread::sleep(Duration::from_millis(30));
            }
        }

        fn last_set(&self) -> Option<f64> {
            self.volume.sets.lock().last().map(|set| set.2)
        }
    }

    fn wait(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn media() -> Value {
        serde_json::from_slice(MEDIA).unwrap()
    }

    /// A connected mock Device showing `layout`, fake Volume and Media Controls, and Actions attached to a live
    /// router. With `cached`, it waits until the router has read the Layout; without, the router never sees the
    /// Device, so every lookup misses its cache.
    fn running(layout: Value, cached: bool) -> Running {
        let events = Events::default();
        let devices = DeviceManager::new(events.clone());
        let config = Config::mock(None);
        let ports = config.mock.as_ref().unwrap().0.clone();
        ports.plug(DEVICE, BoardKind::Device);
        let deck = ports.deck(DEVICE).unwrap();
        deck.set_media_capabilities(true);
        devices.start(config);
        let id = DeviceId::parse(DEVICE).unwrap();
        let session = || match devices.get(&id).map(|d| d.link) {
            Some(LinkState::Connected { since_ms, .. }) => Some(since_ms),
            _ => None,
        };
        wait("the Device connects", || session().is_some());
        let links = Links::new(devices.clone());
        let layout = td_core::Layout::parse(&serde_json::to_vec(&layout).unwrap()).unwrap();
        links
            .with(DEVICE, |s| {
                td_link::xfer::put_layout(s, layout.orientation, &layout)
            })
            .unwrap()
            .unwrap();
        let volume = Arc::new(FakeControl::default());
        let media = Arc::new(FakeControl::default());
        let controls: Controls = Arc::new(BTreeMap::from([
            ("volume".to_owned(), volume.clone() as Arc<dyn Control>),
            ("media".to_owned(), media.clone() as Arc<dyn Control>),
        ]));
        let mut service = Service::new(true, events.clone());
        service.controls = controls.clone();
        let watched = if cached {
            devices.clone()
        } else {
            DeviceManager::new(events.clone())
        };
        let live = Live::start(watched, links.clone(), service);
        let actions = Actions::new(
            events,
            Clock::default(),
            devices.clone(),
            links,
            Arc::new(NoGrants),
            true,
        );
        actions.attach(
            live.clone(),
            controls,
            Arc::new(|key: &StreamKey| key.integration == "volume" && key.stream == "level"),
        );
        if cached {
            wait("the router reads the Layout", || {
                live.tile(DEVICE, session().unwrap(), "media", "volume")
                    .is_some()
            });
        }
        Running {
            actions,
            deck,
            devices,
            live: Some(live),
            volume,
            media,
        }
    }

    fn entry(tile: &str, status: &str, detail: &str) -> Entry {
        (
            tile.into(),
            "integration".into(),
            status.into(),
            detail.into(),
        )
    }

    #[test]
    fn a_drag_sets_the_bound_integration_stream_and_logs_once_per_gesture() {
        let td = running(media(), true);
        td.drag("volume", &[0.2, 0.4, 0.5, 0.6]);
        wait("the gesture ends", || td.last_set() == Some(0.6));
        let sets = td.volume.sets.lock().clone();
        assert!(
            sets.iter().all(|(account, stream, value)| account.is_empty()
                && stream == "level"
                && [0.2, 0.4, 0.5, 0.6].contains(value)),
            "{sets:?}"
        );
        wait("the gesture logs", || !td.log().is_empty());
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(td.log(), [entry("volume", "ok", GESTURE_DONE)]);
    }

    #[test]
    fn a_tap_on_a_part_acts_on_the_widget_account_with_the_part_unchanged() {
        let mut layout = media();
        layout["pages"][0]["tiles"][2]["action"]["account"] = json!("chosen");
        let td = running(layout, true);
        td.deck.emit_press_part("media", "playing", "widget", "next");
        td.deck.emit_press_part("media", "volume", "widget", "mute");
        wait("both parts run", || td.log().len() == 2);
        assert_eq!(
            *td.media.acts.lock(),
            [("chosen".to_owned(), "next".to_owned(), ACT_TIMEOUT)]
        );
        assert_eq!(
            *td.volume.acts.lock(),
            [(String::new(), "mute".to_owned(), ACT_TIMEOUT)]
        );
        assert_eq!(
            td.log(),
            [
                entry("playing", "ok", "Next track."),
                entry("volume", "ok", "Mute changed.")
            ]
        );
    }

    #[test]
    fn a_failed_control_logs_its_own_words_once() {
        let td = running(media(), true);
        *td.media.act_result.lock() = Some(ControlError::Unavailable("Nothing is playing.".into()));
        *td.volume.set_result.lock() = Some(ControlError::Unavailable("No playback device.".into()));
        td.deck
            .emit_press_part("media", "playing", "widget", "play_pause");
        wait("the tap logs", || td.log().len() == 1);
        td.drag("volume", &[0.3, 0.4, 0.5]);
        wait("the gesture logs", || td.log().len() == 2);
        *td.media.act_result.lock() = Some(ControlError::UnknownAccount);
        td.deck.emit_press_part("media", "playing", "widget", "next");
        wait("the refused tap logs", || td.log().len() == 3);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            td.log(),
            [
                entry("playing", "failed", "Nothing is playing."),
                entry("volume", "failed", "No playback device."),
                entry("playing", "failed", UNAVAILABLE),
            ]
        );
    }

    #[test]
    fn hostile_input_never_reaches_a_control_its_tile_does_not_bind() {
        let td = running(media(), true);
        for tile in ["nope", "level", "playing"] {
            td.drag(tile, &[0.9, 0.95]);
        }
        td.drag("volume", &[7.5, -3.0]);
        wait("the clamped gesture ends", || td.last_set() == Some(0.0));
        wait("the clamped gesture logs", || td.log().len() == 1);
        let send = |event: Value| td.deck.emit_value(&event);
        let input = |seq: u32, field: &str, value: Value| {
            let mut event = json!({"m": "v2.input", "page": "media", "tile": "volume", "stream": "value",
                "phase": "end", "value": 0.5, "seq": 1_000_000 + seq, "t": 1});
            event[field] = value;
            event
        };
        send(input(1, "stream", json!("muted")));
        send(input(2, "page", json!("other")));
        send(input(3, "value", json!("0.5")));
        send(input(4, "value", Value::Null));
        send(input(5, "tile", json!("x".repeat(900))));
        for (seq, part, tile) in [
            (1, "mute", "level"),
            (2, "next", "volume"),
            (3, "launch", "playing"),
            (4, "PLAY", "playing"),
            (5, "next", "nope"),
        ] {
            send(json!({"m": "press", "page": "media", "tile": tile, "kind": "widget", "part": part,
                "seq": 1_000_000 + seq, "t": 1}));
        }
        wait("the parts log", || td.log().len() == 5);
        std::thread::sleep(Duration::from_millis(200));
        let values: Vec<f64> = td.volume.sets.lock().iter().map(|s| s.2).collect();
        assert_eq!(values, [1.0, 0.0]);
        assert!(td.volume.acts.lock().is_empty());
        assert!(td.media.acts.lock().is_empty());
        assert!(td.media.sets.lock().is_empty());
        let unavailable = |tile: &str| entry(tile, "failed", UNAVAILABLE);
        assert_eq!(
            td.log(),
            [
                entry("volume", "ok", GESTURE_DONE),
                unavailable("level"),
                unavailable("volume"),
                unavailable("playing"),
                unavailable("nope"),
            ]
        );
    }

    #[test]
    fn a_flood_of_drags_and_taps_stays_bounded_and_the_next_gesture_still_works() {
        let td = running(media(), true);
        let started = Instant::now();
        for i in 0..5_000 {
            let value = f64::from(i % 100) / 100.0;
            td.deck
                .emit_input("media", "volume", "value", InputPhase::Move, value);
        }
        for _ in 0..200 {
            td.deck.emit_press_part("media", "playing", "widget", "next");
        }
        let flooded = started.elapsed();
        std::thread::sleep(Duration::from_millis(300));
        let sets = td.volume.sets.lock().len();
        assert!(sets < 5_000 / 4, "{sets} sets for 5000 moves");
        let acts = td.media.acts.lock().len() as u64;
        assert!(acts <= 10 * (flooded.as_secs() + 2), "{acts} acts in {flooded:?}");
        assert!(td.log().len() as u64 <= acts);

        std::thread::sleep(Duration::from_secs(1));
        td.drag("volume", &[0.25, 0.33]);
        wait("a later gesture is set", || td.last_set() == Some(0.33));
    }

    #[test]
    fn before_the_router_reads_the_layout_a_tap_reads_it_from_the_device_and_a_drag_waits() {
        let td = running(media(), false);
        td.drag("volume", &[0.4, 0.5]);
        td.deck
            .emit_press_part("media", "playing", "widget", "previous");
        wait("the tap runs", || !td.log().is_empty());
        assert_eq!(
            *td.media.acts.lock(),
            [(String::new(), "previous".to_owned(), ACT_TIMEOUT)]
        );
        assert_eq!(td.log(), [entry("playing", "ok", "Previous track.")]);
        assert!(td.volume.sets.lock().is_empty());
    }
}
