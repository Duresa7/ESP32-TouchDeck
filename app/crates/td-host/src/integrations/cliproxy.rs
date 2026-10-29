use super::*;
use crate::sources::{CliProxy, Config, Info, KeyStore, Source};
use secrets::BoundKeys;
use serde_json::json;
use std::time::Instant;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
    url: String,
    interval_s: u64,
}
struct CliProxyIntegration {
    settings: Settings,
    legacy: Config,
    file: Option<PathBuf>,
    keys: Arc<BoundKeys>,
    source: CliProxy,
    fetch: Arc<dyn Fn() -> Box<dyn crate::sources::Fetch> + Send + Sync>,
    next: Option<Instant>,
    last_request: Option<Instant>,
    readings: Vec<crate::sources::Reading>,
    delivery: BTreeMap<String, Delivery>,
}
fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    let legacy = env
        .config_file
        .as_ref()
        .map(|p| Config::load(p))
        .unwrap_or_default();
    let keys = Arc::new(BoundKeys::from_config(
        env.secrets.clone(),
        &legacy.cliproxy_url,
    ));
    Ok(Box::new(CliProxyIntegration {
        settings: Settings {
            enabled: legacy.cliproxy,
            url: legacy.cliproxy_url.clone(),
            interval_s: legacy.cliproxy_interval_s.max(60),
        },
        source: CliProxy::new(&legacy.cliproxy_url, keys.clone(), (env.fetch)()),
        legacy,
        file: env.config_file.clone(),
        keys,
        fetch: env.fetch.clone(),
        next: Some(Instant::now()),
        last_request: None,
        readings: Vec::new(),
        delivery: BTreeMap::new(),
    }))
}
inventory::submit! { Registration(create) }
impl CliProxyIntegration {
    fn reset(&mut self) {
        self.source = CliProxy::new(&self.settings.url, self.keys.clone(), (self.fetch)());
        self.next = Some(Instant::now());
        self.readings.clear();
    }
    fn field(field: &str) -> Result<(), Error> {
        if field == "management_key" {
            Ok(())
        } else {
            Err(Error::ConfigInvalid)
        }
    }
}
impl Integration for CliProxyIntegration {
    fn summary(&self) -> IntegrationSummary {
        let Info::Proxy(info) = self.source.info();
        let (state, message) = match self.keys.get() {
            Err(_) => ("error", Some("Credential store unavailable".into())),
            Ok(None) => ("not_configured", None),
            Ok(Some(_)) => {
                if info.ok == Some(false) {
                    ("error", info.error)
                } else {
                    ("ok", None)
                }
            }
        };
        let streams = [
            ("status", "text", None),
            ("success", "numeric", Some("requests")),
            ("failed", "numeric", Some("requests")),
            ("recent_10m", "numeric", Some("requests")),
            ("five_hour_pct", "numeric", Some("%")),
            ("seven_day_pct", "numeric", Some("%")),
            ("five_hour_reset", "text", None),
            ("seven_day_reset", "text", None),
            ("plan", "text", None),
        ]
        .into_iter()
        .map(|(id, kind, unit)| DataStream {
            id: id.into(),
            kind: kind.into(),
            unit: unit.map(str::to_owned),
        })
        .collect::<Vec<_>>();
        let presets = streams
            .iter()
            .map(|s| PresetTile {
                id: s.id.clone(),
                name: s.id.replace('_', " "),
                action: json!({"type":"widget","source":"cliproxyapi","field":s.id,"account":""}),
            })
            .collect();
        IntegrationSummary {
            id: "cliproxyapi".into(),
            name: "CLIProxyAPI".into(),
            icon: "activity".into(),
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
                ("management_key", "string", true),
            ]
            .into_iter()
            .map(|(name, kind, secret)| ConfigField {
                name: name.into(),
                kind: kind.into(),
                secret,
            })
            .collect(),
            streams,
            actions: vec!["refresh".into()],
            presets,
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            summary: self.summary(),
            config: serde_json::to_value(&self.settings).unwrap(),
            secrets: BTreeMap::from([(
                "management_key".into(),
                SecretState {
                    set: self.keys.get().map_err(|_| Error::Credentials)?.is_some(),
                },
            )]),
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let mut settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        settings.url = settings.url.trim().trim_end_matches('/').into();
        secrets::origin(&settings.url).map_err(|_| Error::ConfigInvalid)?;
        settings.interval_s = settings.interval_s.max(60);
        if settings == self.settings {
            return self.view();
        }
        self.keys
            .rebind(&settings.url)
            .map_err(|_| Error::Credentials)?;
        let mut config = self.legacy.clone();
        config.cliproxy = settings.enabled;
        config.cliproxy_url = settings.url.clone();
        config.cliproxy_interval_s = settings.interval_s;
        if let Some(file) = &self.file
            && config.save(file).is_err()
        {
            self.keys
                .rebind(&self.settings.url)
                .map_err(|_| Error::Credentials)?;
            return Err(Error::Storage);
        }
        self.settings = settings;
        self.legacy = config;
        self.reset();
        self.view()
    }
    fn secret_put(&mut self, field: &str, value: &str) -> Result<(), Error> {
        Self::field(field)?;
        if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(Error::ConfigInvalid);
        }
        self.keys
            .set(value.trim())
            .map_err(|_| Error::Credentials)?;
        self.reset();
        Ok(())
    }
    fn secret_delete(&mut self, field: &str) -> Result<(), Error> {
        Self::field(field)?;
        self.keys.clear().map_err(|_| Error::Credentials)?;
        self.reset();
        Ok(())
    }
    fn refresh(&mut self) {
        let now = Instant::now();
        if self.settings.enabled
            && self
                .last_request
                .is_none_or(|last| now.duration_since(last) >= crate::sources::PROXY_REFRESH_FLOOR)
        {
            self.last_request = Some(now);
            self.readings = self.source.refresh();
            self.next = Instant::now()
                .checked_add(std::time::Duration::from_secs(self.settings.interval_s));
        }
    }
    fn tick(&mut self) {
        let now = Instant::now();
        if self.settings.enabled
            && self.next.is_some_and(|next| now >= next)
            && self.last_request.is_none_or(|last| now.duration_since(last) >= crate::sources::PROXY_REFRESH_FLOOR)
        {
            self.last_request = Some(now);
            self.readings = self.source.poll();
            self.next = Instant::now()
                .checked_add(std::time::Duration::from_secs(self.settings.interval_s));
        }
    }
    fn readings(&self) -> Vec<crate::sources::Reading> {
        self.readings.clone()
    }
    fn delivered(&mut self, delivery: Delivery) {
        self.delivery.insert(delivery.device.clone(), delivery);
    }
}
