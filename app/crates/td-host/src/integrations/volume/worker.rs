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
struct Binding {
    generation: Option<u64>,
    next: Instant,
    retry: Duration,
}
impl Binding {
    fn current(&self, queue: &Queue) -> bool {
        self.generation == Some(queue.notifier.generation.load(Ordering::Acquire))
            && !queue.stop.load(Ordering::Acquire)
    }
    fn record(&mut self, state: &Mutex<State>, generation: u64, result: Result<Snapshot, Failure>) {
        if result.is_ok() {
            self.generation = Some(generation);
            self.retry = Duration::from_secs(1);
            self.next = Instant::now() + HEARTBEAT;
        } else {
            self.generation = None;
            self.next = Instant::now() + self.retry;
            self.retry = (self.retry * 2).min(Duration::from_secs(4));
        }
        publish(state, result);
    }
}
fn run(factory: ProviderFactory, state: Arc<Mutex<State>>, queue: Queue, rx: Receiver<Message>) {
    let mut provider = None;
    let mut binding = Binding {
        generation: None,
        next: Instant::now(),
        retry: Duration::from_secs(1),
    };
    let mut message = None;
    let mut observed_generation = queue.notifier.generation.load(Ordering::Acquire);
    while !queue.stop.load(Ordering::Acquire) {
        let flags = queue.notifier.flags.swap(0, Ordering::AcqRel);
        let generation = queue.notifier.generation.load(Ordering::Acquire);
        let topology_changed = generation != observed_generation || flags & REBIND != 0;
        observed_generation = generation;
        if topology_changed
            || Instant::now() >= binding.next
            || (flags & DIRTY != 0 && binding.generation.is_some())
        {
            let opened = if provider.is_none() {
                match factory(queue.notifier.clone()) {
                    Ok(opened) => {
                        provider = Some(opened);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            } else {
                Ok(())
            };
            let result = opened.and_then(|()| {
                if generation != queue.notifier.generation.load(Ordering::Acquire) {
                    return Err(Failure::Unavailable);
                }
                read(
                    &mut **provider.as_mut().unwrap(),
                    topology_changed || binding.generation.is_none(),
                )
            });
            if generation != queue.notifier.generation.load(Ordering::Acquire) {
                continue;
            }
            binding.record(&state, generation, result);
        }
        if queue.stop.load(Ordering::Acquire) {
            break;
        }
        if generation != queue.notifier.generation.load(Ordering::Acquire) {
            continue;
        }
        if let Some(Message::Mute {
            generation: expected,
            deadline,
            result,
        }) = message.take()
        {
            let outcome =
                if expected != generation || Instant::now() >= deadline || !binding.current(&queue)
                {
                    Err(Failure::Unavailable)
                } else {
                    let provider = provider.as_mut().unwrap();
                    let snapshot = read(&mut **provider, false);
                    if let Err(error) = snapshot {
                        binding.record(&state, generation, Err(error));
                    }
                    snapshot.and_then(|snapshot| {
                        if Instant::now() >= deadline || !binding.current(&queue) {
                            Err(Failure::Unavailable)
                        } else {
                            let outcome = provider.set_mute(!snapshot.muted);
                            if binding.current(&queue) {
                                let readback = outcome.and_then(|()| read(&mut **provider, false));
                                if binding.current(&queue) {
                                    binding.record(&state, generation, readback);
                                }
                            }
                            outcome
                        }
                    })
                };
            let _ = result.try_send(outcome.map_err(Failure::control));
        }
        if generation != queue.notifier.generation.load(Ordering::Acquire) {
            continue;
        }
        if binding.current(&queue) {
            let pending = {
                let mut pending = queue.pending.lock().unwrap();
                if binding.current(&queue)
                    && pending.is_some_and(|(expected, _)| expected <= generation)
                {
                    pending.take()
                } else {
                    None
                }
            };
            if let Some((expected, value)) = pending
                && Some(expected) == binding.generation
                && binding.current(&queue)
            {
                let provider = provider.as_mut().unwrap();
                let result = provider.set_level(value).and_then(|()| {
                    if value > 0.0 && binding.current(&queue) {
                        provider.set_mute(false)
                    } else {
                        Ok(())
                    }
                });
                if binding.current(&queue) {
                    let readback = result.and_then(|()| read(&mut **provider, false));
                    if binding.current(&queue) {
                        binding.record(&state, generation, readback);
                    }
                }
            }
        }
        if generation != queue.notifier.generation.load(Ordering::Acquire) {
            continue;
        }
        message = rx
            .recv_timeout(binding.next.saturating_duration_since(Instant::now()))
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
