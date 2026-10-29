//! Routes for the e2e harness. Compiled only with `test-hooks`, and they answer only when the
//! Companion runs on mock ports; otherwise every one is `route.not_found`.
use super::devices::{ConsoleLine, DeviceError, DevicePath};
use crate::{
    api::{CommonError, Ctx, Endpoint, Feature, NoQuery, Registration, Req},
    devices::{BoardKind, DeviceId, MockPorts},
};
use axum::http::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
use td_core::Orientation;
use td_link::MockDeck;
use ts_rs::TS;

fn mock_ports(ctx: &Ctx) -> Result<MockPorts, DeviceError> {
    let (ports, _) = ctx.devices.mock().ok_or(CommonError::NotFound)?;
    Ok(ports.clone())
}
fn mock_deck(ctx: &Ctx, id: &DeviceId) -> Result<MockDeck, DeviceError> {
    mock_ports(ctx)?
        .deck(id.as_str())
        .ok_or(DeviceError::NotFound)
}

pub struct Console;
impl Endpoint for Console {
    const NAME: &'static str = "test.console";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/devices/{device}/console";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ConsoleLine;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        mock_deck(&ctx, &req.path.device)?;
        let line = req.body.line;
        Ok(ctx
            .devices
            .call(&req.path.device, move |s| s.send_line(&line))
            .await??)
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct MockAction {
    /// `no-confirm`, `rollback`, `normal-update`, `stop` or `hang-up`.
    pub action: String,
}
pub struct Mock;
impl Endpoint for Mock {
    const NAME: &'static str = "test.mock";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/devices/{device}/mock";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = MockAction;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let deck = mock_deck(&ctx, &req.path.device)?;
        match req.body.action.as_str() {
            mode @ ("no-confirm" | "rollback" | "normal-update") => deck.update_mode(mode),
            "stop" => deck.stop(),
            "hang-up" => deck.hang_up(),
            _ => return Err(CommonError::invalid("body", "unknown mock action").into()),
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct MockPress {
    pub page: String,
    pub tile: String,
    pub kind: String,
}
pub struct Press;
impl Endpoint for Press {
    const NAME: &'static str = "test.press";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/devices/{device}/press";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = MockPress;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let press = req.body;
        mock_deck(&ctx, &req.path.device)?.emit_press(&press.page, &press.tile, &press.kind);
        Ok(())
    }
}

pub struct Sets;
impl Endpoint for Sets {
    const NAME: &'static str = "test.sets";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/test/devices/{device}/sets";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<Value>;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        Ok(mock_deck(&ctx, &req.path.device)?.sets())
    }
}

/// What a mock Device received and stores.
#[derive(Serialize, Deserialize, TS)]
pub struct MockRecord {
    pub received: Vec<Value>,
    pub lines: Vec<String>,
    pub settings: Value,
    pub reboots: u32,
    pub assets: Vec<String>,
    pub layouts: Value,
}
pub struct Record;
impl Endpoint for Record {
    const NAME: &'static str = "test.record";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/test/devices/{device}/record";
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = ();
    type Ok = MockRecord;
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let deck = mock_deck(&ctx, &req.path.device)?;
        let layout = |orientation| {
            deck.layout(orientation)
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        };
        Ok(MockRecord {
            received: deck.received(),
            lines: deck.lines(),
            settings: serde_json::to_value(deck.settings()).unwrap_or_default(),
            reboots: deck.reboots(),
            assets: deck.icons().into_iter().collect(),
            layouts: json!({
                "landscape": layout(Orientation::Landscape),
                "portrait": layout(Orientation::Portrait),
            }),
        })
    }
}

/// Events for a mock Device to send: a `key.report` with `type` and `mods` and `keys` or
/// `usage`, a `key.resync` with `mods` and `keys`, a `press` with `page`, `tile` and `kind`,
/// or `{"m": "drop", "count": n}`, which loses `n` key reports. The list runs `repeat` times
/// with `every_ms` between entries, on its own thread.
#[derive(Serialize, Deserialize, TS)]
pub struct MockEmit {
    pub events: Vec<Value>,
    #[serde(default)]
    pub repeat: Option<u32>,
    #[serde(default)]
    pub every_ms: Option<u64>,
}
pub struct Emit;
impl Endpoint for Emit {
    const NAME: &'static str = "test.emit";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/devices/{device}/emit";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = DevicePath;
    type Query = NoQuery;
    type Body = MockEmit;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let deck = mock_deck(&ctx, &req.path.device)?;
        let MockEmit {
            events,
            repeat,
            every_ms,
        } = req.body;
        let every = Duration::from_millis(every_ms.unwrap_or(0));
        std::thread::spawn(move || {
            for _ in 0..repeat.unwrap_or(1) {
                for event in &events {
                    emit(&deck, event);
                    std::thread::sleep(every);
                }
            }
        });
        Ok(())
    }
}
fn emit(deck: &MockDeck, event: &Value) {
    let text = |name: &str| event.get(name).and_then(Value::as_str).unwrap_or_default();
    let number = |name: &str| event.get(name).and_then(Value::as_u64).unwrap_or(0);
    match text("m") {
        "key.report" => {
            deck.emit_key(event);
        }
        "key.resync" => {
            let keys: Vec<u8> = event
                .get("keys")
                .and_then(Value::as_array)
                .map(|keys| {
                    keys.iter()
                        .filter_map(Value::as_u64)
                        .map(|k| k as u8)
                        .collect()
                })
                .unwrap_or_default();
            deck.emit_resync(number("mods") as u8, &keys);
        }
        "press" => {
            deck.emit_press(text("page"), text("tile"), text("kind"));
        }
        "drop" => deck.drop_key_reports(number("count") as u32),
        _ => {}
    }
}

pub struct InputLog;
impl Endpoint for InputLog {
    const NAME: &'static str = "test.input_log";
    const METHOD: Method = Method::GET;
    const PATH: &'static str = "/test/input-log";
    type Path = ();
    type Query = NoQuery;
    type Body = ();
    type Ok = Vec<String>;
    type Error = CommonError;
    async fn handle(ctx: Ctx, _: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let (_, sink) = ctx.devices.mock().ok_or(CommonError::NotFound)?;
        Ok(input_log(sink))
    }
}
/// The keys the fake sink typed, oldest first, as `down <name>` or `up <name>`.
pub fn input_log(sink: &td_host::FakeSink) -> Vec<String> {
    sink.events
        .lock()
        .unwrap()
        .iter()
        .map(|event| match event {
            td_host::KeyEvent::Down(code) => format!("down {}", td_host::keymap::name(*code)),
            td_host::KeyEvent::Up(code) => format!("up {}", td_host::keymap::name(*code)),
        })
        .collect()
}

#[derive(Serialize, Deserialize, TS)]
pub struct PlugBoard {
    pub id: DeviceId,
    pub kind: BoardKind,
}
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MockPortChange {
    Plug(PlugBoard),
    Unplug(DeviceId),
}
pub struct Ports;
impl Endpoint for Ports {
    const NAME: &'static str = "test.ports";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/ports";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = MockPortChange;
    type Ok = ();
    type Error = DeviceError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        let ports = mock_ports(&ctx)?;
        match req.body {
            MockPortChange::Plug(board) => ports.plug(board.id.as_str(), board.kind),
            MockPortChange::Unplug(id) => ports.unplug(id.as_str()),
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, TS)]
pub struct ClockAdvance {
    pub advance_ms: u64,
}
pub struct Clock;
impl Endpoint for Clock {
    const NAME: &'static str = "test.clock";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/clock";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = ClockAdvance;
    type Ok = ();
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<Self::Ok, Self::Error> {
        ctx.devices.mock().ok_or(CommonError::NotFound)?;
        ctx.clock
            .advance(Duration::from_millis(req.body.advance_ms));
        Ok(())
    }
}

fn feature() -> Feature {
    Feature::new("test")
        .endpoint::<Console>()
        .endpoint::<Mock>()
        .endpoint::<Press>()
        .endpoint::<Sets>()
        .endpoint::<Record>()
        .endpoint::<Emit>()
        .endpoint::<InputLog>()
        .endpoint::<Ports>()
        .endpoint::<Clock>()
        .endpoint::<ApprovalsScript>()
        .endpoint::<FirmwareScript>()
}
inventory::submit! { Registration(feature) }

#[derive(Serialize, Deserialize, TS)]
pub struct ApprovalScript {
    pub answers: Vec<crate::actions::ScriptAnswer>,
}
pub struct ApprovalsScript;
impl Endpoint for ApprovalsScript {
    const NAME: &'static str = "test.approvals_script";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/approvals/script";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = ApprovalScript;
    type Ok = ();
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<(), CommonError> {
        ctx.devices.mock().ok_or(CommonError::NotFound)?;
        if ctx.actions.script(req.body.answers) {
            Ok(())
        } else {
            Err(CommonError::NotFound)
        }
    }
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ConsentAnswer {
    Accept,
    Decline,
}
#[derive(Serialize, Deserialize, TS)]
pub struct ConsentScript {
    pub answers: Vec<ConsentAnswer>,
}
pub struct FirmwareScript;
impl Endpoint for FirmwareScript {
    const NAME: &'static str = "test.firmware_script";
    const METHOD: Method = Method::POST;
    const PATH: &'static str = "/test/firmware/script";
    const STATUS: StatusCode = StatusCode::NO_CONTENT;
    type Path = ();
    type Query = NoQuery;
    type Body = ConsentScript;
    type Ok = ();
    type Error = CommonError;
    async fn handle(ctx: Ctx, req: Req<Self>) -> Result<(), CommonError> {
        ctx.devices.mock().ok_or(CommonError::NotFound)?;
        let answers = req.body.answers;
        ctx.firmware.script(
            answers
                .into_iter()
                .map(|answer| matches!(answer, ConsentAnswer::Accept))
                .collect(),
        );
        Ok(())
    }
}
