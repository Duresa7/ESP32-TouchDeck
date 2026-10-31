use super::*;
use crate::sources::{
    Fetch, Reading,
    network::{FetchError, Request, Response, TIMEOUT, parse_pin},
};
use serde_json::json;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const STREAMS: [(&str, &str); 5] = [
    ("clients_total", "clients"),
    ("clients_wired", "clients"),
    ("clients_wireless", "clients"),
    ("wan_receive_bytes_s", "bytes/s"),
    ("wan_send_bytes_s", "bytes/s"),
];
const REPLY: &str = "UniFi sent a reply Open TouchDeck can't read. Check the controller version.";
const AUTH: &str = "UniFi refused access. Check the read-only local account.";
const PIN_NEEDS_HTTPS: &str =
    "A certificate pin needs an https:// address. Change the address or clear the pin.";
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(2);
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    enabled: bool,
    url: String,
    site: String,
    unifi_os: bool,
    interval_s: u64,
    tls_pin: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "https://example.invalid".into(),
            site: "default".into(),
            unifi_os: true,
            interval_s: 10,
            tls_pin: String::new(),
        }
    }
}
impl Settings {
    fn validate(&mut self) -> Result<(), Error> {
        self.url = self.url.trim().trim_end_matches('/').into();
        let origin = secrets::origin(&self.url).map_err(|_| Error::ConfigInvalid)?;
        let url = url::Url::parse(&self.url).map_err(|_| Error::ConfigInvalid)?;
        if url.path() != "/" && !url.path().is_empty() {
            return Err(Error::ConfigInvalid);
        }
        self.url = origin;
        secrets::validate_id(&self.site).map_err(|_| Error::ConfigInvalid)?;
        if self.site.len() > 128 {
            return Err(Error::ConfigInvalid);
        }
        self.interval_s = self.interval_s.clamp(5, 3600);
        self.tls_pin = self.tls_pin.trim().to_ascii_lowercase();
        if !self.tls_pin.is_empty()
            && (self.tls_pin.len() != 64 || !self.tls_pin.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Error::ConfigInvalid);
        }
        if !self.tls_pin.is_empty() && url.scheme() != "https" {
            return Err(Error::ConfigField {
                field: "tls_pin",
                message: PIN_NEEDS_HTTPS,
            });
        }
        Ok(())
    }
}
struct Session {
    cookie: String,
    csrf: Option<String>,
}
struct UniFi {
    settings: Settings,
    file: Option<PathBuf>,
    secrets: Arc<dyn secrets::SecretStore>,
    fetch: Arc<dyn Fetch>,
    session: Option<Session>,
    next: Option<Instant>,
    failures: u32,
    sample: u64,
    live: Vec<LiveSample>,
    readings: Vec<Reading>,
    message: Option<&'static str>,
    delivery: BTreeMap<String, Delivery>,
}
fn create(env: &Environment) -> Result<Box<dyn Integration>, Error> {
    let file = env
        .config_file
        .as_ref()
        .map(|p| p.with_file_name("unifi.json"));
    let mut settings = Settings::default();
    if let Some(file) = &file {
        match std::fs::read(file) {
            Ok(bytes) => {
                settings = serde_json::from_slice(&bytes).map_err(|_| Error::ConfigInvalid)?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Storage),
        }
    }
    settings.validate()?;
    Ok(Box::new(UniFi {
        settings,
        file,
        secrets: env.secrets.clone(),
        fetch: Arc::from((env.fetch)()),
        session: None,
        next: None,
        failures: 0,
        sample: 0,
        live: Vec::new(),
        readings: Vec::new(),
        message: None,
        delivery: BTreeMap::new(),
    }))
}
inventory::submit! { Registration(create) }
#[derive(Deserialize)]
struct Meta {
    rc: String,
}
#[derive(Deserialize)]
struct Health {
    meta: Meta,
    data: Vec<Subsystem>,
}
#[derive(Deserialize)]
struct Subsystem {
    subsystem: String,
    num_user: Option<u64>,
    num_guest: Option<u64>,
    #[serde(rename = "rx_bytes-r")]
    receive: Option<f64>,
    #[serde(rename = "tx_bytes-r")]
    send: Option<f64>,
}
fn values(body: &str) -> Result<[Option<f64>; 5], &'static str> {
    let health: Health = serde_json::from_str(body).map_err(|_| REPLY)?;
    if health.meta.rc != "ok" {
        return Err(REPLY);
    }
    let subsystem = |name: &str| {
        let mut found = health.data.iter().filter(|s| s.subsystem == name);
        let first = found.next();
        if found.next().is_some() {
            return Err(REPLY);
        }
        Ok(first)
    };
    let clients = |s: Option<&Subsystem>| {
        s.and_then(|s| s.num_user)
            .map(|users| users.checked_add(s.and_then(|s| s.num_guest).unwrap_or(0)))
            .map(|sum| sum.map(|n| n as f64).ok_or(REPLY))
            .transpose()
    };
    let wired = clients(subsystem("lan")?)?;
    let wireless = clients(subsystem("wlan")?)?;
    let wan = subsystem("wan")?;
    let total = match (wired, wireless) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
    let result = [
        total,
        wired,
        wireless,
        wan.and_then(|w| w.receive),
        wan.and_then(|w| w.send),
    ];
    if result.iter().all(Option::is_none)
        || result
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0.0..=1e9).contains(v))
    {
        return Err(REPLY);
    }
    Ok(result)
}
impl UniFi {
    fn credential(&self, field: &str) -> Result<Option<String>, Error> {
        Ok(self
            .secrets
            .get("unifi", field)
            .map_err(|_| Error::Credentials)?
            .filter(|s| s.origin == self.settings.url)
            .map(|s| s.value))
    }
    fn build(
        &self,
        path: &str,
        body: Option<String>,
        session: Option<&Session>,
        deadline: Instant,
    ) -> Result<Request, &'static str> {
        secrets::credential_transport(&self.settings.url)
            .map_err(|_| "UniFi credentials require HTTPS. Check the controller address.")?;
        if crate::sources::cliproxy::insecure_http(&self.settings.url) {
            return Err("UniFi credentials require HTTPS. Check the controller address.");
        }
        let mut headers = BTreeMap::new();
        if let Some(session) = session {
            headers.insert("Cookie".into(), session.cookie.clone());
            if let Some(csrf) = &session.csrf {
                headers.insert("X-CSRF-Token".into(), csrf.clone());
            }
        }
        Ok(Request {
            url: format!("{}{path}", self.settings.url),
            json: body,
            headers,
            tls_pin: parse_pin(&self.settings.tls_pin),
            deadline,
        })
    }
    fn request(
        &self,
        path: &str,
        body: Option<String>,
        authenticated: bool,
        deadline: Instant,
    ) -> Result<Response, &'static str> {
        let session = if authenticated {
            Some(self.session.as_ref().ok_or(AUTH)?)
        } else {
            None
        };
        self.fetch
            .send(self.build(path, body, session, deadline)?)
            .map_err(|e| match e {
                FetchError::Timeout => {
                    "UniFi didn't answer within 10 s. Check the controller, then retry."
                }
                FetchError::Tls => {
                    "Couldn't verify the UniFi certificate. Check its SHA-256 fingerprint."
                }
                FetchError::TooLarge => {
                    "The UniFi reply is too large. Check the controller version."
                }
                FetchError::Unauthorized => AUTH,
                FetchError::InvalidReply => REPLY,
                FetchError::Transport => "Couldn't connect to UniFi. Check the controller address.",
            })
    }
    fn login(&mut self, deadline: Instant) -> Result<(), &'static str> {
        let username = self
            .credential("username")
            .map_err(|_| "Credential store unavailable")?
            .ok_or(AUTH)?;
        let password = self
            .credential("password")
            .map_err(|_| "Credential store unavailable")?
            .ok_or(AUTH)?;
        let reply = self.request(
            if self.settings.unifi_os {
                "/api/auth/login"
            } else {
                "/api/login"
            },
            Some(json!({"username":username,"password":password}).to_string()),
            false,
            deadline,
        )?;
        if matches!(reply.status, 401 | 403) {
            return Err(AUTH);
        }
        if !(200..300).contains(&reply.status) {
            return Err(REPLY);
        }
        #[derive(Deserialize)]
        struct Login {
            meta: Option<Meta>,
        }
        let login: Login = serde_json::from_str(&reply.body).map_err(|_| REPLY)?;
        if login.meta.as_ref().is_some_and(|m| m.rc != "ok")
            || (!self.settings.unifi_os && login.meta.is_none())
        {
            return Err(AUTH);
        }
        let cookie = reply
            .set_cookie
            .iter()
            .filter_map(|v| v.split(';').next())
            .map(str::trim)
            .filter(|v| {
                v.split_once('=')
                    .is_some_and(|(k, v)| !k.is_empty() && !v.is_empty())
            })
            .collect::<Vec<_>>()
            .join("; ");
        if cookie.is_empty() {
            return Err(REPLY);
        }
        let csrf = reply.csrf_token.clone();
        if cookie.len() > 8192
            || cookie.chars().any(char::is_control)
            || csrf
                .as_ref()
                .is_some_and(|v| v.is_empty() || v.len() > 8192 || v.chars().any(char::is_control))
            || (self.settings.unifi_os && csrf.is_none())
        {
            return Err(REPLY);
        }
        self.session = Some(Session { cookie, csrf });
        Ok(())
    }
    fn logout(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        let Ok(request) = self.build(
            if self.settings.unifi_os {
                "/api/auth/logout"
            } else {
                "/logout"
            },
            Some("{}".into()),
            Some(&session),
            Instant::now() + LOGOUT_TIMEOUT,
        ) else {
            return;
        };
        let fetch = self.fetch.clone();
        let _ = std::thread::Builder::new()
            .name("unifi-logout".into())
            .spawn(move || {
                let _ = fetch.send(request);
            });
    }
    fn reset(&mut self) {
        self.live.clear();
        self.readings.clear();
        self.message = None;
    }
    fn poll(&mut self) -> Result<[Option<f64>; 5], &'static str> {
        let deadline = Instant::now() + TIMEOUT;
        if self.session.is_none() {
            self.login(deadline)?;
        }
        let path = format!(
            "{}/api/s/{}/stat/health",
            if self.settings.unifi_os {
                "/proxy/network"
            } else {
                ""
            },
            self.settings.site
        );
        let mut reply = self.request(&path, None, true, deadline)?;
        if reply.status == 401 {
            self.session = None;
            self.login(deadline)?;
            reply = self.request(&path, None, true, deadline)?;
        }
        if matches!(reply.status, 401 | 403) {
            self.session = None;
            return Err(AUTH);
        }
        if reply.status != 200 {
            return Err(REPLY);
        }
        values(&reply.body)
    }
}
impl Integration for UniFi {
    fn summary(&self) -> IntegrationSummary {
        let credentials = self.credential("username").and_then(|u| {
            self.credential("password")
                .map(|p| u.is_some() && p.is_some())
        });
        let (state, message) = match credentials {
            Err(_) => ("error", Some("Credential store unavailable")),
            Ok(false) => ("not_configured", None),
            Ok(true) => (
                if self.message.is_some() {
                    "error"
                } else {
                    "ok"
                },
                self.message,
            ),
        };
        let config_schema = [
            ("enabled", "boolean", false),
            ("url", "string", false),
            ("site", "string", false),
            ("unifi_os", "boolean", false),
            ("interval_s", "integer", false),
            ("tls_pin", "string", false),
            ("username", "string", true),
            ("password", "string", true),
        ]
        .into_iter()
        .map(|(name, kind, secret)| ConfigField {
            name: name.into(),
            kind: kind.into(),
            secret,
        })
        .collect();
        let presets = [
            ("clients_total", "Connected clients", None),
            ("wan_receive_bytes_s", "WAN receive", Some(td_core::layout::TileRender::Graph)),
            ("wan_send_bytes_s", "WAN send", Some(td_core::layout::TileRender::Graph)),
        ].into_iter().map(|(id, name, render)| PresetTile {
            id: id.into(), name: name.into(),
            action: json!({"type":"widget","source":"unifi","account":self.settings.site,"field":id}),
            render,
        }).collect();
        IntegrationSummary {
            revision: self.sample,
            id: "unifi".into(),
            name: "UniFi".into(),
            icon: "network".into(),
            enabled: self.settings.enabled,
            status: IntegrationStatus {
                state: state.into(),
                message: message.map(str::to_owned),
                delivery: self.delivery.values().cloned().collect(),
            },
            config_schema,
            streams: STREAMS
                .iter()
                .map(|(id, unit)| DataStream {
                    id: (*id).into(),
                    kind: "numeric".into(),
                    unit: Some((*unit).into()),
                })
                .collect(),
            actions: vec!["refresh".into()],
            presets,
        }
    }
    fn view(&self) -> Result<IntegrationView, Error> {
        Ok(IntegrationView {
            detail: None,
            summary: self.summary(),
            config: serde_json::to_value(&self.settings).map_err(|_| Error::ConfigInvalid)?,
            secrets: ["username", "password"]
                .into_iter()
                .map(|f| {
                    Ok((
                        f.into(),
                        SecretState {
                            set: self.credential(f)?.is_some(),
                        },
                    ))
                })
                .collect::<Result<_, Error>>()?,
        })
    }
    fn configure(&mut self, config: Value) -> Result<IntegrationView, Error> {
        let mut settings: Settings =
            serde_json::from_value(config).map_err(|_| Error::ConfigInvalid)?;
        settings.validate()?;
        if settings == self.settings {
            return self.view();
        }
        self.logout();
        if settings.url != self.settings.url || settings.tls_pin != self.settings.tls_pin {
            for field in ["username", "password"] {
                self.secrets
                    .delete("unifi", field)
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
        self.reset();
        self.view()
    }
    fn secret_put(&mut self, field: &str, value: &str) -> Result<(), Error> {
        if !matches!(field, "username" | "password")
            || value.is_empty()
            || value.len() > 4096
            || value.chars().any(char::is_control)
        {
            return Err(Error::ConfigInvalid);
        }
        self.logout();
        self.reset();
        self.secrets
            .put(
                "unifi",
                field,
                secrets::Secret {
                    origin: self.settings.url.clone(),
                    value: value.into(),
                },
            )
            .map_err(|_| Error::Credentials)
    }
    fn secret_delete(&mut self, field: &str) -> Result<(), Error> {
        if !matches!(field, "username" | "password") {
            return Err(Error::ConfigInvalid);
        }
        self.logout();
        self.reset();
        self.secrets
            .delete("unifi", field)
            .map_err(|_| Error::Credentials)
    }
    fn refresh(&mut self) {
        self.tick();
    }
    fn tick(&mut self) {
        if !self.settings.enabled || self.next.is_some_and(|n| Instant::now() < n) {
            return;
        }
        if !matches!(
            (self.credential("username"), self.credential("password")),
            (Ok(Some(_)), Ok(Some(_)))
        ) {
            return;
        }
        match self.poll() {
            Ok(values) => {
                self.failures = 0;
                self.message = None;
                self.sample = self.sample.saturating_add(1);
                let updated = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                self.live = STREAMS
                    .iter()
                    .zip(values)
                    .filter_map(|(s, v)| Some((s, v?)))
                    .map(|((id, _), v)| LiveSample {
                        account: self.settings.site.clone(),
                        stream: (*id).into(),
                        sample: self.sample,
                        ttl_ms: (self.settings.interval_s * 3000).clamp(1000, 60000) as u32,
                        value: td_core::live::LiveValue::Number(v),
                    })
                    .collect();
                self.readings = STREAMS
                    .iter()
                    .zip(values)
                    .filter_map(|(s, v)| Some((s, v?)))
                    .map(|((id, unit), v)| {
                        Reading::new(
                            "unifi",
                            &self.settings.site,
                            id,
                            format!("{v} {unit}"),
                            -1.0,
                            updated,
                            false,
                        )
                    })
                    .collect();
            }
            Err(message) => {
                self.failures = self.failures.saturating_add(1);
                self.message = Some(message);
                self.live.clear();
                for r in &mut self.readings {
                    r.stale = true;
                }
            }
        }
        let delay = self
            .settings
            .interval_s
            .saturating_mul(1u64 << self.failures.min(10))
            .min(3600);
        self.next = Some(Instant::now() + Duration::from_secs(delay));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failed_poll_doubles_the_wait_before_the_next() {
        let mut unifi = UniFi {
            settings: Settings {
                enabled: true,
                url: "http://example.invalid".into(),
                interval_s: 5,
                ..Settings::default()
            },
            file: None,
            secrets: Arc::new(secrets::MemorySecrets::default()),
            fetch: Arc::new(crate::sources::HttpFetch::new()),
            session: None,
            next: None,
            failures: 0,
            sample: 0,
            live: Vec::new(),
            readings: Vec::new(),
            message: None,
            delivery: BTreeMap::new(),
        };
        unifi.secret_put("username", "synthetic-user").unwrap();
        unifi.secret_put("password", "synthetic-password").unwrap();
        for seconds in [10, 20, 40, 80] {
            unifi.next = None;
            let start = Instant::now();
            unifi.tick();
            let wait = unifi.next.unwrap() - start;
            assert!(unifi.message.is_some());
            assert!(
                wait >= Duration::from_secs(seconds) && wait < Duration::from_secs(seconds + 1),
                "{wait:?}"
            );
        }
    }
}
