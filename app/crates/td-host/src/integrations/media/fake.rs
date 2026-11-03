use super::*;
use std::sync::mpsc::{self, Sender};

struct State {
    feed: Option<Feed>,
    commands: VecDeque<String>,
    response: Option<bool>,
    pending: Vec<Sender<Result<bool, String>>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            feed: None,
            commands: VecDeque::new(),
            response: Some(true),
            pending: Vec::new(),
        }
    }
}
#[derive(Clone, Default)]
pub struct FakeMedia(Arc<Mutex<State>>);
impl FakeMedia {
    pub fn feed(&self) -> Feed {
        self.0
            .lock()
            .unwrap()
            .feed
            .clone()
            .expect("fake provider started")
    }
    pub fn running(&self) -> bool {
        self.0.lock().unwrap().feed.is_some()
    }
    pub fn begin(&self) -> u64 {
        self.feed().invalidate()
    }
    pub fn metadata(&self, generation: u64, title: &str, artist: &str, playback: Playback) {
        self.feed().metadata(generation, title, artist, playback);
    }
    pub fn cover(&self, generation: u64, bytes: &[u8]) {
        self.feed().cover(generation, bytes);
    }
    pub fn idle(&self) {
        let feed = self.feed();
        let generation = feed.invalidate();
        feed.idle(generation);
    }
    pub fn heartbeat(&self, now: Instant) {
        self.feed().heartbeat(now);
    }
    pub fn commands(&self) -> Vec<String> {
        self.0.lock().unwrap().commands.iter().cloned().collect()
    }
    pub fn respond(&self, response: Option<bool>) {
        self.0.lock().unwrap().response = response;
    }
}
struct Session(FakeMedia);
impl Drop for Session {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap();
        state.feed = None;
        state.pending.clear();
    }
}
impl MediaProvider for FakeMedia {
    fn start(&self, feed: Feed) -> Result<Arc<dyn MediaSession>, String> {
        feed.idle(feed.generation());
        self.0.lock().unwrap().feed = Some(feed);
        Ok(Arc::new(Session(self.clone())))
    }
}
impl MediaSession for Session {
    fn command(&self, command: MediaCommand, deadline: Instant) -> Receiver<Result<bool, String>> {
        let (tx, rx) = mpsc::channel();
        let mut state = self.0.0.lock().unwrap();
        if Instant::now() >= deadline {
            return rx;
        }
        if state.commands.len() == 1024 {
            state.commands.pop_front();
        }
        state.commands.push_back(command.name().into());
        if let Some(response) = state.response {
            let _ = tx.send(Ok(response));
        } else if state.pending.len() < 32 {
            state.pending.push(tx);
        }
        rx
    }
}
