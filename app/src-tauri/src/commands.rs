use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use td_core::{Hello, Layout, Orientation, Request, Settings};
use td_host::actions::{self, Plan};
use td_host::icons::{self, Builtin, IconCache};
use td_host::{Approvals, CommandSpec, FakeSink, Grant, InputSink, Mirror, Mirrored, Outcome};
use td_link::discover::{Candidate, Probe, discover, probe_transport};
use td_link::mock::MOCK_PORT;
use td_link::xfer::{IconSync, PutReport, get_layout, prune_icons, push_icons, put_layout};
use td_link::{Incoming, MockDeck, Serial, Session, SessionConfig, Transport};

use crate::app_updates::AppUpdateGate;
use crate::sources::SourcesState;

const HELLO_TIMEOUT: Duration = Duration::from_secs(1);
const SETTINGS_TIMEOUT: Duration = Duration::from_secs(2);

struct Link {
    id: u64,
    port: String,
    hello: Hello,
    session: Arc<Session>,
}

pub struct AppState {
    mock: Option<MockDeck>,
    link: Mutex<Option<Link>>,
    sessions: AtomicU64,
    updating: AtomicBool,
    fake_input: Option<FakeSink>,
    mirror: Mutex<MirrorInfo>,
    action_layout: Mutex<Option<(Layout, String)>>,
    approvals: Mutex<Approvals>,
    pending: Mutex<BTreeMap<u64, ApprovalRequest>>,
    requests: AtomicU64,
    action_log: Mutex<VecDeque<ActionLog>>,
    log_path: Mutex<Option<PathBuf>>,
    icon_cache: Mutex<Option<IconCache>>,
}

impl AppState {
    pub fn new(mock: bool) -> AppState {
        // Linux tests type into a fake sink, since a CI runner has no /dev/uinput.
        let fake = std::env::var("TOUCHDECK_INPUT").is_ok_and(|value| value == "fake")
            || (mock && cfg!(target_os = "linux"));
        AppState {
            mock: mock.then(MockDeck::new),
            link: Mutex::new(None),
            sessions: AtomicU64::new(0),
            updating: AtomicBool::new(false),
            fake_input: fake.then(FakeSink::default),
            mirror: Mutex::new(MirrorInfo::default()),
            action_layout: Mutex::new(None),
            approvals: Mutex::new(Approvals::in_memory()),
            pending: Mutex::new(BTreeMap::new()),
            requests: AtomicU64::new(0),
            action_log: Mutex::new(VecDeque::new()),
            log_path: Mutex::new(None),
            icon_cache: Mutex::new(None),
        }
    }

    /// Keeps approvals in `approvals.json` under the config directory and the action log in
    /// `actions.log` under the log directory. The mock deck keeps approvals in memory, so a
    /// test never grants anything on the real list.
    pub fn use_dirs(&self, config: Option<PathBuf>, logs: Option<PathBuf>) -> Result<(), String> {
        if let Some(logs) = logs {
            let _ = std::fs::create_dir_all(&logs);
            *self.log_path.lock().unwrap() = Some(logs.join("actions.log"));
        }
        if let (None, Some(config)) = (&self.mock, config) {
            let path = config.join("approvals.json");
            let approvals = Approvals::load(path.clone())
                .map_err(|error| format!("{}: {error}", path.display()))?;
            *self.approvals.lock().unwrap() = approvals;
        }
        Ok(())
    }

    /// Keeps icon files taken from apps under `<cache>/icons`.
    pub fn use_icon_dir(&self, cache: Option<PathBuf>) {
        let dir = cache.unwrap_or_else(|| std::env::temp_dir().join("touchdeck"));
        *self.icon_cache.lock().unwrap() = Some(IconCache::new(dir.join("icons")));
    }

    fn icon_cache(&self) -> Result<IconCache, String> {
        self.icon_cache
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "the icon cache is not set up".to_owned())
    }

    fn input_sink(&self) -> Result<Box<dyn InputSink>, String> {
        match &self.fake_input {
            Some(fake) => Ok(Box::new(fake.clone())),
            None => td_host::input::system_sink(),
        }
    }

    fn transport(&self, port: &str) -> Result<Box<dyn Transport>, String> {
        match &self.mock {
            Some(mock) if port == MOCK_PORT => Ok(Box::new(mock.connect())),
            Some(_) => Err(format!("{port} is not the mock deck")),
            None => Serial::open(port)
                .map(|serial| Box::new(serial) as Box<dyn Transport>)
                .map_err(|error| format!("{port}: {error}")),
        }
    }

    fn connected(&self) -> Option<(DeckInfo, Arc<Session>)> {
        self.link.lock().unwrap().as_ref().map(|link| {
            let info = DeckInfo {
                session: link.id,
                port: link.port.clone(),
                hello: link.hello.clone(),
                online: link.session.is_open(),
            };
            (info, link.session.clone())
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeckInfo {
    session: u64,
    port: String,
    hello: Hello,
    online: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LinkClosed {
    session: u64,
    port: String,
    reason: String,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_decks(app: AppHandle) -> Result<Vec<Candidate>, String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let state = app.state::<AppState>();
        if state.updating.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        let connected = state.connected();
        let ports: Vec<String> = match &state.mock {
            Some(_) => vec![MOCK_PORT.to_owned()],
            None => td_link::discover::candidate_ports(),
        };
        if state.mock.is_none() && connected.is_none() {
            return Ok(discover());
        }
        Ok(ports
            .into_iter()
            .map(|port| match &connected {
                Some((info, _)) if info.port == port => Candidate {
                    port,
                    probe: Probe::Deck {
                        hello: info.hello.clone(),
                    },
                },
                _ => {
                    let probe = match state.transport(&port) {
                        Ok(transport) => probe_transport(transport),
                        Err(detail) => Probe::Busy { detail },
                    };
                    Candidate { port, probe }
                }
            })
            .collect())
    })
    .await?
}

#[tauri::command]
pub async fn connect(app: AppHandle, port: String) -> Result<DeckInfo, String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let state = app.state::<AppState>();
        if state.updating.load(Ordering::Acquire) {
            return Err("firmware update in progress".into());
        }
        if let Some(previous) = state.link.lock().unwrap().take() {
            previous.session.close();
        }
        let (session, incoming) = Session::open(state.transport(&port)?, SessionConfig::default())
            .map_err(|error| error.to_string())?;
        let hello = session
            .hello(HELLO_TIMEOUT)
            .map_err(|error| format!("{port}: {error}"))?;
        if !hello.is_deck() {
            return Err(format!(
                "{port} is not a TouchDeck: product {:?} protocol {}",
                hello.product, hello.proto
            ));
        }
        let mirror = start_mirror(&state, &hello);
        let session = Arc::new(session);
        app.state::<SourcesState>().connected(session.clone());
        let id = state.sessions.fetch_add(1, Ordering::Relaxed) + 1;
        *state.link.lock().unwrap() = Some(Link {
            id,
            port: port.clone(),
            hello: hello.clone(),
            session: session.clone(),
        });
        let forwarder = app.clone();
        let closed = LinkClosed {
            session: id,
            port: port.clone(),
            reason: String::new(),
        };
        let puller = (app.clone(), session.clone());
        std::thread::spawn(move || forward(&forwarder, &session, &incoming, closed, mirror));
        std::thread::spawn(move || pull_action_layout(&puller.0, &puller.1));
        Ok(DeckInfo {
            session: id,
            port,
            hello,
            online: true,
        })
    })
    .await?
}

fn forward(
    app: &AppHandle,
    session: &Arc<Session>,
    incoming: &Receiver<Incoming>,
    closed: LinkClosed,
    mut mirror: Option<Mirror>,
) {
    for item in incoming {
        match item {
            Incoming::Line(line) => {
                let _ = app.emit("console-line", line);
            }
            Incoming::Event(event) => {
                if let Some(report) = Mirrored::parse(&event) {
                    mirror_report(app, mirror.as_mut(), &report);
                } else if event.get("m").and_then(Value::as_str) == Some("press") {
                    if let Some(tile) = event["tile"].as_str() {
                        app.state::<SourcesState>().pressed(tile);
                    }
                    if event["kind"] != "widget" {
                        on_press(app, session, &event);
                    }
                }
                let _ = app.emit("deck-event", event);
            }
            Incoming::Closed(reason) => {
                if let Some(mirror) = mirror.as_mut() {
                    mirror.release_all();
                }
                let state = app.state::<AppState>();
                let mut link = state.link.lock().unwrap();
                if link.as_ref().is_some_and(|link| link.id == closed.session) {
                    link.take();
                    app.state::<SourcesState>().disconnected();
                }
                drop(link);
                session.close();
                let _ = app.emit("link-closed", LinkClosed { reason, ..closed });
                return;
            }
        }
    }
}

#[tauri::command]
pub async fn disconnect(app: AppHandle) -> Result<(), String> {
    app.state::<SourcesState>().disconnected();
    blocking(move || {
        let link = app.state::<AppState>().link.lock().unwrap().take();
        if let Some(link) = link {
            link.session.close();
        }
    })
    .await
}

#[tauri::command]
pub fn deck_info(state: State<'_, AppState>) -> Option<DeckInfo> {
    state.connected().map(|(info, _)| info)
}

#[tauri::command]
pub async fn console_send(app: AppHandle, line: String) -> Result<(), String> {
    blocking(move || {
        let (_, session) = app
            .state::<AppState>()
            .connected()
            .ok_or("no deck is connected")?;
        session.send_line(&line).map_err(|error| error.to_string())
    })
    .await?
}

fn session(app: &AppHandle) -> Result<Arc<Session>, String> {
    app.state::<AppState>()
        .connected()
        .map(|(_, session)| session)
        .ok_or_else(|| "no deck is connected".to_owned())
}

fn parse_layout(text: &str) -> Result<Layout, String> {
    Layout::parse(text.as_bytes()).map_err(|errors| {
        let all: Vec<String> = errors.iter().map(ToString::to_string).collect();
        all.join("; ")
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct PushReport {
    #[serde(flatten)]
    put: PutReport,
    icons: IconSync,
}

/// Validates a layout with the schema rules, sends the icons it names that the deck lacks, and
/// pushes its canonical form into the deck's slot for its own orientation.
#[tauri::command]
pub async fn push_layout(app: AppHandle, layout: String) -> Result<PushReport, String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let parsed = parse_layout(&layout)?;
        let session = session(&app)?;
        let cache = app.state::<AppState>().icon_cache()?;
        let icons = push_icons(&session, &icons::file_ids(&parsed), |id| cache.get(id))
            .map_err(|error| format!("icons: {error}"))?;
        let put =
            put_layout(&session, parsed.orientation, &parsed).map_err(|error| error.to_string())?;
        if put.state.source == "storage" && put.state.orientation == parsed.orientation {
            use_deck_layout(&app, parsed);
        }
        Ok(PushReport { put, icons })
    })
    .await?
}

/// Removes every icon on the deck that neither `layouts` nor the deck's own stored layouts name.
#[tauri::command]
pub async fn remove_unused_icons(
    app: AppHandle,
    layouts: Vec<String>,
) -> Result<Vec<String>, String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let session = session(&app)?;
        let mut keep = std::collections::BTreeSet::new();
        for text in &layouts {
            keep.extend(icons::file_ids(&parse_layout(text)?));
        }
        for orientation in [Orientation::Landscape, Orientation::Portrait] {
            // A slot with no stored layout names no icon.
            if let Ok(report) = get_layout(&session, orientation)
                && let Ok(stored) = Layout::parse(&report.data)
            {
                keep.extend(icons::file_ids(&stored));
            }
        }
        prune_icons(&session, &keep).map_err(|error| error.to_string())
    })
    .await?
}

/// The built-in icon set, with each icon's `file:` id.
#[tauri::command]
pub async fn builtin_icons() -> Result<Vec<Builtin>, String> {
    blocking(|| icons::builtin().to_vec()).await
}

/// The built-in icon a tile's action suggests, if any.
#[tauri::command]
pub fn default_icon(action: Value) -> Option<Builtin> {
    let name = icons::default_for(&action)?;
    icons::builtin()
        .iter()
        .find(|icon| icon.name == name)
        .cloned()
}

/// The `TDI1` file for an icon id, from the built-in set or the app's cache.
#[tauri::command]
pub fn icon_file(state: State<'_, AppState>, id: String) -> Result<tauri::ipc::Response, String> {
    state
        .icon_cache()?
        .get(&id)
        .map(tauri::ipc::Response::new)
        .ok_or_else(|| format!("no icon file for {id}"))
}

/// Takes the icon of the program at `path`, stores it in the cache, and returns its id.
#[tauri::command]
pub async fn take_app_icon(app: AppHandle, path: String) -> Result<String, String> {
    blocking(move || {
        app.state::<AppState>()
            .icon_cache()?
            .take_app_icon(std::path::Path::new(&path))
    })
    .await?
}

/// Pulls the layout the deck stores for `orientation` and returns its text.
#[tauri::command]
pub async fn pull_layout(app: AppHandle, orientation: Orientation) -> Result<String, String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let report =
            get_layout(&*session(&app)?, orientation).map_err(|error| error.to_string())?;
        String::from_utf8(report.data).map_err(|_| "the deck's layout is not UTF-8".to_owned())
    })
    .await?
}

#[tauri::command]
pub async fn settings_get(app: AppHandle) -> Result<Settings, String> {
    blocking(move || {
        session(&app)?
            .request(&Request::SettingsGet, SETTINGS_TIMEOUT)
            .map_err(|error| error.to_string())?
            .fields()
            .map_err(|error| error.to_string())
    })
    .await?
}

/// Stores the settings and returns the deck's reply fields, `reboot_required` included.
#[tauri::command]
pub async fn settings_put(app: AppHandle, settings: Settings) -> Result<Value, String> {
    blocking(move || {
        session(&app)?
            .request(&Request::SettingsPut(settings), SETTINGS_TIMEOUT)
            .map_err(|error| error.to_string())?
            .fields()
            .map_err(|error| error.to_string())
    })
    .await?
}

/// Restarts the deck with the console's `reboot` command.
#[tauri::command]
pub async fn reboot_deck(app: AppHandle) -> Result<(), String> {
    blocking(move || {
        session(&app)?
            .send_line("reboot")
            .map_err(|error| error.to_string())
    })
    .await?
}

/// What the mock deck recorded, for the e2e specs to assert on.
#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_mock_record(state: State<'_, AppState>) -> Result<Value, String> {
    let mock = state
        .mock
        .as_ref()
        .ok_or("the app is not running the mock deck")?;
    let layout = |orientation| {
        mock.layout(orientation)
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    Ok(serde_json::json!({
        "received": mock.received(),
        "lines": mock.lines(),
        "settings": mock.settings(),
        "reboots": mock.reboots(),
        "icons": mock.icons(),
        "layouts": {
            "landscape": layout(Orientation::Landscape),
            "portrait": layout(Orientation::Portrait),
        },
    }))
}

#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_mock(state: State<'_, AppState>, action: String) -> Result<(), String> {
    let mock = state
        .mock
        .as_ref()
        .ok_or("the app is not running the mock deck")?;
    match action.as_str() {
        "no-confirm" | "rollback" | "normal-update" => mock.update_mode(&action),
        "stop" => mock.stop(),
        "hang-up" => mock.hang_up(),
        other => return Err(format!("unknown mock action {other:?}")),
    }
    Ok(())
}

// Firmware may spend 3 s waiting for the LVGL lock before replying.
const LAYOUT_PULL_TIMEOUT: Duration = Duration::from_secs(5);
const ACTION_LOG_KEPT: usize = 100;

#[derive(Debug, Clone, Default, Serialize)]
pub struct MirrorInfo {
    enabled: bool,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct KeyMirrored {
    seq: u32,
    report: String,
    outcome: &'static str,
    gap: Option<(u32, u32)>,
    errors: Vec<String>,
    /// When the event reached the app, in milliseconds since the Unix epoch.
    arrived_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequest {
    id: u64,
    page: String,
    tile: String,
    hash: String,
    #[serde(flatten)]
    spec: CommandSpec,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionLog {
    at_ms: f64,
    page: String,
    tile: String,
    kind: String,
    status: String,
    detail: String,
    pid: Option<u32>,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutSummary {
    name: String,
    source: String,
    pages: usize,
    tiles: usize,
}

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |since| since.as_secs_f64() * 1000.0)
}

fn start_mirror(state: &AppState, hello: &Hello) -> Option<Mirror> {
    let (mirror, info) = match state.input_sink() {
        Ok(sink) => {
            let mut mirror = Mirror::new(sink);
            let wanted = Mirror::wanted(&hello.sinks);
            mirror.set_enabled(wanted);
            let reason = if wanted {
                "typing the deck's keys on this PC"
            } else {
                "off: the deck types through its own USB HID sink"
            };
            (
                Some(mirror),
                MirrorInfo {
                    enabled: wanted,
                    reason: reason.into(),
                },
            )
        }
        Err(error) => (
            None,
            MirrorInfo {
                enabled: false,
                reason: format!("off: {error}"),
            },
        ),
    };
    *state.mirror.lock().unwrap() = info;
    mirror
}

fn mirror_report(app: &AppHandle, mirror: Option<&mut Mirror>, report: &Mirrored) {
    let arrived_ms = now_ms();
    let (outcome, errors) = match mirror {
        Some(mirror) => (mirror.apply(report), std::mem::take(&mut mirror.errors)),
        None => (Outcome::Off, Vec::new()),
    };
    let (label, gap) = match outcome {
        Outcome::Typed { gap } => ("typed", gap),
        Outcome::Duplicate => ("duplicate", None),
        Outcome::Off => ("off", None),
        Outcome::Unknown => ("unknown", None),
    };
    if let Some((from, to)) = gap {
        eprintln!("key mirror: seq {from} to {to} never arrived; released every held key");
    }
    let _ = app.emit(
        "key-mirror",
        KeyMirrored {
            seq: report.seq(),
            report: report.describe(),
            outcome: label,
            gap,
            errors,
            arrived_ms,
        },
    );
}

fn summary(layout: &Layout, source: &str) -> LayoutSummary {
    let tiles = layout
        .pages
        .iter()
        .map(|page| match page {
            td_core::layout::Page::Grid { tiles, .. } => tiles.len(),
            td_core::layout::Page::Dashboard { .. } => 0,
        })
        .sum();
    LayoutSummary {
        name: layout.name.clone(),
        source: source.into(),
        pages: layout.pages.len(),
        tiles,
    }
}

/// Reads the displayed layout, including builtin fallback, so previous-session actions and
/// source bindings cannot survive a reconnect to the built-in pages.
fn pull_action_layout(app: &AppHandle, session: &Session) -> bool {
    let Ok(_transfer) = app.state::<AppUpdateGate>().begin_transfer() else {
        return false;
    };
    let Some(summary) = refresh_deck_bindings(
        &app.state::<AppState>(),
        &app.state::<SourcesState>(),
        session,
    ) else {
        return false;
    };
    let _ = app.emit("action-layout", summary);
    true
}

fn refresh_deck_bindings(
    state: &AppState,
    sources: &SourcesState,
    session: &Session,
) -> Option<LayoutSummary> {
    let shown = session.layout_state(LAYOUT_PULL_TIMEOUT).ok()?;
    let layout = match shown.source.as_str() {
        "builtin" => Layout::parse(include_bytes!("../../../schema/examples/builtin.json")).ok()?,
        "storage" => {
            let report = td_link::xfer::get_layout(session, shown.orientation).ok()?;
            Layout::parse(&report.data).ok()?
        }
        _ => return None,
    };
    Some(bind_deck_layout(state, sources, layout))
}

fn use_deck_layout(app: &AppHandle, layout: Layout) {
    let summary = bind_deck_layout(
        &app.state::<AppState>(),
        &app.state::<SourcesState>(),
        layout,
    );
    let _ = app.emit("action-layout", summary);
}

fn bind_deck_layout(state: &AppState, sources: &SourcesState, layout: Layout) -> LayoutSummary {
    sources.use_layout(layout.clone());
    let summary = summary(&layout, "deck");
    *state.action_layout.lock().unwrap() = Some((layout, "deck".into()));
    summary
}

fn resolve(app: &AppHandle, page: &str, tile: &str) -> Option<Plan> {
    let state = app.state::<AppState>();
    let layout = state.action_layout.lock().unwrap();
    let (layout, _) = layout.as_ref()?;
    actions::find_tile(layout, page, tile).map(|tile| Plan::of(&tile.action))
}

fn on_press(app: &AppHandle, session: &Session, event: &Value) {
    let field = |name: &str| {
        event
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let (page, tile, kind) = (field("page"), field("tile"), field("kind"));
    let entry = |status: &str, detail: String| ActionLog {
        at_ms: now_ms(),
        page: page.clone(),
        tile: tile.clone(),
        kind: kind.clone(),
        status: status.into(),
        detail,
        pid: None,
        exit_code: None,
    };
    let plan = match resolve(app, &page, &tile) {
        Some(plan) => plan,
        // The deck may show a layout pushed since the app last read it, so read it again once.
        None if pull_action_layout(app, session) => match resolve(app, &page, &tile) {
            Some(plan) => plan,
            None => {
                return log_action(
                    app,
                    entry("unknown", "no such tile in the deck's layout".into()),
                );
            }
        },
        None => {
            return log_action(
                app,
                entry("unknown", "no layout to look the tile up in".into()),
            );
        }
    };
    match plan {
        Plan::Deck => {}
        Plan::Launch { path, args, cwd } => match actions::launch(&path, &args, &cwd) {
            Ok(pid) => log_action(
                app,
                ActionLog {
                    pid: Some(pid),
                    ..entry("started", path)
                },
            ),
            Err(error) => log_action(app, entry("failed", format!("{path}: {error}"))),
        },
        Plan::Open { target } => match actions::open(&target) {
            Ok(()) => log_action(app, entry("opened", target)),
            Err(error) => log_action(app, entry("failed", format!("{target}: {error}"))),
        },
        Plan::Command(spec) => {
            let hash = spec.hash();
            let state = app.state::<AppState>();
            if state.approvals.lock().unwrap().is_always(&hash) {
                return run_command(app, page, tile, spec, "always allowed");
            }
            let id = state.requests.fetch_add(1, Ordering::Relaxed) + 1;
            let request = ApprovalRequest {
                id,
                page,
                tile,
                hash,
                spec,
            };
            state.pending.lock().unwrap().insert(id, request.clone());
            let _ = app.emit("approval-request", request);
        }
    }
}

fn run_command(app: &AppHandle, page: String, tile: String, spec: CommandSpec, why: &str) {
    let app = app.clone();
    let why = why.to_owned();
    std::thread::spawn(move || {
        let line = std::iter::once(spec.program.as_str())
            .chain(spec.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        let (status, exit_code, detail) = match actions::run(&spec) {
            Ok(exit) => ("exited", exit.code(), format!("{line} ({why})")),
            Err(error) => ("failed", None, format!("{line}: {error}")),
        };
        log_action(
            &app,
            ActionLog {
                at_ms: now_ms(),
                page,
                tile,
                kind: "command".into(),
                status: status.into(),
                detail,
                pid: None,
                exit_code,
            },
        );
    });
}

fn log_action(app: &AppHandle, entry: ActionLog) {
    let state = app.state::<AppState>();
    {
        let mut log = state.action_log.lock().unwrap();
        log.push_back(entry.clone());
        while log.len() > ACTION_LOG_KEPT {
            log.pop_front();
        }
    }
    if let Some(path) = state.log_path.lock().unwrap().as_ref()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(
            file,
            "{}",
            serde_json::to_string(&entry).unwrap_or_default()
        );
    }
    let _ = app.emit("action-log", entry);
}

#[tauri::command]
pub fn mirror_state(state: State<'_, AppState>) -> MirrorInfo {
    state.mirror.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_action_layout(
    app: AppHandle,
    state: State<'_, AppState>,
    json: String,
    source: Option<String>,
) -> Result<LayoutSummary, String> {
    let layout = Layout::parse(json.as_bytes()).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let source = source.unwrap_or_else(|| "file".into());
    let summary = summary(&layout, &source);
    *state.action_layout.lock().unwrap() = Some((layout, source));
    let _ = app.emit("action-layout", summary.clone());
    Ok(summary)
}

#[tauri::command]
pub fn action_layout(state: State<'_, AppState>) -> Option<LayoutSummary> {
    state
        .action_layout
        .lock()
        .unwrap()
        .as_ref()
        .map(|(layout, source)| summary(layout, source))
}

#[tauri::command]
pub fn pending_approvals(state: State<'_, AppState>) -> Vec<ApprovalRequest> {
    state.pending.lock().unwrap().values().cloned().collect()
}

#[tauri::command]
pub fn action_log(state: State<'_, AppState>) -> Vec<ActionLog> {
    state.action_log.lock().unwrap().iter().cloned().collect()
}

/// Answers an approval request with `once`, `always`, or `deny`.
#[tauri::command]
pub fn approval_answer(app: AppHandle, id: u64, answer: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let request = state
        .pending
        .lock()
        .unwrap()
        .remove(&id)
        .ok_or_else(|| format!("no approval request {id} is waiting"))?;
    let grant = match answer.as_str() {
        "once" => Grant::Once,
        "always" => Grant::Always,
        "deny" => {
            log_action(
                &app,
                ActionLog {
                    at_ms: now_ms(),
                    page: request.page,
                    tile: request.tile,
                    kind: "command".into(),
                    status: "denied".into(),
                    detail: request.spec.program,
                    pid: None,
                    exit_code: None,
                },
            );
            return Ok(());
        }
        other => {
            state.pending.lock().unwrap().insert(id, request);
            return Err(format!("unknown answer {other:?}"));
        }
    };
    let saved = state.approvals.lock().unwrap().grant(&request.hash, grant);
    let why = match grant {
        Grant::Once => "allowed once",
        Grant::Always => "always allowed",
    };
    run_command(&app, request.page, request.tile, request.spec, why);
    saved.map_err(|error| format!("ran, but the approval was not saved: {error}"))
}

/// Brings the window to the front without sending a key, so mirrored keys reach the key
/// monitor. Returns whether the window is in front.
#[tauri::command]
pub fn focus_window(window: tauri::WebviewWindow, webview: tauri::Webview) -> bool {
    #[cfg(windows)]
    {
        let front = window
            .hwnd()
            .is_ok_and(|hwnd| td_host::bring_to_front(hwnd.0 as isize));
        // Focus WebView2 itself. Focusing only the native frame leaves document.hasFocus()
        // false after another window takes focus. Webview::set_focus sends no fallback key.
        let focused = webview.set_focus().is_ok();
        front && focused
    }
    #[cfg(not(windows))]
    {
        let _ = webview;
        let _ = window.set_focus();
        window.is_focused().unwrap_or(false)
    }
}

/// Drives the mock deck's events. Each entry is a `key.report` with `type` and `mods` and
/// `keys` or `usage`, a `key.resync` with `mods` and `keys`, a `press` with `page`, `tile`,
/// and `kind`, or `{"m":"drop","count":n}`, which loses `n` key reports. The list runs
/// `repeat` times with `every_ms` between entries, on its own thread.
#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_emit(
    state: State<'_, AppState>,
    events: Vec<Value>,
    repeat: Option<u32>,
    every_ms: Option<u64>,
) -> Result<(), String> {
    let mock = state
        .mock
        .clone()
        .ok_or("the app is not running the mock deck")?;
    let every = Duration::from_millis(every_ms.unwrap_or(0));
    std::thread::spawn(move || {
        for _ in 0..repeat.unwrap_or(1) {
            for event in &events {
                let text = |name: &str| event.get(name).and_then(Value::as_str).unwrap_or_default();
                let number = |name: &str| event.get(name).and_then(Value::as_u64).unwrap_or(0);
                match text("m") {
                    "key.report" => {
                        mock.emit_key(event);
                    }
                    "key.resync" => {
                        let keys: Vec<u8> = event
                            .get("keys")
                            .and_then(Value::as_array)
                            .map(|keys| {
                                keys.iter()
                                    .filter_map(Value::as_u64)
                                    .map(|key| key as u8)
                                    .collect()
                            })
                            .unwrap_or_default();
                        mock.emit_resync(number("mods") as u8, &keys);
                    }
                    "press" => {
                        mock.emit_press(text("page"), text("tile"), text("kind"));
                    }
                    "drop" => mock.drop_key_reports(number("count") as u32),
                    _ => {}
                }
                std::thread::sleep(every);
            }
        }
    });
    Ok(())
}

/// The key events the fake input sink recorded, oldest first, as `down <name>` or `up <name>`.
#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_input_log(state: State<'_, AppState>) -> Option<Vec<String>> {
    state.fake_input.as_ref().map(|fake| {
        fake.events
            .lock()
            .unwrap()
            .iter()
            .map(|event| match event {
                td_host::KeyEvent::Down(code) => format!("down {}", td_host::keymap::name(*code)),
                td_host::KeyEvent::Up(code) => format!("up {}", td_host::keymap::name(*code)),
            })
            .collect()
    })
}

#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_mock_press(
    state: State<'_, AppState>,
    page: String,
    tile: String,
) -> Result<(), String> {
    state
        .mock
        .as_ref()
        .ok_or("the app is not running the mock deck")?
        .emit_press(&page, &tile, "widget");
    Ok(())
}

#[cfg(feature = "e2e")]
#[tauri::command]
pub fn e2e_mock_sets(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    Ok(state
        .mock
        .as_ref()
        .ok_or("the app is not running the mock deck")?
        .sets())
}

#[tauri::command]
pub async fn update_firmware(
    app: AppHandle,
    bytes: Vec<u8>,
    port: String,
    recover: bool,
) -> Result<(), String> {
    blocking(move || {
        let _transfer = app.state::<AppUpdateGate>().begin_transfer()?;
        let bundle = td_link::bundle::Bundle::parse(&bytes)?;
        let state = app.state::<AppState>();
        if state.updating.swap(true, Ordering::AcqRel) {
            return Err("firmware update already in progress".into());
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let _reset = Reset(&state.updating);
        app.state::<SourcesState>().disconnected();
        if let Some(link) = state.link.lock().unwrap().take() {
            link.session.close();
        }
        let mut connector: Box<dyn td_link::update::Connector> = match &state.mock {
            Some(mock) if port == MOCK_PORT => Box::new(mock.clone()),
            Some(_) => return Err("select the mock port".into()),
            None => Box::new(td_link::update::PortConnector(&port)),
        };
        let mut flasher = td_link::recover::RomRecovery(&port);
        let recovery = if recover {
            if state.mock.is_some() {
                return Err("ROM recovery requires real hardware".into());
            }
            Some(&mut flasher as &mut dyn td_link::recover::Recovery)
        } else {
            None
        };
        td_link::update::run(
            &mut *connector,
            recovery,
            &bundle,
            td_link::update::Deadlines::default(),
            &mut |progress| {
                let _ = app.emit("firmware-update", progress);
            },
        )
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_reconnect_replaces_previous_actions_and_source_bindings() {
        let state = AppState::new(true);
        let sources = SourcesState::new(true);
        let previous = parse_layout(r#"{
            "schema":1,"name":"Previous session","orientation":"landscape",
            "pages":[{"id":"fkeys","title":"Fixture","kind":"grid","rows":1,"cols":2,
                "tiles":[{"id":"f1","label":"Never run","row":0,"col":0,"rowSpan":1,"colSpan":1,
                    "icon":null,"action":{"type":"command","program":"never-run-fixture","args":[],"cwd":"","shell":false}},
                    {"id":"feed","label":"Context","row":0,"col":1,"rowSpan":1,"colSpan":1,
                    "icon":null,"action":{"type":"widget","source":"claude-status","field":"context_pct","account":""}}]}]}
        "#).unwrap();
        bind_deck_layout(&state, &sources, previous);
        assert!(matches!(
            actions::find_tile(
                &state.action_layout.lock().unwrap().as_ref().unwrap().0,
                "fkeys",
                "f1"
            )
            .map(|tile| Plan::of(&tile.action)),
            Some(Plan::Command(_))
        ));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while sources.status().layout.widgets != 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let deck = MockDeck::new();
        let (session, _incoming) =
            Session::open(Box::new(deck.connect()), SessionConfig::default()).unwrap();
        let summary =
            refresh_deck_bindings(&state, &sources, &session).expect("bind the displayed builtin");
        assert_eq!(summary.name, "Built-in");
        assert_eq!(
            actions::find_tile(
                &state.action_layout.lock().unwrap().as_ref().unwrap().0,
                "fkeys",
                "f1"
            )
            .map(|tile| Plan::of(&tile.action)),
            Some(Plan::Deck)
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while sources.status().layout.name.as_deref() != Some("Built-in") {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(sources.status().layout.widgets, 0);
        assert_eq!(sources.status().layout.dashboards, 0);
    }
}
