//! Live values for Live tiles. The Companion reads the Integration streams that connected Devices' Layouts bind and
//! sends each Device what changed over the link, never through the API ([ADR 0004](../../../../../docs/adr/0004-firmware-draws-live-tiles.md)).

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

use crate::devices::{ControlLayout, ControlLayoutUnavailable, DeviceManager, LinkState};
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
    generation: std::sync::atomic::AtomicU64,
    publication: Mutex<()>,
    devices: std::sync::OnceLock<DeviceManager>,
    settable: std::sync::OnceLock<crate::actions::controls::Settable>,
    trusted: std::sync::OnceLock<crate::store::LayoutStore>,
    wake: Mutex<bool>,
    woken: Condvar,
    router: Mutex<Router>,
    /// Devices that showed a new Layout since the last round.
    changed: Mutex<BTreeSet<String>>,
    shown: Mutex<HashMap<String, (u64, Arc<Layout>)>>,
    /// The newest polled samples, kept for rounds when the Integrations are busy.
    samples: Mutex<HashMap<StreamKey, LiveSample>>,
    covers: Mutex<Prepared>,
    /// Requests each Device accepted per Integration, for the Integration's delivery status.
    sent: Mutex<HashMap<(String, String), u64>>,
    #[cfg(any(test, feature = "test-hooks"))]
    injected: Mutex<HashMap<StreamKey, LiveSample>>,
    #[cfg(any(test, feature = "test-hooks"))]
    rounds: std::sync::atomic::AtomicU64,
}

impl Shared {
    fn assess(&self, device: &str, session: u64) -> (ControlLayout, Option<Layout>) {
        let changed = self.changed.lock();
        if changed.contains(device) {
            return (ControlLayout::default(), None);
        }
        let layout = match self.shown.lock().get(device) {
            Some((read_in, layout)) if *read_in == session => layout.clone(),
            _ => return (ControlLayout::default(), None),
        };
        let orientation = layout.orientation;
        let has_controls = layout.pages.iter().any(|page| {
            let td_core::layout::Page::Grid { tiles, .. } = page else {
                return false;
            };
            tiles.iter().any(|tile| {
                ["previous", "play_pause", "next", "mute"]
                    .iter()
                    .any(|part| td_host::actions::Plan::part(tile, part).is_some())
                    || tile.streams.iter().flatten().any(|(stream, _)| {
                        crate::actions::controls::takes_input(tile, stream)
                            && stream_source(tile, stream).is_some_and(|source| {
                                self.settable
                                    .get()
                                    .is_some_and(|settable| settable(&source))
                            })
                    })
            })
        });
        let Some(trusted) = self
            .trusted
            .get()
            .and_then(|store| store.get(device, layout.orientation))
        else {
            return (
                ControlLayout::NoPushedCopy {
                    orientation,
                    has_controls,
                },
                None,
            );
        };
        if trusted.to_canonical_json() != layout.to_canonical_json() {
            return (
                ControlLayout::Differs {
                    orientation,
                    has_controls,
                },
                None,
            );
        }
        (
            ControlLayout::Matches {
                orientation,
                has_controls,
            },
            Some(trusted),
        )
    }
    fn wake(&self) {
        *self.wake.lock() = true;
        self.woken.notify_one();
    }
}

#[derive(Clone)]
pub struct Live(Arc<Shared>);

impl Live {
    pub(crate) fn cover_assets(&self, device: &str) -> BTreeSet<String> {
        self.0.router.lock().cover_assets(device)
    }

    pub(crate) fn forget_cover(&self, device: &str, id: &str) {
        self.0.router.lock().forget_cover(device, id);
    }

    pub fn start(devices: DeviceManager, links: Links, integrations: Service) -> Live {
        let live = Live(Arc::default());
        let _ = live.0.devices.set(devices.clone());
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
        let _publication = self.0.publication.lock();
        let mut changed = self.0.changed.lock();
        changed.insert(device.to_owned());
        self.0
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.0.shown.lock().remove(device);
        drop(changed);
        if let Some(devices) = self.0.devices.get() {
            let session = devices
                .summaries()
                .into_iter()
                .find(|d| d.id.as_str() == device)
                .and_then(|d| match d.link {
                    LinkState::Connected { since_ms, .. } => Some(since_ms),
                    _ => None,
                });
            devices.control_layout(device, session, ControlLayout::default());
        }
        self.0.wake();
    }

    pub(crate) fn generation(&self) -> u64 {
        self.0.generation.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) fn settable(&self, predicate: crate::actions::controls::Settable) {
        let _ = self.0.settable.set(predicate);
    }

    pub(crate) fn trust(&self, layouts: crate::store::LayoutStore) {
        let _ = self.0.trusted.set(layouts);
    }

    /// Returns a Tile from the Companion's pushed copy only when the active Layout matches in this session.
    /// Returns None during replacement or before a matching Layout has been read. Never waits for a link.
    pub fn tile(&self, device: &str, session: u64, page: &str, tile: &str) -> Option<Tile> {
        self.binding(device, session, page, tile).ok().flatten()
    }

    pub(crate) fn binding(
        &self,
        device: &str,
        session: u64,
        page: &str,
        tile: &str,
    ) -> Result<Option<Tile>, ()> {
        let (_, trusted) = self.0.assess(device, session);
        let trusted = trusted.ok_or(())?;
        Ok(td_host::actions::find_tile(&trusted, page, tile).cloned())
    }

    /// Adds a sample as if an Integration reported it, for tests without real Integration values.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn inject(&self, key: StreamKey, sample: LiveSample) {
        self.0.injected.lock().insert(key, sample);
        self.0.wake();
    }

    /// How many live rounds have finished.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn rounds(&self) -> u64 {
        self.0.rounds.load(std::sync::atomic::Ordering::SeqCst)
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
        #[cfg(any(test, feature = "test-hooks"))]
        shared
            .rounds
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = jobs.try_send(covers);
    }
}

fn round(
    shared: &Shared,
    devices: &DeviceManager,
    links: &Links,
    integrations: &Service,
) -> Vec<CoverJob> {
    let generation = shared.generation.load(std::sync::atomic::Ordering::SeqCst);
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
    let changed = {
        let mut changed = shared.changed.lock();
        let mut shown = shared.shown.lock();
        for device in changed.iter() {
            shown.remove(device);
        }
        std::mem::take(&mut *changed)
    };
    let wanted = {
        let mut router = shared.router.lock();
        router.set_covers(covers);
        router.set_off(
            integrations
                .summaries()
                .into_iter()
                .filter(|summary| !summary.enabled)
                .map(|summary| summary.id)
                .collect(),
        );
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
    publish_shown(shared, shown);
    publish_status(shared, devices, generation);
    jobs
}

fn publish_status(shared: &Shared, devices: &DeviceManager, generation: u64) {
    let _publication = shared.publication.lock();
    if shared.generation.load(std::sync::atomic::Ordering::SeqCst) != generation {
        return;
    }
    for device in devices.summaries() {
        let (session, status) = match device.link {
            LinkState::Connected {
                since_ms,
                protocol: 2,
                ..
            } if device
                .capabilities
                .as_ref()
                .and_then(LiveCaps::of)
                .is_some() =>
            {
                let mut status = shared.assess(&device.id, since_ms).0;
                if matches!(status, ControlLayout::Unavailable { .. })
                    && shared.router.lock().read_failed(&device.id, since_ms)
                {
                    status = ControlLayout::Unavailable {
                        reason: ControlLayoutUnavailable::ReadFailed,
                    };
                }
                (Some(since_ms), status)
            }
            LinkState::Connected { since_ms, .. } => (
                Some(since_ms),
                ControlLayout::Unavailable {
                    reason: ControlLayoutUnavailable::Unsupported,
                },
            ),
            _ => (
                None,
                ControlLayout::Unavailable {
                    reason: ControlLayoutUnavailable::Offline,
                },
            ),
        };
        devices.control_layout(&device.id, session, status);
    }
}

fn publish_shown(shared: &Shared, mut shown: HashMap<String, (u64, Arc<Layout>)>) {
    let changed = shared.changed.lock();
    shown.retain(|device, _| !changed.contains(device));
    *shared.shown.lock() = shown;
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
