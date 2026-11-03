//! Live values for Live tiles. The Companion reads the Integration streams that connected Devices' Layouts bind and
//! sends each Device what changed over the link, never through the API ([ADR 0004](../../../../../docs/adr/0004-firmware-draws-live-tiles.md)).
//!
//! Three threads share this state. The live thread runs a delivery round every 250 ms, or at once when a Control
//! wakes it or a Device shows a new Layout. The cover thread turns Control pictures into cover JPEGs without holding
//! any lock, so encoding never delays the link. Controls call `wake` from their own threads.
//!
//! Lock order: a Device's link, then `router`. `wake`, `changed`, `shown`, `samples`, `covers`, `sent` and
//! `injected` are each taken alone, never while waiting for another lock. Polled samples come from the registry with
//! `try_lock` only, and Control samples without the registry at all.
//!
//! A Device's taps and drags find their Tile through `Live::tile`, which reads `changed` and then `shown`, each
//! alone. It never takes `router` or a link, so a round that holds them for a cover upload never delays a drag.

mod covers;
mod router;

use std::collections::{BTreeSet, HashMap};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use td_core::Layout;
use td_core::layout::Tile;
use td_core::live::LiveValue;
use td_host::integrations::{Delivery, LiveSample};

pub use router::{Binding, LiveCaps, Report, Router, StreamKey, bindings, stream_source};

use crate::devices::{DeviceManager, LinkState};
use crate::features::integrations::{Controls, Service};
use crate::link::Links;
use covers::CoverCache;
use router::{CoverJob, Prepared};

/// How often the router looks for new samples when nothing wakes it. A polled sample reaches its Devices within one
/// round.
const ROUND: Duration = Duration::from_millis(250);
/// How long a round waits for a Device whose link is busy, such as with a Layout push, before it tries next round.
const LINK_WAIT: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Shared {
    wake: Mutex<bool>,
    woken: Condvar,
    router: Mutex<Router>,
    /// Devices that showed a new Layout since the last round.
    changed: Mutex<BTreeSet<String>>,
    /// The Layout each Device showed when the router last read it, with the link session it was read in. A Device
    /// in `changed` has none until a round reads its new Layout.
    shown: Mutex<HashMap<String, (u64, Arc<Layout>)>>,
    /// The newest polled samples, kept for rounds when the Integrations are busy.
    samples: Mutex<HashMap<StreamKey, LiveSample>>,
    covers: Mutex<Prepared>,
    /// Requests each Device accepted per Integration, for the Integration's delivery status.
    sent: Mutex<HashMap<(String, String), u64>>,
    #[cfg(any(test, feature = "test-hooks"))]
    injected: Mutex<HashMap<StreamKey, LiveSample>>,
}

impl Shared {
    fn wake(&self) {
        *self.wake.lock() = true;
        self.woken.notify_one();
    }
}

#[derive(Clone)]
pub struct Live(Arc<Shared>);

impl Live {
    pub fn start(devices: DeviceManager, links: Links, integrations: Service) -> Live {
        let live = Live(Arc::default());
        live.0
            .router
            .lock()
            .set_controls(integrations.controls.clone());
        for control in integrations.controls.values() {
            let shared = Arc::downgrade(&live.0);
            control.subscribe(Arc::new(move || {
                if let Some(shared) = shared.upgrade() {
                    shared.wake();
                }
            }));
        }
        let (jobs, queue) = sync_channel(1);
        let shared = Arc::downgrade(&live.0);
        let controls = integrations.controls.clone();
        thread::spawn(move || make_covers(shared, controls, queue));
        let shared = Arc::downgrade(&live.0);
        thread::spawn(move || run(shared, devices, links, integrations, jobs));
        live
    }

    /// The Device now shows another Layout, so its bindings are read again and every value goes again.
    pub fn layout_changed(&self, device: &str) {
        self.0.changed.lock().insert(device.to_owned());
        self.0.shown.lock().remove(device);
        self.0.wake();
    }

    /// A Tile of the Layout the Device showed in link session `session` (its `since_ms`), as the router last read
    /// it. None while the router hasn't read that Layout, such as just after a push. Never waits for a link.
    pub fn tile(&self, device: &str, session: u64, page: &str, tile: &str) -> Option<Tile> {
        if self.0.changed.lock().contains(device) {
            return None;
        }
        let layout = match self.0.shown.lock().get(device) {
            Some((read_in, layout)) if *read_in == session => layout.clone(),
            _ => return None,
        };
        td_host::actions::find_tile(&layout, page, tile).cloned()
    }

    /// Adds a sample as if an Integration reported it, for tests without real Integration values.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn inject(&self, key: StreamKey, sample: LiveSample) {
        self.0.injected.lock().insert(key, sample);
        self.0.wake();
    }
}

fn run(
    shared: Weak<Shared>,
    devices: DeviceManager,
    links: Links,
    integrations: Service,
    jobs: SyncSender<Vec<CoverJob>>,
) {
    loop {
        let Some(shared) = shared.upgrade() else {
            break;
        };
        {
            let mut wake = shared.wake.lock();
            if !*wake {
                shared.woken.wait_for(&mut wake, ROUND);
            }
            *wake = false;
        }
        let covers = round(&shared, &devices, &links, &integrations);
        let _ = jobs.try_send(covers);
    }
}

fn round(
    shared: &Shared,
    devices: &DeviceManager,
    links: &Links,
    integrations: &Service,
) -> Vec<CoverJob> {
    let connected: Vec<(String, u64, LiveCaps)> = devices
        .summaries()
        .into_iter()
        .filter_map(|device| match device.link {
            LinkState::Connected {
                since_ms,
                protocol: 2,
                ..
            } => {
                let caps = LiveCaps::of(device.capabilities.as_ref()?)?;
                Some((device.id.as_str().to_owned(), since_ms, caps))
            }
            _ => None,
        })
        .collect();
    let covers = shared.covers.lock().clone();
    let changed = std::mem::take(&mut *shared.changed.lock());
    {
        let mut shown = shared.shown.lock();
        for device in &changed {
            shown.remove(device);
        }
    }
    let wanted = {
        let mut router = shared.router.lock();
        router.set_covers(covers);
        router.retain(&connected.iter().map(|(id, ..)| id.clone()).collect());
        for device in changed {
            router.layout_changed(&device);
        }
        router.wanted()
    };
    if let Some(read) = integrations.live(&wanted) {
        *shared.samples.lock() = read;
    }
    let mut samples = shared.samples.lock().clone();
    samples.extend(integrations.control_live(&wanted));
    #[cfg(any(test, feature = "test-hooks"))]
    samples.extend(shared.injected.lock().clone());
    for (device, session, caps) in connected {
        let delivered = links.within(&device, LINK_WAIT, |link| {
            shared
                .router
                .lock()
                .deliver(&device, session, link, caps, &samples, Instant::now())
        });
        if let Some(Ok(report)) = delivered {
            record(shared, integrations, &device, report);
        }
    }
    let (jobs, shown) = {
        let router = shared.router.lock();
        (router.cover_jobs(&samples), router.shown())
    };
    *shared.shown.lock() = shown;
    jobs
}

fn make_covers(shared: Weak<Shared>, controls: Controls, queue: Receiver<Vec<CoverJob>>) {
    let mut cache = CoverCache::default();
    while let Ok(jobs) = queue.recv() {
        let mut made = Prepared::new();
        for job in jobs {
            let Some(control) = controls.get(&job.source.integration) else {
                continue;
            };
            let LiveValue::Image(Some(source)) = &job.sample.value else {
                continue;
            };
            let still_reported = || control.live().contains(&job.sample);
            let cover = match cache.get(source, job.side) {
                Some(cover) => cover.ok(),
                None if !still_reported() => continue,
                None => control
                    .picture(source)
                    .and_then(|bytes| cache.prepare(source, &bytes, job.side).ok()),
            };
            if still_reported() {
                made.insert((job.source, job.sample.sample, job.side), cover);
            }
        }
        let Some(shared) = shared.upgrade() else {
            break;
        };
        let mut covers = shared.covers.lock();
        if *covers != made {
            *covers = made;
            drop(covers);
            shared.wake();
        }
    }
}

fn record(shared: &Shared, integrations: &Service, device: &str, report: Report) {
    let touched: BTreeSet<&String> = report.sent.keys().chain(report.errors.keys()).collect();
    for integration in touched {
        let sent = {
            let mut totals = shared.sent.lock();
            let total = totals
                .entry((integration.clone(), device.to_owned()))
                .or_default();
            *total += report.sent.get(integration).copied().unwrap_or(0);
            *total
        };
        integrations.delivered(
            integration,
            Delivery {
                device: device.to_owned(),
                sent,
                error: report.errors.get(integration).cloned(),
            },
        );
    }
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
