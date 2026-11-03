use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const DIRTY: u8 = 1;
const REBIND: u8 = 2;
const HEARTBEAT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct Notifier {
    tx: SyncSender<Message>,
    flags: Arc<AtomicU8>,
    generation: Arc<AtomicU64>,
}
impl Notifier {
    /// Called by OS callbacks; does not wait or access COM objects.
    pub fn changed(&self) {
        self.flags.fetch_or(DIRTY, Ordering::Release);
        let _ = self.tx.try_send(Message::Wake);
    }
    /// Invalidates queued controls before notifying the worker to bind the new default endpoint.
    pub fn endpoint_changed(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.flags.fetch_or(DIRTY | REBIND, Ordering::Release);
        let _ = self.tx.try_send(Message::Wake);
    }
}
enum Message {
    Wake,
    Mute {
        generation: u64,
        deadline: Instant,
        result: SyncSender<Result<(), ControlError>>,
    },
}
#[derive(Clone)]
pub(super) struct Queue {
    notifier: Notifier,
    pending: Arc<Mutex<Option<(u64, f64)>>>,
    stop: Arc<AtomicBool>,
}
impl Queue {
    pub fn set(&self, value: f64) {
        *self.pending.lock().unwrap() =
            Some((self.notifier.generation.load(Ordering::Acquire), value));
        let _ = self.notifier.tx.try_send(Message::Wake);
    }
    pub fn refresh(&self) {
        self.notifier.changed();
    }
    pub fn mute(&self, timeout: Duration) -> Result<(), ControlError> {
        let timeout = timeout.min(Duration::from_secs(2));
        let (tx, rx) = mpsc::sync_channel(1);
        self.notifier
            .tx
            .try_send(Message::Mute {
                generation: self.notifier.generation.load(Ordering::Acquire),
                deadline: Instant::now() + timeout,
                result: tx,
            })
            .map_err(|_| Failure::Unavailable.control())?;
        rx.recv_timeout(timeout)
            .unwrap_or_else(|_| Err(Failure::Unavailable.control()))
    }
}
pub(super) struct Worker {
    queue: Queue,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(
        factory: ProviderFactory,
        state: Arc<Mutex<State>>,
    ) -> Result<(Self, Queue), Error> {
        let (tx, rx) = mpsc::sync_channel(16);
        let notifier = Notifier {
            tx,
            flags: Arc::new(AtomicU8::new(DIRTY | REBIND)),
            generation: Arc::new(AtomicU64::new(0)),
        };
        let queue = Queue {
            notifier,
            pending: Arc::default(),
            stop: Arc::new(AtomicBool::new(false)),
        };
        state.lock().unwrap().enabled = true;
        let worker_queue = queue.clone();
        let thread = thread::Builder::new()
            .name("volume".into())
            .spawn(move || run(factory, state, worker_queue, rx))
            .map_err(|_| Error::Storage)?;
        Ok((
            Self {
                queue: queue.clone(),
                thread: Some(thread),
            },
            queue,
        ))
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.queue.stop.store(true, Ordering::Release);
        let _ = self.queue.notifier.tx.try_send(Message::Wake);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn publish(state: &Mutex<State>, current: Result<Snapshot, Failure>) {
    let wake = {
        let mut state = state.lock().unwrap();
        if !state.enabled {
            return;
        }
        state.current = current;
        state.sample = state.sample.saturating_add(1);
        state.wake.clone()
    };
    if let Some(wake) = wake {
        wake();
    }
}
fn read(provider: &mut dyn VolumeProvider, rebind: bool) -> Result<Snapshot, Failure> {
    provider.read(rebind).and_then(|snapshot| {
        if snapshot.valid() {
            Ok(snapshot)
        } else {
            Err(Failure::Unavailable)
        }
    })
}
fn run(factory: ProviderFactory, state: Arc<Mutex<State>>, queue: Queue, rx: Receiver<Message>) {
    let mut provider = None;
    let mut next = Instant::now();
    let mut message = None;
    while !queue.stop.load(Ordering::Acquire) {
        let flags = queue.notifier.flags.swap(0, Ordering::AcqRel);
        if flags != 0 || Instant::now() >= next {
            if provider.is_none() {
                match factory(queue.notifier.clone()) {
                    Ok(opened) => provider = Some(opened),
                    Err(error) => publish(&state, Err(error)),
                }
            }
            let result = provider
                .as_mut()
                .map(|p| read(&mut **p, flags & REBIND != 0));
            let healthy = result.as_ref().is_some_and(Result::is_ok);
            if let Some(result) = result {
                publish(&state, result);
            }
            next = Instant::now()
                + if healthy {
                    HEARTBEAT
                } else {
                    Duration::from_secs(1)
                };
        }
        if queue.stop.load(Ordering::Acquire) {
            break;
        }
        if queue.notifier.flags.load(Ordering::Acquire) & REBIND != 0 {
            continue;
        }
        let generation = queue.notifier.generation.load(Ordering::Acquire);
        if let Some(Message::Mute {
            generation: expected,
            deadline,
            result,
        }) = message.take()
        {
            let outcome = if expected != generation || Instant::now() >= deadline {
                Err(Failure::Unavailable)
            } else if let Some(provider) = provider.as_mut() {
                read(&mut **provider, false).and_then(|snapshot| {
                    if Instant::now() >= deadline
                        || expected != queue.notifier.generation.load(Ordering::Acquire)
                    {
                        Err(Failure::Unavailable)
                    } else {
                        provider.set_mute(!snapshot.muted)
                    }
                })
            } else {
                Err(Failure::Unavailable)
            };
            if let Some(provider) = provider.as_mut() {
                publish(&state, read(&mut **provider, false));
            }
            let _ = result.try_send(outcome.map_err(Failure::control));
        }
        let pending = queue.pending.lock().unwrap().take();
        if let Some((expected, value)) = pending
            && expected == queue.notifier.generation.load(Ordering::Acquire)
            && !queue.stop.load(Ordering::Acquire)
            && let Some(provider) = provider.as_mut()
        {
            let result = provider.set_level(value).and_then(|()| {
                if value > 0.0 {
                    provider.set_mute(false)
                } else {
                    Ok(())
                }
            });
            publish(&state, result.and_then(|()| read(&mut **provider, false)));
        }
        message = rx
            .recv_timeout(next.saturating_duration_since(Instant::now()))
            .ok();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "changes the Windows playback volume briefly, then restores it"]
    fn real_os_volume_smoke() {
        struct Restore {
            provider: Box<dyn VolumeProvider>,
            original: Snapshot,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let level = self.provider.set_level(self.original.level);
                let mute = self.provider.set_mute(self.original.muted);
                assert!(level.is_ok() && mute.is_ok(), "could not restore volume");
            }
        }
        let (tx, rx) = mpsc::sync_channel(16);
        let notifier = Notifier {
            tx,
            flags: Arc::new(AtomicU8::new(0)),
            generation: Arc::new(AtomicU64::new(0)),
        };
        let mut provider = super::super::windows::open(notifier).unwrap();
        let original = provider.read(true).unwrap();
        let mut restore = Restore { provider, original };
        let target = if original.level > 0.99 {
            original.level - 0.01
        } else {
            original.level + 0.01
        };
        while rx.try_recv().is_ok() {}
        restore.provider.set_level(target).unwrap();
        rx.recv_timeout(Duration::from_secs(2))
            .expect("Core Audio notification");
        assert!((restore.provider.read(false).unwrap().level - target).abs() < 0.002);
        restore.provider.set_level(original.level).unwrap();
        restore.provider.set_mute(original.muted).unwrap();
        let restored = restore.provider.read(false).unwrap();
        assert!((restored.level - original.level).abs() < 0.002);
        assert_eq!(restored.muted, original.muted);
        println!("Core Audio read, 1% change, notification and restoration passed");
    }
}
