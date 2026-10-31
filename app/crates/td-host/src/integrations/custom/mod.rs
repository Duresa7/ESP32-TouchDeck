mod command;
mod path;
use super::*;
use crate::sources::network::{FetchError, GetRequest, TIMEOUT, parse_pin};
pub use command::CommandApproval;
use serde_json::json;
use std::time::{Duration, Instant};
use td_core::live::LiveValue;

const WAITING: &str = "Waiting for approval on this PC.";
const PIN_NEEDS_HTTPS: &str =
    "A certificate pin needs an https:// address. Change the address or clear the pin.";

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Number,
    Text,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    Http {
        url: String,
        #[serde(default)]
        tls_pin: Option<String>,
        value_path: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    Command {
        program: String,
        args: Vec<String>,
        value_path: Option<String>,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    name: String,
    interval_s: u64,
    kind: Kind,
    input: Input,
}

#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
    sources: Vec<Source>,
}

impl Settings {
    fn validate(&self) -> Result<(), Error> {
        let mut ids = std::collections::BTreeSet::new();
        let mut fields = std::collections::BTreeSet::new();
        if self.sources.len() > 16 {
            return Err(Error::ConfigInvalid);
        }
        for source in &self.sources {
            secrets::validate_id(&source.id).map_err(|_| Error::ConfigInvalid)?;
            if source.id.len() > 48
                || !ids.insert(&source.id)
                || source.name.is_empty()
                || source.name.len() > 128
                || !(1..=3600).contains(&source.interval_s)
            {
                return Err(Error::ConfigInvalid);
            }
            let value_path = match &source.input {
                Input::Http {
                    url,
                    value_path,
                    headers,
                    tls_pin,
                } => {
                    let origin = secrets::origin(url).map_err(|_| Error::ConfigInvalid)?;
                    if let Some(pin) = tls_pin {
                        parse_pin(pin).ok_or(Error::ConfigInvalid)?;
                        if !origin.starts_with("https://") {
                            return Err(Error::ConfigField {
                                field: "sources",
                                message: PIN_NEEDS_HTTPS,
                            });
                        }
                    }
                    if url.len() > 2048 || headers.len() > 16 {
                        return Err(Error::ConfigInvalid);
                    }
                    for (header, field) in headers {
                        secrets::validate_id(field).map_err(|_| Error::ConfigInvalid)?;
                        if field.len() > 64
                            || !fields.insert(field)
                            || ["enabled", "sources"].contains(&field.as_str())
                            || header.is_empty()
                            || !header
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                            || ["host", "content-length", "transfer-encoding", "connection"]
                                .contains(&header.to_ascii_lowercase().as_str())
                        {
                            return Err(Error::ConfigInvalid);
                        }
                    }
                    Some(value_path)
                }
                Input::Command {
                    program,
                    args,
                    value_path,
                } => {
                    #[cfg(windows)]
                    if !std::path::Path::new(program)
                        .extension()
                        .and_then(|v| v.to_str())
                        .is_some_and(|v| {
                            v.eq_ignore_ascii_case("exe") || v.eq_ignore_ascii_case("com")
                        })
                    {
                        return Err(Error::ConfigInvalid);
                    }
                    if !std::path::Path::new(program).is_absolute()
                        || program.len() > 4096
                        || args.len() > 64
                        || args.iter().any(|a| a.len() > 4096 || a.contains('\0'))
                        || program.contains('\0')
                    {
                        return Err(Error::ConfigInvalid);
                    }
                    value_path.as_ref()
                }
            };
            if let Some(value_path) = value_path {
                path::ValuePath::parse(value_path).map_err(|_| Error::ConfigInvalid)?;
            }
        }
        Ok(())
    }
    fn fields(&self) -> BTreeMap<String, (&str, Option<&str>)> {
        self.sources
            .iter()
            .flat_map(|s| match &s.input {
                Input::Http {
                    url,
                    headers,
                    tls_pin,
                    ..
                } => headers
                    .values()
                    .map(|field| (field.clone(), (url.as_str(), tls_pin.as_deref())))
                    .collect(),
                _ => Vec::new(),
            })
            .collect()
    }
}

struct Sample {
    next: Instant,
    failures: u32,
    value: Option<LiveSample>,
    updated: u64,
    error: Option<&'static str>,
}

struct Custom {
    approval: Option<Arc<dyn CommandApproval>>,
    settings: Settings,
    file: Option<PathBuf>,
    secrets: Arc<dyn secrets::SecretStore>,
    fetch: Box<dyn crate::sources::Fetch>,
    samples: BTreeMap<String, Sample>,
    sequence: u64,
    last_poll: Option<Instant>,
    revision: u64,
    delivery: BTreeMap<String, Delivery>,
}

fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    let file = env
        .config_file
        .as_ref()
        .map(|p| p.with_file_name("custom.json"));
    let settings = match file.as_ref().map(std::fs::read) {
        Some(Ok(bytes)) => {
            serde_json::from_slice::<Settings>(&bytes).map_err(|_| Error::ConfigInvalid)?
        }
        Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => return Err(Error::Storage),
        _ => Settings::default(),
    };
    settings.validate()?;
    Ok(Box::new(Custom {
        approval: None,
        settings,
        file,
        secrets: env.secrets.clone(),
        fetch: (env.fetch)(),
        samples: BTreeMap::new(),
        sequence: 0,
        last_poll: None,
        revision: 0,
        delivery: BTreeMap::new(),
    }))
}
inventory::submit! { Registration(create) }

fn parse_value(source: &Source, body: &str) -> Result<LiveValue, &'static str> {
    let invalid = "Couldn't read the value. Check the value path and kind.";
    let value_path = match &source.input {
        Input::Http { value_path, .. } => Some(value_path),
        Input::Command { value_path, .. } => value_path.as_ref(),
    };
    if let Some(value_path) = value_path {
        let json: Value = serde_json::from_str(body).map_err(|_| invalid)?;
        let path = path::ValuePath::parse(value_path).map_err(|_| invalid)?;
        let selected = path.select(&json).map_err(|_| invalid)?;
        match source.kind {
            Kind::Number => path::number(selected.as_f64().ok_or(invalid)?).map_err(|_| invalid),
            Kind::Text => selected
                .as_str()
                .filter(|s| s.len() <= 4096)
                .map(|s| LiveValue::Text(s.into()))
                .ok_or(invalid),
        }
    } else {
        match source.kind {
            Kind::Number => {
                path::number(body.trim().parse().map_err(|_| invalid)?).map_err(|_| invalid)
            }
            Kind::Text if body.trim().len() <= 4096 => Ok(LiveValue::Text(body.trim().into())),
            _ => Err(invalid),
        }
    }
}

impl Custom {
    fn poll(&self, source: &Source) -> Result<LiveValue, &'static str> {
        let body = match &source.input {
            Input::Http {
                url,
                headers,
                tls_pin,
                ..
            } => {
                let origin = secrets::origin(url).map_err(|_| "Check the HTTP address.")?;
                if !headers.is_empty() {
                    secrets::credential_transport(url)
                        .map_err(|_| "Use HTTPS for secret headers.")?;
                }
                let headers = headers
                    .iter()
                    .map(|(name, field)| {
                        let secret = self
                            .secrets
                            .get("custom", field)
                            .map_err(|_| "Credential store unavailable.")?
                            .filter(|s| s.origin == origin)
                            .ok_or("Enter the secret headers.")?;
                        Ok((name.clone(), secret.value))
                    })
                    .collect::<Result<Vec<_>, &'static str>>()?;
                self.fetch
                    .request(&GetRequest {
                        url,
                        headers: &headers,
                        tls_pin: tls_pin.as_deref().and_then(parse_pin),
                        deadline: Instant::now() + TIMEOUT,
                    })
                    .map_err(|e| match e {
                        FetchError::Unauthorized => {
                            "The server refused access. Check the secret headers."
                        }
                        FetchError::Timeout => "The server didn't answer within 10 s.",
                        FetchError::TooLarge => "The HTTP reply is over 1 MiB.",
                        FetchError::Tls => {
                            "Can't verify the server certificate. Check the TLS fingerprint."
                        }
                        FetchError::Redirect => "The server redirected. Use its final address.",
                        FetchError::Transport => {
                            "Couldn't fetch the value. Check the HTTP address."
                        }
                    })?
            }
            Input::Command { program, args, .. } => {
                if !self
                    .approval
                    .as_ref()
                    .is_some_and(|a| a.approved(program, args))
                {
                    return Err(WAITING);
                }
                command::capture(program, args, Duration::from_secs(10))?
            }
        };
        if body.len() > 1024 * 1024 {
            return Err("The HTTP reply is over 1 MiB.");
        }
        parse_value(source, &body)
    }
}

impl Integration for Custom {
    fn command_approval(&mut self, approval: Arc<dyn CommandApproval>) {
        self.approval = Some(approval);
    }
    fn summary(&self) -> IntegrationSummary {
        let message = self
            .samples
            .values()
            .find_map(|s| s.error)
            .map(str::to_owned);
        let mut config_schema = vec![
            ConfigField {
                name: "enabled".into(),
                kind: "boolean".into(),
                secret: false,
            },
            ConfigField {
                name: "sources".into(),
                kind: "json".into(),
                secret: false,
            },
        ];
        config_schema.extend(self.settings.fields().keys().map(|name| ConfigField {
            name: name.clone(),
            kind: "string".into(),
            secret: true,
        }));
        IntegrationSummary {
            revision: self.revision, id: "custom".into(), name: "Custom".into(), icon: "code".into(), enabled: self.settings.enabled,
            status: IntegrationStatus { state: if message.is_some() { "error" } else if self.settings.sources.is_empty() { "not_configured" } else { "ok" }.into(), message, delivery: self.delivery.values().cloned().collect() },
            config_schema,
            streams: self.settings.sources.iter().map(|s| DataStream { id: s.id.clone(), kind: match s.kind { Kind::Number => "numeric", Kind::Text => "text" }.into(), unit: None }).collect(),
            actions: vec!["refresh".into()],
            presets: self.settings.sources.iter().filter(|s| s.kind == Kind::Number).flat_map(|s| [("value", None), ("graph", Some(td_core::layout::TileRender::Graph))].into_iter().map(move |(kind, render)| PresetTile { id: format!("{}_{kind}", s.id), name: format!("{} {kind}", s.name), action: json!({"type":"widget","source":"custom","account":"","field":s.id}), render })).collect(),
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        let secrets = self
            .settings
            .fields()
            .into_iter()
            .map(|(field, (url, _))| {
                let origin = secrets::origin(url).map_err(|_| Error::ConfigInvalid)?;
                let set = self
                    .secrets
                    .get("custom", &field)
                    .map_err(|_| Error::Credentials)?
                    .is_some_and(|s| s.origin == origin);
                Ok((field, SecretState { set }))
            })
            .collect::<Result<_, Error>>()?;
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: serde_json::to_value(&self.settings).map_err(|_| Error::ConfigInvalid)?,
            secrets,
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        settings.validate()?;
        if settings == self.settings {
            return self.view();
        }
        let next = settings.fields();
        for (field, (url, pin)) in self.settings.fields() {
            if next.get(&field).is_none_or(|&(next_url, next_pin)| {
                secrets::origin(next_url).ok() != secrets::origin(url).ok() || next_pin != pin
            }) {
                self.secrets
                    .delete("custom", &field)
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
        self.samples
            .retain(|id, _| settings.sources.iter().any(|s| s.id == *id));
        self.settings = settings;
        for sample in self.samples.values_mut() {
            sample.value = None;
            sample.error = None;
        }
        self.revision += 1;
        self.view()
    }
    fn secret_put(&mut self, field: &str, value: &str) -> Result<(), Error> {
        let fields = self.settings.fields();
        let &(url, _) = fields.get(field).ok_or(Error::ConfigInvalid)?;
        secrets::credential_transport(url).map_err(|_| Error::ConfigInvalid)?;
        if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(Error::ConfigInvalid);
        }
        self.secrets
            .put(
                "custom",
                field,
                secrets::Secret {
                    origin: secrets::origin(url).map_err(|_| Error::ConfigInvalid)?,
                    value: value.into(),
                },
            )
            .map_err(|_| Error::Credentials)
    }
    fn secret_delete(&mut self, field: &str) -> Result<(), Error> {
        if !self.settings.fields().contains_key(field) {
            return Err(Error::ConfigInvalid);
        }
        self.secrets
            .delete("custom", field)
            .map_err(|_| Error::Credentials)
    }
    fn refresh(&mut self) {
        self.tick();
    }
    fn tick(&mut self) {
        if !self.settings.enabled
            || self
                .last_poll
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.last_poll = Some(Instant::now());
        for source in &self.settings.sources {
            if self
                .samples
                .get(&source.id)
                .is_some_and(|s| Instant::now() < s.next)
            {
                continue;
            }
            let result = self.poll(source);
            let sample = self.samples.entry(source.id.clone()).or_insert(Sample {
                updated: 0,
                next: Instant::now(),
                failures: 0,
                value: None,
                error: None,
            });
            let delay = match result {
                Ok(value) => {
                    self.sequence += 1;
                    sample.updated = crate::sources::unix_ms(std::time::SystemTime::now());
                    sample.value = Some(LiveSample {
                        account: String::new(),
                        stream: source.id.clone(),
                        sample: self.sequence,
                        ttl_ms: (source.interval_s * 3000).clamp(1000, 60000) as u32,
                        value,
                    });
                    sample.failures = 0;
                    sample.error = None;
                    Duration::from_secs(source.interval_s)
                }
                Err(WAITING) => {
                    sample.error = Some(WAITING);
                    Duration::from_secs(1)
                }
                Err(error) => {
                    sample.failures = (sample.failures + 1).min(12);
                    sample.error = Some(error);
                    Duration::from_secs((source.interval_s * (1 << sample.failures)).min(3600))
                }
            };
            sample.next = Instant::now() + delay;
            self.revision += 1;
        }
    }
    fn readings(&self) -> Vec<crate::sources::Reading> {
        if !self.settings.enabled {
            return Vec::new();
        }
        self.settings
            .sources
            .iter()
            .filter_map(|source| {
                let sample = self.samples.get(&source.id)?;
                let value = match &sample.value.as_ref()?.value {
                    LiveValue::Number(n) => n.to_string(),
                    LiveValue::Text(text) => text.clone(),
                    _ => return None,
                };
                Some(crate::sources::Reading::new(
                    "custom",
                    "",
                    &source.id,
                    value,
                    -1.0,
                    sample.updated,
                    sample.error.is_some(),
                ))
            })
            .collect()
    }
    fn live(&self) -> Vec<LiveSample> {
        if !self.settings.enabled {
            return Vec::new();
        }
        self.settings
            .sources
            .iter()
            .filter_map(|s| self.samples.get(&s.id)?.value.clone())
            .collect()
    }
    fn delivered(&mut self, delivery: Delivery) {
        self.delivery.insert(delivery.device.clone(), delivery);
    }
}
