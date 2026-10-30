//! Live values for Live tiles. The Companion reads the Integration streams that connected Devices' Layouts bind and
//! sends each Device what changed over the link, never through the API ([ADR 0004](../../../../../docs/adr/0004-firmware-draws-live-tiles.md)).

mod router;

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use td_host::integrations::{Delivery, LiveSample};

pub use router::{Binding, LiveCaps, Report, Router, StreamKey, bindings};

use crate::devices::{DeviceManager, LinkState};
use crate::features::integrations::Service;
use crate::link::Links;

/// How often the router looks for new samples. A sample reaches its Devices within one round.
const ROUND: Duration = Duration::from_millis(250);
/// How long a round waits for a Device whose link is busy, such as with a Layout push, before it tries next round.
const LINK_WAIT: Duration = Duration::from_millis(50);

#[derive(Default)]
struct Shared {
    router: Mutex<Router>,
    /// Devices that showed a new Layout since the last round.
    changed: Mutex<BTreeSet<String>>,
    /// The newest samples read, kept for rounds when the Integrations are busy.
    samples: Mutex<HashMap<StreamKey, LiveSample>>,
    /// Requests each Device accepted per Integration, for the Integration's delivery status.
    sent: Mutex<HashMap<(String, String), u64>>,
    #[cfg(any(test, feature = "test-hooks"))]
    injected: Mutex<HashMap<StreamKey, LiveSample>>,
}

#[derive(Clone)]
pub struct Live(Arc<Shared>);

impl Live {
    pub fn start(devices: DeviceManager, links: Links, integrations: Service) -> Live {
        let live = Live(Arc::default());
        let shared = Arc::downgrade(&live.0);
        thread::spawn(move || run(shared, devices, links, integrations));
        live
    }

    /// The Device now shows another Layout, so its bindings are read again and every value goes again.
    pub fn layout_changed(&self, device: &str) {
        self.0.changed.lock().insert(device.to_owned());
    }

    /// Adds a sample as if an Integration reported it, for tests without real Integration values.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn inject(&self, key: StreamKey, sample: LiveSample) {
        self.0.injected.lock().insert(key, sample);
    }
}

fn run(shared: Weak<Shared>, devices: DeviceManager, links: Links, integrations: Service) {
    loop {
        thread::sleep(ROUND);
        let Some(shared) = shared.upgrade() else {
            break;
        };
        round(&shared, &devices, &links, &integrations);
    }
}

fn round(shared: &Shared, devices: &DeviceManager, links: &Links, integrations: &Service) {
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
    let wanted = {
        let mut router = shared.router.lock();
        router.retain(&connected.iter().map(|(id, ..)| id.clone()).collect());
        for device in std::mem::take(&mut *shared.changed.lock()) {
            router.layout_changed(&device);
        }
        router.wanted()
    };
    if let Some(read) = integrations.live(&wanted) {
        *shared.samples.lock() = read;
    }
    let samples = shared.samples.lock().clone();
    #[cfg(any(test, feature = "test-hooks"))]
    let samples = {
        let mut samples = samples;
        samples.extend(shared.injected.lock().clone());
        samples
    };
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
