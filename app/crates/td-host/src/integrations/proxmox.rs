use super::*;
use crate::sources::{
    Reading,
    network::{FetchError, GetRequest, MAX_BODY, parse_pin},
};
use serde_json::json;
use std::time::{Duration, Instant, SystemTime};
use td_core::{layout::TileRender, live::LiveValue};

const FIELDS: [&str; 2] = ["token_id", "token_secret"];
const INVALID: &str = "Couldn't read the Proxmox response. Check the server.";
const LARGE: &str = "Proxmox sent more than 1 MiB. Check the server.";
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
    url: String,
    interval_s: u64,
    #[serde(default)]
    tls_pin: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "https://example.invalid:8006".into(),
            interval_s: 15,
            tls_pin: String::new(),
        }
    }
}
impl Settings {
    fn validate(mut self) -> Result<Self, Error> {
        self.url = self.url.trim().trim_end_matches('/').to_owned();
        let origin = secrets::origin(&self.url).map_err(|_| Error::ConfigInvalid)?;
        if self.url != origin {
            return Err(Error::ConfigInvalid);
        }
        secrets::credential_transport(&self.url).map_err(|_| Error::ConfigInvalid)?;
        self.interval_s = self.interval_s.clamp(5, 3600);
        self.tls_pin = self.tls_pin.trim().to_ascii_lowercase();
        if !self.tls_pin.is_empty() && parse_pin(&self.tls_pin).is_none() {
            return Err(Error::ConfigInvalid);
        }
        Ok(self)
    }
}
#[derive(Deserialize)]
struct Resources {
    data: Vec<Resource>,
}
#[derive(Deserialize)]
struct Resource {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    status: Option<String>,
    cpu: Option<f64>,
    mem: Option<f64>,
    maxmem: Option<f64>,
}
struct ValueSample {
    account: String,
    stream: &'static str,
    value: f64,
}
struct PollData {
    values: Vec<ValueSample>,
    resources: Vec<Resource>,
}
fn parse(body: &str) -> Result<PollData, &'static str> {
    if body.len() > MAX_BODY {
        return Err(LARGE);
    }
    let reply: Resources = serde_json::from_str(body).map_err(|_| INVALID)?;
    let mut resources: Vec<_> = reply
        .data
        .into_iter()
        .filter(|r| matches!(r.kind.as_str(), "node" | "qemu" | "lxc"))
        .collect();
    resources.sort_by(|a, b| a.id.cmp(&b.id));
    let mut seen = std::collections::BTreeSet::new();
    let mut samples = Vec::new();
    let mut running = 0;
    let mut total = 0;
    for r in &resources {
        let prefix = format!("{}/", r.kind);
        if !r.id.starts_with(&prefix)
            || r.id.len() <= prefix.len()
            || r.id.len() > 128
            || !r
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"/._-".contains(&c))
            || !seen.insert(&r.id)
        {
            return Err(INVALID);
        }
        let online = match (r.kind.as_str(), r.status.as_deref()) {
            ("node", Some("online")) | ("qemu" | "lxc", Some("running")) => true,
            ("node", Some("offline")) | ("qemu" | "lxc", Some("stopped")) => false,
            _ => return Err(INVALID),
        };
        let mut add = |stream, value| {
            samples.push(ValueSample {
                account: r.id.clone(),
                stream,
                value,
            })
        };
        if r.kind != "node" {
            total += 1;
            running += usize::from(online);
            add("running", f64::from(online));
        }
        if online {
            let cpu = r
                .cpu
                .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 1.0)
                .ok_or(INVALID)?;
            let mem = r
                .mem
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or(INVALID)?;
            let max = r
                .maxmem
                .filter(|n| n.is_finite() && *n > 0.0 && mem <= *n)
                .ok_or(INVALID)?;
            add("cpu_pct", cpu * 100.0);
            add("ram_pct", mem / max * 100.0);
        }
    }
    for (stream, value) in [("running_guests", running), ("total_guests", total)] {
        samples.push(ValueSample {
            account: "cluster".into(),
            stream,
            value: value as f64,
        });
    }
    Ok(PollData {
        values: samples,
        resources,
    })
}
struct Proxmox {
    settings: Settings,
    file: Option<PathBuf>,
    secrets: Arc<dyn secrets::SecretStore>,
    fetch: Box<dyn crate::sources::Fetch>,
    next: Option<Instant>,
    failures: u32,
    samples: u64,
    readings: Vec<Reading>,
    live: Vec<LiveSample>,
    resources: Vec<Resource>,
    message: Option<String>,
    delivery: BTreeMap<String, Delivery>,
}
impl Proxmox {
    fn credential(&self, field: &str) -> Result<Option<String>, Error> {
        let origin = secrets::origin(&self.settings.url).map_err(|_| Error::ConfigInvalid)?;
        Ok(self
            .secrets
            .get("proxmox", field)
            .map_err(|_| Error::Credentials)?
            .filter(|s| s.origin == origin)
            .map(|s| s.value))
    }
    fn clear(&mut self) {
        self.readings.clear();
        self.live.clear();
        self.resources.clear();
        self.message = None;
    }
    fn poll(&mut self, now: Instant) {
        if !self.settings.enabled || self.next.is_some_and(|n| now < n) {
            return;
        }
        let result = self.collect();
        self.samples = self.samples.saturating_add(1);
        match result {
            Ok(Some(PollData { values, resources })) => {
                self.failures = 0;
                self.message = None;
                self.resources = resources;
                let updated = crate::sources::unix_ms(SystemTime::now());
                self.readings = values
                    .iter()
                    .map(|v| {
                        Reading::new(
                            "proxmox",
                            &v.account,
                            v.stream,
                            format!("{:.1}", v.value),
                            if v.stream.ends_with("_pct") {
                                v.value / 100.0
                            } else {
                                -1.0
                            },
                            updated,
                            false,
                        )
                    })
                    .collect();
                self.live = values
                    .into_iter()
                    .map(|v| LiveSample {
                        account: v.account,
                        stream: v.stream.into(),
                        sample: self.samples,
                        ttl_ms: (self.settings.interval_s * 3000).clamp(1000, 60000) as u32,
                        value: LiveValue::Number(v.value),
                    })
                    .collect();
            }
            Ok(None) => self.clear(),
            Err(message) => {
                self.failures = self.failures.saturating_add(1);
                self.message = Some(message.into());
                self.live.clear();
                self.readings.clear();
            }
        }
        let delay = self
            .settings
            .interval_s
            .saturating_mul(1 << self.failures.min(10))
            .min(3600);
        self.next = Some(now + Duration::from_secs(delay));
    }
    fn collect(&self) -> Result<Option<PollData>, &'static str> {
        let id = self
            .credential("token_id")
            .map_err(|_| "Credential store unavailable. Retry after unlocking it.")?;
        let secret = self
            .credential("token_secret")
            .map_err(|_| "Credential store unavailable. Retry after unlocking it.")?;
        let (Some(id), Some(secret)) = (id, secret) else {
            return Ok(None);
        };
        let body = self.fetch.request(&GetRequest { url:&format!("{}/api2/json/cluster/resources",self.settings.url), authorization:&format!("PVEAPIToken={id}={secret}"), tls_pin:parse_pin(&self.settings.tls_pin) }).map_err(|e| match e {
            FetchError::Unauthorized => "Proxmox rejected the token. Check its read-only permissions.",
            FetchError::Timeout => "Proxmox didn't answer within 10 s. Check the connection, then retry.",
            FetchError::TooLarge => LARGE,
            FetchError::Tls => "Couldn't verify the Proxmox certificate. Check the SHA-256 pin or system trust store.",
            FetchError::Transport => "Couldn't reach Proxmox. Check the address and connection, then retry.",
        })?;
        parse(&body).map(Some)
    }
    fn presets(&self) -> Vec<PresetTile> {
        let mut presets = Vec::new();
        let mut add = |account: &str, field: &str, name: &str, graph: bool| {
            presets.push(PresetTile {
                id: format!("preset_{}", presets.len()),
                name: name.into(),
                action: json!({"type":"widget","source":"proxmox","account":account,"field":field}),
                render: graph.then_some(TileRender::Graph),
            })
        };
        for r in self.resources.iter().filter(|r| r.kind == "node") {
            for (field, name, graph) in [
                ("cpu_pct", "Node CPU graph", true),
                ("cpu_pct", "Node CPU", false),
                ("ram_pct", "Node RAM graph", true),
                ("ram_pct", "Node RAM", false),
            ] {
                add(&r.id, field, name, graph);
            }
        }
        for r in self.resources.iter().filter(|r| r.kind != "node") {
            add(&r.id, "running", "Guest status", false);
        }
        add("cluster", "running_guests", "Running guests", false);
        presets
    }
}
impl Integration for Proxmox {
    fn summary(&self) -> IntegrationSummary {
        let credentials: Result<Vec<_>, _> = FIELDS.iter().map(|f| self.credential(f)).collect();
        let (state, message) = match credentials {
            Err(_) => (
                "error",
                Some("Credential store unavailable. Retry after unlocking it.".into()),
            ),
            Ok(fields) if fields.iter().any(Option::is_none) => ("not_configured", None),
            _ => (
                if self.message.is_some() {
                    "error"
                } else {
                    "ok"
                },
                self.message.clone(),
            ),
        };
        IntegrationSummary {
            revision: self.samples,
            id: "proxmox".into(),
            name: "Proxmox".into(),
            icon: "server".into(),
            enabled: self.settings.enabled,
            status: IntegrationStatus {
                state: state.into(),
                message,
                delivery: self.delivery.values().cloned().collect(),
            },
            config_schema: [
                ("enabled", "boolean", false),
                ("url", "string", false),
                ("interval_s", "integer", false),
                ("tls_pin", "string", false),
                ("token_id", "string", true),
                ("token_secret", "string", true),
            ]
            .into_iter()
            .map(|(name, kind, secret)| ConfigField {
                name: name.into(),
                kind: kind.into(),
                secret,
            })
            .collect(),
            streams: [
                ("cpu_pct", "%"),
                ("ram_pct", "%"),
                ("running", "boolean"),
                ("running_guests", "guests"),
                ("total_guests", "guests"),
            ]
            .into_iter()
            .map(|(id, unit)| DataStream {
                id: id.into(),
                kind: "numeric".into(),
                unit: Some(unit.into()),
            })
            .collect(),
            actions: vec!["refresh".into()],
            presets: self.presets(),
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: serde_json::to_value(&self.settings).map_err(|_| Error::ConfigInvalid)?,
            secrets: FIELDS
                .iter()
                .map(|f| {
                    Ok((
                        (*f).into(),
                        SecretState {
                            set: self.credential(f)?.is_some(),
                        },
                    ))
                })
                .collect::<Result<_, Error>>()?,
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let settings = serde_json::from_value::<Settings>(config)
            .map_err(|_| Error::ConfigInvalid)?
            .validate()?;
        if settings != self.settings {
            if secrets::origin(&settings.url) != secrets::origin(&self.settings.url) {
                for field in FIELDS {
                    self.secrets
                        .delete("proxmox", field)
                        .map_err(|_| Error::Credentials)?;
                }
            }
            if let Some(file) = &self.file {
                crate::sources::write_atomic(
                    file,
                    &serde_json::to_vec(&settings).map_err(|_| Error::Storage)?,
                )
                .map_err(|_| Error::Storage)?;
            }
            self.settings = settings;
            self.clear();
        }
        self.view()
    }
    fn secret_put(&mut self, field: &str, value: &str) -> Result<(), Error> {
        if !FIELDS.contains(&field)
            || value.is_empty()
            || value.len() > 4096
            || !value.bytes().all(|b| b.is_ascii_graphic() && b != b'=')
        {
            return Err(Error::ConfigInvalid);
        }
        self.secrets
            .put(
                "proxmox",
                field,
                secrets::Secret {
                    origin: secrets::origin(&self.settings.url)
                        .map_err(|_| Error::ConfigInvalid)?,
                    value: value.into(),
                },
            )
            .map_err(|_| Error::Credentials)?;
        self.clear();
        Ok(())
    }
    fn secret_delete(&mut self, field: &str) -> Result<(), Error> {
        if !FIELDS.contains(&field) {
            return Err(Error::ConfigInvalid);
        }
        self.secrets
            .delete("proxmox", field)
            .map_err(|_| Error::Credentials)?;
        self.clear();
        Ok(())
    }
    fn refresh(&mut self) {
        self.tick();
    }
    fn tick(&mut self) {
        self.poll(Instant::now());
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
fn load(env: &Environment) -> Result<Proxmox, Error> {
    let file = env
        .config_file
        .as_ref()
        .map(|p| p.with_file_name("proxmox.json"));
    let settings = if let Some(file) = &file {
        match std::fs::read(file) {
            Ok(bytes) => serde_json::from_slice::<Settings>(&bytes)
                .map_err(|_| Error::ConfigInvalid)?
                .validate()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(_) => return Err(Error::Storage),
        }
    } else {
        Settings::default()
    };
    Ok(Proxmox {
        settings,
        file,
        secrets: env.secrets.clone(),
        fetch: (env.fetch)(),
        next: None,
        failures: 0,
        samples: 0,
        readings: Vec::new(),
        live: Vec::new(),
        resources: Vec::new(),
        message: None,
        delivery: BTreeMap::new(),
    })
}
fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    Ok(Box::new(load(env)?))
}
inventory::submit! {Registration(create)}

#[cfg(test)]
mod tests {
    use super::*;
    struct Recorded;
    impl crate::sources::Fetch for Recorded {
        fn get(&self, _: &str, _: &str) -> Result<String, String> {
            unreachable!()
        }
        fn post_json(&self, _: &str, _: &str, _: &Value) -> Result<String, String> {
            unreachable!()
        }
        fn request(&self, _: &GetRequest<'_>) -> Result<String, FetchError> {
            Ok(include_str!("../../tests/fixtures/proxmox-resources.json").into())
        }
    }
    #[test]
    fn repeated_values_advance_samples_and_configuration_cannot_bypass_deadlines() {
        let mut integration = load(&Environment {
            secrets: Arc::new(secrets::MemorySecrets::default()),
            config_file: None,
            fetch: Arc::new(|| Box::new(Recorded)),
        })
        .unwrap();
        integration.configure(json!({"enabled":true,"url":"https://example.invalid:8006","interval_s":0,"tls_pin":""})).unwrap();
        for f in FIELDS {
            integration.secret_put(f, "fixture-private-value").unwrap();
        }
        let now = Instant::now();
        integration.poll(now);
        let before = integration.live();
        assert!(!before.is_empty());
        integration.poll(now + Duration::from_secs(5));
        let after = integration.live();
        assert_eq!(before[0].value, after[0].value);
        assert!(after[0].sample > before[0].sample);
        integration.configure(json!({"enabled":false,"url":"https://example.invalid:8006","interval_s":0,"tls_pin":""})).unwrap();
        assert!(integration.live().is_empty());
        integration.configure(json!({"enabled":true,"url":"https://example.invalid:8006","interval_s":999999,"tls_pin":""})).unwrap();
        integration.tick();
        assert!(integration.live().is_empty());
        assert_eq!(integration.view().unwrap().config["interval_s"], 3600);
    }
    #[test]
    fn metrics_and_config_reject_invalid_input_without_echoing_it() {
        for body in [
            r#"{"data":[{"type":"node","id":"node/example.invalid","status":"online","cpu":2,"mem":1,"maxmem":4}]}"#,
            r#"{"data":[{"type":"qemu","id":"qemu/100","status":"unknown"}]}"#,
            r#"{"data":[{"type":"node","id":"node/example.invalid","status":"online","cpu":0.5,"mem":1,"maxmem":0}]}"#,
        ] {
            assert!(parse(body).is_err());
        }
        for (url, pin) in [
            ("https://example.invalid/path", ""),
            ("https://example.invalid?token=redacted", ""),
            ("https://example.invalid", "bad-pin"),
            ("http://example.invalid", ""),
        ] {
            assert!(
                Settings {
                    url: url.into(),
                    tls_pin: pin.into(),
                    ..Settings::default()
                }
                .validate()
                .is_err()
            );
        }
    }
}
