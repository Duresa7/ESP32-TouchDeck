use super::{ClientInfo, CommonError, Ctx, EventInfo};
use axum::{
    Extension,
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket, rejection::WebSocketUpgradeRejection},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
    time::Duration,
};
use ts_rs::TS;

/// A frame on the `state` topic. The type is the frame's `data`.
pub trait Event: Serialize + TS + 'static {
    const NAME: &'static str;
    /// The Device the frame is about, sent as `device`.
    fn device(&self) -> Option<&str> {
        None
    }
}
/// The first frame. Its fields sit at the top level of the frame, not under `data`.
#[derive(Serialize, Deserialize, TS)]
pub struct Hello {
    pub api: String,
    pub build: String,
    pub session: ClientInfo,
}
impl Event for Hello {
    const NAME: &'static str = "hello";
}
/// The second frame: one key per feature snapshot.
#[derive(Serialize, Deserialize, TS)]
#[ts(rename = "SyncFrame")]
pub struct Sync(pub serde_json::Map<String, serde_json::Value>);
impl Event for Sync {
    const NAME: &'static str = "sync";
}
#[derive(Serialize, Deserialize, TS)]
pub struct AuthRevoked {}
impl Event for AuthRevoked {
    const NAME: &'static str = "auth.revoked";
}
#[derive(Serialize, Deserialize, TS)]
pub struct CompanionStopping {
    pub reason: String,
}
impl Event for CompanionStopping {
    const NAME: &'static str = "companion.stopping";
}
pub(super) fn kernel_events() -> Vec<EventInfo> {
    let feature = super::Feature::new("kernel")
        .event::<Hello>()
        .event::<Sync>()
        .event::<AuthRevoked>()
        .event::<CompanionStopping>();
    feature.events
}
fn frame<E: Event>(event: &E) -> serde_json::Value {
    let mut frame = json!({ "type": E::NAME, "data": event });
    if let Some(device) = event.device() {
        frame["device"] = device.into();
    }
    frame
}

#[derive(Default)]
struct Queue {
    frames: VecDeque<(serde_json::Value, usize)>,
    bytes: usize,
    overflow: bool,
}
struct Subscriber {
    queue: Mutex<Queue>,
    notify: tokio::sync::Notify,
}
/// Fans `state` frames out to every open socket, each with its own bounded queue.
#[derive(Default, Clone)]
pub struct Events {
    subscribers: Arc<Mutex<Vec<Weak<Subscriber>>>>,
}
impl Events {
    pub fn publish<E: Event>(&self, event: &E) {
        self.publish_frame(frame(event));
    }
    pub(crate) fn publish_frame(&self, frame: serde_json::Value) {
        let size = frame.to_string().len();
        self.subscribers.lock().retain(|weak| {
            let Some(s) = weak.upgrade() else {
                return false;
            };
            let mut q = s.queue.lock();
            if q.frames.len() >= 1024 || q.bytes + size > 4 * 1024 * 1024 {
                q.overflow = true;
            } else {
                q.bytes += size;
                q.frames.push_back((frame.clone(), size));
            }
            s.notify.notify_one();
            true
        });
    }
    /// Wakes every socket so it rechecks its session.
    pub fn notify(&self) {
        for s in self.subscribers.lock().iter().filter_map(Weak::upgrade) {
            s.notify.notify_one();
        }
    }
    fn subscribe(&self) -> Arc<Subscriber> {
        let s = Arc::new(Subscriber {
            queue: Mutex::new(Queue::default()),
            notify: tokio::sync::Notify::new(),
        });
        self.subscribers.lock().push(Arc::downgrade(&s));
        s
    }
}
struct Lease {
    ctx: Ctx,
    id: String,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let now = self.ctx.clock.now();
        self.ctx.auth.lock().socket_close(&self.id, now);
    }
}
pub(super) async fn upgrade(
    State(ctx): State<Ctx>,
    Extension(client): Extension<ClientInfo>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Result<Response, CommonError> {
    #[cfg(any(test, feature = "test-hooks"))]
    ctx.handler_hits
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let ws = ws.map_err(|e| CommonError::invalid("upgrade", e.body_text()))?;
    ctx.auth.lock().socket_open(&client.id)?;
    let lease = Lease {
        ctx: ctx.clone(),
        id: client.id.clone(),
    };
    // Only `touchdeck.v1` is selected, so the bearer entry is never echoed.
    Ok(ws
        .protocols(["touchdeck.v1"])
        .max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| async move {
            let _lease = lease;
            serve(socket, ctx, client).await;
        }))
}
fn numbered(mut value: serde_json::Value, seq: &mut u64) -> Message {
    value["seq"] = (*seq).into();
    *seq += 1;
    Message::Text(value.to_string().into())
}
/// Every send waits at most this long. A reader that stalls longer loses its socket.
const SEND_TIMEOUT: Duration = Duration::from_secs(2);
type Sink = futures_util::stream::SplitSink<WebSocket, Message>;
async fn send(sink: &mut Sink, message: Message) -> bool {
    matches!(
        tokio::time::timeout(SEND_TIMEOUT, sink.send(message)).await,
        Ok(Ok(()))
    )
}
async fn serve(socket: WebSocket, ctx: Ctx, client: ClientInfo) {
    let subscriber = ctx.events.subscribe();
    let (mut sink, mut stream) = socket.split();
    let mut seq = 0u64;
    let mut snapshot = serde_json::Map::new();
    for feature in super::registry() {
        for entry in feature.snapshots {
            snapshot.insert(entry.key.into(), (entry.read)(&ctx));
        }
    }
    let mut hello = serde_json::to_value(Hello {
        api: "v1".into(),
        build: env!("TD_BUILD").into(),
        session: client.clone(),
    })
    .unwrap();
    hello["type"] = Hello::NAME.into();
    let sync = frame(&Sync(snapshot));
    for value in [hello, sync] {
        if !send(&mut sink, numbered(value, &mut seq)).await {
            return;
        }
    }
    let mut shutdown = ctx.shutdown.subscribe();
    let mut ping = tokio::time::interval(Duration::from_secs(20));
    ping.tick().await;
    let mut missed = 0;
    // `Some(code)` closes with that code. `None` drops the connection: a send already failed,
    // so a close frame would stall too.
    let close: Option<u16> = loop {
        if !ctx.auth.lock().active(&client.id) {
            let revoked = numbered(frame(&AuthRevoked {}), &mut seq);
            break send(&mut sink, revoked).await.then_some(4401);
        }
        let next = {
            let mut queue = subscriber.queue.lock();
            if queue.overflow {
                break Some(4008);
            }
            let next = queue.frames.pop_front();
            if let Some((_, size)) = &next {
                queue.bytes -= size;
            }
            next
        };
        if let Some((value, _)) = next {
            if !send(&mut sink, numbered(value, &mut seq)).await {
                break None;
            }
            continue;
        }
        tokio::select! {
            _ = subscriber.notify.notified() => {}
            _ = shutdown.changed() => {
                let stopping = frame(&CompanionStopping { reason: "shutdown".into() });
                break send(&mut sink, numbered(stopping, &mut seq)).await.then_some(1001);
            }
            _ = ping.tick() => {
                if missed >= 2 {
                    break Some(1001);
                }
                missed += 1;
                if !send(&mut sink, Message::Ping(Vec::new().into())).await {
                    break None;
                }
            }
            message = stream.next() => match message {
                Some(Ok(Message::Pong(_))) => missed = 0,
                Some(Ok(Message::Close(_))) => break Some(1000),
                // A protocol error, such as a frame over the size limit, or a closed stream.
                None | Some(Err(_)) => break None,
                _ => {}
            },
        }
    };
    if let Some(code) = close {
        let frame = CloseFrame {
            code,
            reason: "".into(),
        };
        send(&mut sink, Message::Close(Some(frame))).await;
    }
}
