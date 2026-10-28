use super::{ClientInfo, CommonError, Ctx};
use axum::{
    Extension,
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};
pub trait Event {
    const NAME: &'static str;
}
#[derive(Default)]
struct Queue {
    frames: VecDeque<String>,
    bytes: usize,
    overflow: bool,
}
struct Subscriber {
    queue: Mutex<Queue>,
    notify: tokio::sync::Notify,
}
#[derive(Default, Clone)]
pub struct Events {
    subscribers: Arc<Mutex<Vec<Weak<Subscriber>>>>,
}
impl Events {
    pub fn publish(&self, value: serde_json::Value) {
        let frame = value.to_string();
        self.subscribers.lock().unwrap().retain(|weak| {
            let Some(s) = weak.upgrade() else {
                return false;
            };
            let mut q = s.queue.lock().unwrap();
            if q.frames.len() >= 1024 || q.bytes + frame.len() > 4 * 1024 * 1024 {
                q.overflow = true;
            } else {
                q.bytes += frame.len();
                q.frames.push_back(frame.clone());
            }
            s.notify.notify_one();
            true
        });
    }
    pub fn notify(&self) {
        for s in self
            .subscribers
            .lock()
            .unwrap()
            .iter()
            .filter_map(Weak::upgrade)
        {
            s.notify.notify_one();
        }
    }
    fn subscribe(&self) -> Arc<Subscriber> {
        let s = Arc::new(Subscriber {
            queue: Mutex::new(Queue::default()),
            notify: tokio::sync::Notify::new(),
        });
        self.subscribers.lock().unwrap().push(Arc::downgrade(&s));
        s
    }
}
struct Lease {
    ctx: Ctx,
    id: String,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.ctx.auth.lock().unwrap().socket_close(&self.id);
    }
}
pub(super) async fn upgrade(
    State(ctx): State<Ctx>,
    Extension(client): Extension<ClientInfo>,
    ws: WebSocketUpgrade,
) -> Result<Response, CommonError> {
    ctx.auth.lock().unwrap().socket_open(&client.id)?;
    let lease = Lease {
        ctx: ctx.clone(),
        id: client.id.clone(),
    };
    // Only `touchdeck.v1` is selected, so the bearer entry is never echoed.
    Ok(ws
        .protocols(["touchdeck.v1"])
        .max_message_size(4096)
        .on_upgrade(move |socket| async move {
            let _lease = lease;
            serve(socket, ctx, client).await;
        }))
}
fn frame(mut value: serde_json::Value, seq: &mut u64) -> Message {
    value["seq"] = (*seq).into();
    *seq += 1;
    Message::Text(value.to_string().into())
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
    let hello = json!({"type": "hello", "api": "v1", "build": env!("TD_BUILD"), "session": client});
    let sync = json!({"type": "sync", "data": snapshot});
    for value in [hello, sync] {
        if sink.send(frame(value, &mut seq)).await.is_err() {
            return;
        }
    }
    let mut shutdown = ctx.shutdown.subscribe();
    let mut ping = tokio::time::interval(Duration::from_secs(20));
    ping.tick().await;
    let mut missed = 0;
    let code = loop {
        if !ctx.auth.lock().unwrap().active(&client.id) {
            let revoked = json!({"type": "auth.revoked", "data": {}});
            let _ = sink.send(frame(revoked, &mut seq)).await;
            break 4401;
        }
        let next = {
            let mut queue = subscriber.queue.lock().unwrap();
            if queue.overflow {
                break 4008;
            }
            let next = queue.frames.pop_front();
            if let Some(f) = &next {
                queue.bytes -= f.len();
            }
            next
        };
        if let Some(next) = next {
            let value = serde_json::from_str(&next).unwrap();
            let sent =
                tokio::time::timeout(Duration::from_secs(2), sink.send(frame(value, &mut seq)));
            if !matches!(sent.await, Ok(Ok(()))) {
                break 4008;
            }
            continue;
        }
        tokio::select! {
            _ = subscriber.notify.notified() => {}
            _ = shutdown.changed() => {
                let stopping = json!({"type": "companion.stopping", "data": {"reason": "shutdown"}});
                let _ = sink.send(frame(stopping, &mut seq)).await;
                break 1001;
            }
            _ = ping.tick() => {
                if missed >= 2 {
                    break 1001;
                }
                missed += 1;
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break 1001;
                }
            }
            message = stream.next() => match message {
                Some(Ok(Message::Pong(_))) => missed = 0,
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break 1000,
                _ => {}
            },
        }
    };
    let close = Message::Close(Some(CloseFrame {
        code,
        reason: "".into(),
    }));
    let _ = tokio::time::timeout(Duration::from_secs(2), sink.send(close)).await;
}
