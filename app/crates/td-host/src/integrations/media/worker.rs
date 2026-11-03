use super::*;

pub(super) struct WorkerState {
    feed: Feed,
    rechecks: VecDeque<Instant>,
    identity: Arc<AtomicU64>,
}
impl WorkerState {
    pub fn new(feed: Feed, identity: Arc<AtomicU64>) -> Self {
        Self {
            feed,
            identity,
            rechecks: VecDeque::new(),
        }
    }
    pub fn command_current(&self, identity: u64) -> bool {
        self.feed.active.load(Ordering::SeqCst) && self.identity.load(Ordering::SeqCst) == identity
    }
    pub fn session_changed(&mut self) {
        self.identity.fetch_add(1, Ordering::SeqCst);
        self.rechecks.clear();
    }
    pub fn properties_event(&mut self, now: Instant) {
        if self.rechecks.is_empty() {
            self.metadata_changed(now);
        }
    }
    pub fn metadata_changed(&mut self, now: Instant) {
        self.rechecks = [300, 1000, 3000]
            .map(|ms| now + Duration::from_millis(ms))
            .into();
    }
    pub fn retry_due(&mut self, now: Instant) -> bool {
        if self.rechecks.front().is_some_and(|due| now >= *due) {
            self.rechecks.pop_front();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    struct Player {
        state: Arc<Mutex<Option<WorkerState>>>,
    }
    impl MediaProvider for Player {
        fn start(&self, feed: Feed) -> Result<Arc<dyn MediaSession>, String> {
            feed.metadata(feed.generation(), "Track", "Artist", Playback::Playing);
            *self.state.lock().unwrap() =
                Some(WorkerState::new(feed.clone(), Arc::new(AtomicU64::new(1))));
            Ok(Arc::new(CommandPlayer {
                state: self.state.clone(),
            }))
        }
    }
    struct CommandPlayer {
        state: Arc<Mutex<Option<WorkerState>>>,
    }
    impl MediaSession for CommandPlayer {
        fn command(&self, _: MediaCommand, _: Instant) -> Receiver<Result<bool, String>> {
            let mut guard = self.state.lock().unwrap();
            let state = guard.as_mut().unwrap();
            let identity = state.identity.load(Ordering::SeqCst);
            state.feed.invalidate();
            state.properties_event(Instant::now());
            let (tx, rx) = mpsc::sync_channel(1);
            tx.send(if state.command_current(identity) {
                Ok(true)
            } else {
                Err(NO_RESPONSE.into())
            })
            .unwrap();
            rx
        }
    }
    fn player() -> (Media, Arc<Player>) {
        let player = Arc::new(Player {
            state: Arc::new(Mutex::new(None)),
        });
        (Media::new(player.clone()), player)
    }
    #[test]
    fn property_event_before_true_command_completion_reports_success() {
        let (media, _) = player();
        for action in ["next", "previous"] {
            assert!(
                media
                    .control()
                    .unwrap()
                    .act("", action, Duration::from_secs(1))
                    .is_ok()
            );
        }
    }
    #[test]
    fn obsolete_session_and_stopped_lifecycle_refuse_commands() {
        let (mut media, player) = player();
        let mut guard = player.state.lock().unwrap();
        let state = guard.as_mut().unwrap();
        let identity = state.identity.load(Ordering::SeqCst);
        state.session_changed();
        assert!(!state.command_current(identity));
        let identity = state.identity.load(Ordering::SeqCst);
        assert!(state.command_current(identity));
        media.stop();
        assert!(!state.command_current(identity));
    }
    fn fetch(state: &WorkerState, art: bool) {
        let generation = state.feed.generation();
        state
            .feed
            .metadata(generation, "Track", "Artist", Playback::Playing);
        if art {
            state.feed.cover(generation, b"synthetic cover");
        }
    }
    fn has_cover(media: &Media) -> bool {
        media
            .live()
            .iter()
            .any(|s| matches!(&s.value, LiveValue::Image(Some(_))))
    }
    #[test]
    fn duplicate_properties_preserve_delayed_cover_retry() {
        let (media, player) = player();
        let mut guard = player.state.lock().unwrap();
        let state = guard.as_mut().unwrap();
        let now = Instant::now();
        state.feed.invalidate();
        state.properties_event(now);
        state.metadata_changed(now);
        fetch(state, false);
        state.feed.invalidate();
        state.properties_event(now + Duration::from_millis(100));
        fetch(state, false);
        assert!(!has_cover(&media));
        if state.retry_due(now + Duration::from_millis(300)) {
            fetch(state, true);
        }
        assert!(has_cover(&media));
    }
    #[test]
    fn identical_strings_on_later_track_start_cover_retries() {
        let (media, player) = player();
        let mut guard = player.state.lock().unwrap();
        let state = guard.as_mut().unwrap();
        let previous = Instant::now();
        state.properties_event(previous);
        state.metadata_changed(previous);
        for ms in [300, 1000, 3000] {
            assert!(state.retry_due(previous + Duration::from_millis(ms)));
        }
        let now = previous + Duration::from_secs(4);
        fetch(state, true);
        state.feed.invalidate();
        state.properties_event(now);
        fetch(state, false);
        assert!(!has_cover(&media));
        if state.retry_due(now + Duration::from_millis(300)) {
            fetch(state, true);
        }
        assert!(has_cover(&media));
    }
}
