use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use td_core::Hello;
use td_link::discover::{Candidate, Probe, discover, probe_transport};
use td_link::mock::MOCK_PORT;
use td_link::{Incoming, MockDeck, Serial, Session, SessionConfig, Transport};

use crate::sources::SourcesState;

const HELLO_TIMEOUT: Duration = Duration::from_secs(1);

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
                if event["m"] == "press"
                    && let Some(tile) = event["tile"].as_str()
                {
                    app.state::<SourcesState>().pressed(tile);
                }
                let _ = app.emit("deck-event", event);
            }
            Incoming::Closed(reason) => {
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
    blocking(move || {
        let link = app.state::<AppState>().link.lock().unwrap().take();
        if let Some(link) = link {
            app.state::<SourcesState>().disconnected();
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
        .press(&page, &tile, "widget");
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
