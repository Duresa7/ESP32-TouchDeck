use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use td_core::{Hello, Layout, Orientation, Request, Settings};
use td_link::discover::{Candidate, Probe, discover, probe_transport};
use td_link::mock::MOCK_PORT;
use td_link::xfer::{PutReport, get_layout, put_layout};
use td_link::{Incoming, MockDeck, Serial, Session, SessionConfig, Transport};

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
}

impl AppState {
    pub fn new(mock: bool) -> AppState {
        AppState {
            mock: mock.then(MockDeck::new),
            link: Mutex::new(None),
            sessions: AtomicU64::new(0),
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
        let state = app.state::<AppState>();
        let connected = state.connected();
        let ports: Vec<String> = match &state.mock {
            Some(_) => vec![MOCK_PORT.to_owned()],
            None => td_link::discover::candidate_ports(),
        };
        if state.mock.is_none() && connected.is_none() {
            return discover();
        }
        ports
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
            .collect()
    })
    .await
}

#[tauri::command]
pub async fn connect(app: AppHandle, port: String) -> Result<DeckInfo, String> {
    blocking(move || {
        let state = app.state::<AppState>();
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
        let session = Arc::new(session);
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
        std::thread::spawn(move || forward(&forwarder, &session, &incoming, closed));
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
) {
    for item in incoming {
        match item {
            Incoming::Line(line) => {
                let _ = app.emit("console-line", line);
            }
            Incoming::Event(event) => {
                let _ = app.emit("deck-event", event);
            }
            Incoming::Closed(reason) => {
                let state = app.state::<AppState>();
                let mut link = state.link.lock().unwrap();
                if link.as_ref().is_some_and(|link| link.id == closed.session) {
                    link.take();
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

/// Validates a layout with the schema rules and pushes its canonical form into the deck's
/// slot for its own orientation.
#[tauri::command]
pub async fn push_layout(app: AppHandle, layout: String) -> Result<PutReport, String> {
    blocking(move || {
        let parsed = Layout::parse(layout.as_bytes()).map_err(|errors| {
            let all: Vec<String> = errors.iter().map(ToString::to_string).collect();
            all.join("; ")
        })?;
        put_layout(&*session(&app)?, parsed.orientation, &parsed).map_err(|error| error.to_string())
    })
    .await?
}

/// Pulls the layout the deck stores for `orientation` and returns its text.
#[tauri::command]
pub async fn pull_layout(app: AppHandle, orientation: Orientation) -> Result<String, String> {
    blocking(move || {
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
        "stop" => mock.stop(),
        "hang-up" => mock.hang_up(),
        other => return Err(format!("unknown mock action {other:?}")),
    }
    Ok(())
}
