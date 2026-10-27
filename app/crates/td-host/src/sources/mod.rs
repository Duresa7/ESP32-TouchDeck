//! Usage sources for the widget tiles and dashboard pages of the deck.
//!
//! A [`Source`] yields [`Reading`]s. The [`Hub`] polls every enabled source on its own
//! thread, hands the readings to a [`Scheduler`], and sends the `widget.set` and
//! `dashboard.set` requests that come due to a [`Sink`], which is the connected deck.
//! CLIProxyAPI lists auth files, then POSTs read-only provider GETs through the hub.

pub mod cliproxy;
pub mod scheduler;

use std::fs;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use td_core::{Layout, Request};

pub use td_core::app_dirs::write_atomic;

pub use cliproxy::{Account, CliProxy, Fetch, HttpFetch, ProxyInfo};
pub use scheduler::Scheduler;

pub const CLIPROXYAPI: &str = "cliproxyapi";
pub const DEFAULT_PROXY_URL: &str = "http://localhost:8317";
pub const DEFAULT_PROXY_INTERVAL_S: u64 = 120;
pub const PROXY_REFRESH_FLOOR: Duration = Duration::from_secs(15);

/// A source that fails this many polls in a row marks its readings stale.
pub const FAILS_TO_STALE: u32 = 3;
/// The least time between two sends for one tile or page.
pub const SEND_FLOOR: Duration = Duration::from_secs(2);

/// One value a source reports. `bar` is a fraction from 0 to 1, or -1 for none.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reading {
    pub source: String,
    pub account: String,
    pub field: String,
    pub value: String,
    pub bar: f64,
    /// Unix milliseconds of the data behind the reading.
    pub updated: u64,
    pub stale: bool,
}

impl Reading {
    pub fn new(
        source: &str,
        account: &str,
        field: &str,
        value: impl Into<String>,
        bar: f64,
        updated: u64,
        stale: bool,
    ) -> Reading {
        Reading {
            source: source.to_owned(),
            account: account.to_owned(),
            field: field.to_owned(),
            value: value.into(),
            bar,
            updated,
            stale,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Info {
    Proxy(ProxyInfo),
}

pub trait Source: Send {
    fn id(&self) -> &'static str;
    fn poll(&mut self) -> Vec<Reading>;
    /// A poll the user asked for, such as a tap on one of the source's tiles.
    fn refresh(&mut self) -> Vec<Reading> {
        self.poll()
    }
    fn info(&self) -> Info;
}

/// Where the requests go. The app implements it over the connected session.
pub trait Sink: Send + Sync {
    /// `Ok(None)` is a deck that took the request and `Ok(Some(reason))` one that refused
    /// it, which is not retried. `Err` is a link that failed, which is retried.
    fn send(&self, request: &Request) -> Result<Option<String>, String>;
}

/// Holds the CLIProxyAPI management key. The app backs it with the OS credential store.
pub trait KeyStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, key: &str) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemoryKeys(Mutex<Option<String>>);

impl KeyStore for MemoryKeys {
    fn get(&self) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn set(&self, key: &str) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(key.to_owned());
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// The last 4 characters of a key, which is all the app ever shows of it.
pub fn key_hint(key: &str) -> String {
    let tail: Vec<char> = key.chars().rev().take(4).collect();
    let tail: String = tail.into_iter().rev().collect();
    format!("…{tail}")
}

pub fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// What the sources page saves. The management key is never part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub cliproxy: bool,
    pub cliproxy_url: String,
    pub cliproxy_interval_s: u64,
    /// The layout whose widget and dashboard tiles the sources feed.
    pub layout_path: Option<String>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            cliproxy: false,
            cliproxy_url: DEFAULT_PROXY_URL.to_owned(),
            cliproxy_interval_s: DEFAULT_PROXY_INTERVAL_S,
            layout_path: None,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Config {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut text = serde_json::to_string_pretty(self).expect("config serializes");
        text.push('\n');
        write_atomic(path, text.as_bytes())
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.cliproxy_interval_s.max(60))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LayoutInfo {
    pub name: Option<String>,
    pub widgets: usize,
    pub dashboards: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    pub proxy: ProxyInfo,
    pub layout: LayoutInfo,
    pub deck: bool,
    pub sent: u64,
    pub send_error: Option<String>,
}

enum Cmd {
    Polled {
        generation: u64,
        readings: Vec<Reading>,
        info: Info,
    },
    Config(Config),
    Layout(Layout),
    Sink(Option<Arc<dyn Sink>>),
    Press(String),
    Refresh(String),
    Stop,
}

enum WorkerCmd {
    Refresh,
    Stop,
}

struct Worker {
    id: &'static str,
    tx: Sender<WorkerCmd>,
}

fn spawn_worker(
    mut source: Box<dyn Source>,
    every: Duration,
    generation: u64,
    core: Sender<Cmd>,
) -> (Worker, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let id = source.id();
    let handle = thread::spawn(move || {
        let mut refresh = false;
        loop {
            let readings = if refresh {
                source.refresh()
            } else {
                source.poll()
            };
            let info = source.info();
            if core
                .send(Cmd::Polled {
                    generation,
                    readings,
                    info,
                })
                .is_err()
            {
                return;
            }
            refresh = match rx.recv_timeout(every) {
                Ok(WorkerCmd::Refresh) => {
                    while let Ok(cmd) = rx.try_recv() {
                        if matches!(cmd, WorkerCmd::Stop) {
                            return;
                        }
                    }
                    true
                }
                Err(RecvTimeoutError::Timeout) => false,
                Ok(WorkerCmd::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            };
        }
    });
    (Worker { id, tx }, handle)
}

struct Core {
    config: Config,
    keys: Arc<dyn KeyStore>,
    status: Arc<Mutex<Status>>,
    scheduler: Scheduler,
    sink: Option<Arc<dyn Sink>>,
    tx: Sender<Cmd>,
    workers: Vec<Worker>,
    handles: Vec<JoinHandle<()>>,
    generation: u64,
    retry_at: Option<Instant>,
    proxy_refresh: RefreshGate,
}

#[derive(Default)]
struct RefreshGate(Option<Instant>);

impl RefreshGate {
    fn allow(&mut self, now: Instant) -> bool {
        if self
            .0
            .is_some_and(|last| now.saturating_duration_since(last) < PROXY_REFRESH_FLOOR)
        {
            return false;
        }
        self.0 = Some(now);
        true
    }
}

impl Core {
    fn apply(&mut self, config: Config) {
        self.stop_workers();
        self.generation += 1;
        self.config = config;
        {
            let mut status = self.status.lock().unwrap();
            status.proxy = ProxyInfo {
                url: self.config.cliproxy_url.clone(),
                insecure_http: cliproxy::insecure_http(&self.config.cliproxy_url),
                ..ProxyInfo::default()
            };
        }
        self.load_layout();
        if self.config.cliproxy {
            let source = CliProxy::new(
                &self.config.cliproxy_url,
                self.keys.clone(),
                Box::new(HttpFetch::new()),
            );
            self.start(Box::new(source), self.config.interval());
        }
    }

    fn start(&mut self, source: Box<dyn Source>, every: Duration) {
        let (worker, handle) = spawn_worker(source, every, self.generation, self.tx.clone());
        self.workers.push(worker);
        self.handles.push(handle);
    }

    fn stop_workers(&mut self) {
        for worker in self.workers.drain(..) {
            let _ = worker.tx.send(WorkerCmd::Stop);
        }
        self.handles.clear();
    }

    fn load_layout(&mut self) {
        let Some(path) = self
            .config
            .layout_path
            .clone()
            .filter(|p| !p.trim().is_empty())
        else {
            return;
        };
        match fs::read(&path)
            .map_err(|error| format!("{path}: {error}"))
            .and_then(|bytes| {
                Layout::parse(&bytes).map_err(|errors| {
                    errors
                        .first()
                        .map_or_else(|| "invalid layout".to_owned(), ToString::to_string)
                })
            }) {
            Ok(layout) => self.use_layout(&layout),
            Err(error) => {
                self.scheduler.set_layout(None);
                self.status.lock().unwrap().layout = LayoutInfo {
                    error: Some(error),
                    ..LayoutInfo::default()
                };
            }
        }
    }

    fn use_layout(&mut self, layout: &Layout) {
        self.scheduler.set_layout(Some(layout));
        let (widgets, dashboards) = self.scheduler.counts();
        self.status.lock().unwrap().layout = LayoutInfo {
            name: Some(layout.name.clone()),
            widgets,
            dashboards,
            error: None,
        };
    }

    fn handle(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Polled {
                generation,
                readings,
                info,
            } => {
                if generation == self.generation {
                    let source = match &info {
                        Info::Proxy(_) => CLIPROXYAPI,
                    };
                    self.scheduler.update(source, readings);
                    let mut status = self.status.lock().unwrap();
                    match info {
                        Info::Proxy(info) => status.proxy = info,
                    }
                }
            }
            Cmd::Config(config) => self.apply(config),
            Cmd::Layout(layout) => self.use_layout(&layout),
            Cmd::Sink(sink) => {
                self.status.lock().unwrap().deck = sink.is_some();
                self.sink = sink;
                self.scheduler.resync();
                self.retry_at = None;
            }
            Cmd::Press(tile) => {
                if let Some(source) = self.scheduler.source_of_tile(&tile).map(str::to_owned) {
                    self.refresh(&source);
                }
            }
            Cmd::Refresh(source) => self.refresh(&source),
            Cmd::Stop => return false,
        }
        true
    }

    fn refresh(&mut self, source: &str) {
        if !self.workers.iter().any(|worker| worker.id == source) {
            return;
        }
        if source == CLIPROXYAPI && !self.proxy_refresh.allow(Instant::now()) {
            return;
        }
        for worker in self.workers.iter().filter(|worker| worker.id == source) {
            let _ = worker.tx.send(WorkerCmd::Refresh);
        }
    }

    fn flush(&mut self) {
        let Some(sink) = self.sink.clone() else {
            return;
        };
        let now = Instant::now();
        if self.retry_at.is_some_and(|at| now < at) {
            return;
        }
        self.retry_at = None;
        let due = self.scheduler.due(now);
        for (done, request) in due.iter().enumerate() {
            let result = sink.send(request);
            let mut status = self.status.lock().unwrap();
            match result {
                Ok(refused) => {
                    status.sent += 1;
                    status.send_error = refused;
                }
                Err(error) => {
                    status.send_error = Some(error);
                    drop(status);
                    for request in &due[done..] {
                        self.scheduler.forget(request);
                    }
                    self.retry_at = Some(now + Duration::from_secs(1));
                    return;
                }
            }
        }
    }

    fn wait(&self) -> Duration {
        let idle = Duration::from_secs(1);
        let mut wait = match (&self.sink, self.retry_at) {
            (None, _) => idle,
            (Some(_), Some(at)) => at.saturating_duration_since(Instant::now()),
            (Some(_), None) => self
                .scheduler
                .pending_in(Instant::now())
                .map_or(idle, |left| left.min(idle)),
        };
        if wait.is_zero() {
            wait = Duration::from_millis(20);
        }
        wait
    }
}

fn core_loop(mut core: Core, rx: &Receiver<Cmd>) {
    loop {
        let first = match rx.recv_timeout(core.wait()) {
            Ok(cmd) => Some(cmd),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let mut running = first.is_none_or(|cmd| core.handle(cmd));
        while running && let Ok(cmd) = rx.try_recv() {
            running = core.handle(cmd);
        }
        if !running {
            break;
        }
        core.flush();
    }
    core.stop_workers();
}

/// Runs the sources and feeds the deck. It holds no deck of its own: the app hands it
/// a [`Sink`] on connect and takes it back on disconnect.
pub struct Hub {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Hub {
    pub fn start(config: Config, keys: Arc<dyn KeyStore>) -> Hub {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let mut core = Core {
            config: Config::default(),
            keys,
            status: status.clone(),
            scheduler: Scheduler::default(),
            sink: None,
            tx: tx.clone(),
            workers: Vec::new(),
            handles: Vec::new(),
            generation: 0,
            retry_at: None,
            proxy_refresh: RefreshGate::default(),
        };
        core.apply(config);
        let thread = thread::spawn(move || core_loop(core, &rx));
        Hub {
            tx,
            status,
            thread: Mutex::new(Some(thread)),
        }
    }

    pub fn configure(&self, config: Config) {
        let _ = self.tx.send(Cmd::Config(config));
    }

    /// Feeds the tiles of this layout, such as the one the editor just pushed.
    pub fn use_layout(&self, layout: Layout) {
        let _ = self.tx.send(Cmd::Layout(layout));
    }

    pub fn set_sink(&self, sink: Option<Arc<dyn Sink>>) {
        let _ = self.tx.send(Cmd::Sink(sink));
    }

    /// A tap asks the tile's source to refresh. CLIProxyAPI accepts at most one per 15 s.
    pub fn pressed(&self, tile: &str) {
        let _ = self.tx.send(Cmd::Press(tile.to_owned()));
    }

    pub fn refresh(&self, source: &str) {
        let _ = self.tx.send(Cmd::Refresh(source.to_owned()));
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_refresh_ignores_bursts_and_accepts_the_fifteen_second_boundary() {
        let start = Instant::now();
        let mut gate = RefreshGate::default();
        assert!(gate.allow(start));
        for ms in [0, 1, 100, 14_999] {
            assert!(!gate.allow(start + Duration::from_millis(ms)));
        }
        assert!(gate.allow(start + Duration::from_secs(15)));
        assert!(!gate.allow(start + Duration::from_secs(29)));
        assert!(gate.allow(start + Duration::from_secs(30)));
    }
}
