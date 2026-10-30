use super::*;
use crate::sources::Reading;
use serde_json::json;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use td_core::layout::TileRender;

#[cfg(windows)]
mod windows;

/// How long a Device shows a This PC value without a newer one before it draws it stale.
const LIVE_TTL_MS: u32 = 5_000;
const STREAMS: [(&str, &str, &str); 10] = [
    ("cpu_pct", "CPU usage", "%"),
    ("ram_pct", "RAM usage", "%"),
    ("ram_used_bytes", "RAM used", "bytes"),
    ("ram_total_bytes", "RAM total", "bytes"),
    ("gpu_pct", "GPU usage", "%"),
    ("disk_active_pct", "Disk activity", "%"),
    ("disk_read_bytes_s", "Disk read", "bytes/s"),
    ("disk_write_bytes_s", "Disk write", "bytes/s"),
    ("network_receive_bytes_s", "Network receive", "bytes/s"),
    ("network_send_bytes_s", "Network send", "bytes/s"),
];

#[derive(Default)]
pub struct CounterSample {
    pub values: BTreeMap<String, f64>,
    pub totals: BTreeMap<String, u64>,
}
pub trait CounterSource: Send {
    fn collect(&mut self) -> CounterSample;
    fn supported(&self) -> bool {
        true
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
}

pub struct ThisPc {
    source: Box<dyn CounterSource>,
    settings: Settings,
    file: Option<PathBuf>,
    next: Option<Instant>,
    previous: Option<(Instant, BTreeMap<String, u64>)>,
    readings: Vec<Reading>,
    live: Vec<LiveSample>,
    samples: u64,
    unavailable: Vec<&'static str>,
    delivery: BTreeMap<String, Delivery>,
}
impl ThisPc {
    pub fn new(source: Box<dyn CounterSource>) -> Self {
        Self {
            source,
            settings: Settings { enabled: true },
            file: None,
            next: None,
            previous: None,
            readings: Vec::new(),
            live: Vec::new(),
            samples: 0,
            unavailable: Vec::new(),
            delivery: BTreeMap::new(),
        }
    }
    pub fn sample_at(&mut self, now: Instant) {
        if !self.settings.enabled || self.next.is_some_and(|next| now < next) {
            return;
        }
        let sample = self.source.collect();
        let mut values = sample.values;
        self.unavailable = STREAMS
            .iter()
            .filter(|(id, _, _)| !values.contains_key(*id) && !sample.totals.contains_key(*id))
            .map(|(_, name, _)| *name)
            .collect();
        if let Some((before, totals)) = &self.previous {
            let seconds = now.duration_since(*before).as_secs_f64();
            for (id, total) in &sample.totals {
                if let Some(delta) = totals.get(id).and_then(|old| total.checked_sub(*old)) {
                    values.insert(id.clone(), delta as f64 / seconds);
                }
            }
        }
        self.previous = Some((now, sample.totals));
        let updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let current: Vec<(&str, &str, f64)> = STREAMS
            .iter()
            .filter_map(|(id, _, unit)| {
                let value = *values.get(*id)?;
                if !value.is_finite() || value < 0.0 {
                    return None;
                }
                let value = if *unit == "%" {
                    value.min(100.0)
                } else {
                    value
                };
                Some((*id, *unit, value))
            })
            .collect();
        self.samples += 1;
        self.readings = current
            .iter()
            .map(|(id, unit, value)| {
                Reading::new(
                    "this_pc",
                    "",
                    id,
                    format!("{value:.1} {unit}"),
                    if *unit == "%" { value / 100.0 } else { -1.0 },
                    updated,
                    false,
                )
            })
            .collect();
        self.live = current
            .iter()
            .map(|(id, _, value)| LiveSample {
                account: String::new(),
                stream: (*id).to_owned(),
                sample: self.samples,
                ttl_ms: LIVE_TTL_MS,
                value: td_core::live::LiveValue::Number(*value),
            })
            .collect();
        self.next = Some(now + Duration::from_secs(1));
    }
}
impl Integration for ThisPc {
    fn summary(&self) -> IntegrationSummary {
        let message = if !self.source.supported() {
            Some("This PC needs Windows.".into())
        } else if !self.unavailable.is_empty() {
            Some(format!("Unavailable: {}.", self.unavailable.join(", ")))
        } else {
            None
        };
        IntegrationSummary {
            revision: 0,
            id: "this_pc".into(),
            name: "This PC".into(),
            icon: "monitor".into(),
            enabled: self.settings.enabled,
            status: IntegrationStatus {
                state: if message.is_some() { "error" } else { "ok" }.into(),
                message,
                delivery: self.delivery.values().cloned().collect(),
            },
            config_schema: vec![ConfigField {
                name: "enabled".into(),
                kind: "boolean".into(),
                secret: false,
            }],
            streams: STREAMS
                .iter()
                .map(|(id, _, unit)| DataStream {
                    id: (*id).into(),
                    kind: "numeric".into(),
                    unit: Some((*unit).into()),
                })
                .collect(),
            actions: vec!["refresh".into()],
            presets: STREAMS
                .iter()
                .map(|(id, name, _)| PresetTile {
                    id: (*id).into(),
                    name: (*name).into(),
                    action: json!({"type":"widget","source":"this_pc","field":id,"account":""}),
                    render: Some(TileRender::Graph),
                })
                .collect(),
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: json!({"enabled":self.settings.enabled}),
            secrets: BTreeMap::new(),
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        if settings != self.settings {
            if let Some(file) = &self.file {
                crate::sources::write_atomic(
                    file,
                    &serde_json::to_vec(&settings).map_err(|_| Error::Storage)?,
                )
                .map_err(|_| Error::Storage)?;
            }
            self.settings = settings;
            self.previous = None;
            self.next = None;
            self.readings.clear();
            self.live.clear();
            self.unavailable.clear();
        }
        self.view()
    }
    fn secret_put(&mut self, _: &str, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn secret_delete(&mut self, _: &str) -> Result<(), Error> {
        Err(Error::ConfigInvalid)
    }
    fn refresh(&mut self) {
        self.tick();
    }
    fn tick(&mut self) {
        self.sample_at(Instant::now());
    }
    fn readings(&self) -> Vec<Reading> {
        self.readings.clone()
    }
    fn live(&self) -> Vec<LiveSample> {
        self.live.clone()
    }
    fn delivered(&mut self, delivery: Delivery) {
        self.delivery.insert(delivery.device.clone(), delivery);
    }
}
#[cfg(not(windows))]
struct Unsupported;
#[cfg(not(windows))]
impl CounterSource for Unsupported {
    fn collect(&mut self) -> CounterSample {
        CounterSample::default()
    }
    fn supported(&self) -> bool {
        false
    }
}
fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    #[cfg(windows)]
    let source = Box::new(windows::Counters::new());
    #[cfg(not(windows))]
    let source = Box::new(Unsupported);
    let mut pc = ThisPc::new(source);
    pc.file = env
        .config_file
        .as_ref()
        .map(|file| file.with_file_name("this-pc.json"));
    if let Some(file) = &pc.file {
        match std::fs::read(file) {
            Ok(bytes) => {
                pc.settings = serde_json::from_slice(&bytes).map_err(|_| Error::ConfigInvalid)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Storage),
        }
    }
    Ok(Box::new(pc))
}
inventory::submit! { Registration(create) }
