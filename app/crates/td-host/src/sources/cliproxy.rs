//! CLIProxyAPI, read passively.
//!
//! One `GET /v0/management/auth-files` per poll turns each auth file into an account. The
//! source never calls `usage-queue`, which pops records other tools may own, and never
//! writes. No documented management endpoint reports a per-account quota, so a quota
//! row waits for one that does.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::{CLIPROXYAPI, FAILS_TO_STALE, Info, KeyStore, Reading, Source, key_hint, unix_ms};

pub const AUTH_FILES: &str = "/v0/management/auth-files";
const TIMEOUT: Duration = Duration::from_secs(5);

/// One authenticated account, from one entry of the auth-files reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub provider: String,
    pub email: String,
    pub status: String,
    pub success: u64,
    pub failed: u64,
    /// Requests in the newest `recent_requests` bucket, which spans 10 minutes.
    pub recent_10m: u64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Bucket {
    success: u64,
    failed: u64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    id: String,
    name: String,
    label: String,
    email: String,
    provider: String,
    r#type: String,
    status: String,
    disabled: bool,
    unavailable: bool,
    success: u64,
    failed: u64,
    recent_requests: Vec<Bucket>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Reply {
    Files { files: Vec<Entry> },
    Bare(Vec<Entry>),
}

pub fn parse_accounts(body: &str) -> Result<Vec<Account>, String> {
    let entries = match serde_json::from_str::<Reply>(body) {
        Ok(Reply::Files { files } | Reply::Bare(files)) => files,
        Err(error) => return Err(format!("not an auth-files reply: {error}")),
    };
    Ok(entries
        .into_iter()
        .map(|entry| {
            let pick = |names: [&String; 4]| {
                names
                    .into_iter()
                    .find(|name| !name.trim().is_empty())
                    .cloned()
                    .unwrap_or_default()
            };
            let status = if !entry.status.trim().is_empty() {
                entry.status.clone()
            } else if entry.disabled {
                "disabled".to_owned()
            } else if entry.unavailable {
                "unavailable".to_owned()
            } else {
                "unknown".to_owned()
            };
            Account {
                provider: pick([&entry.provider, &entry.r#type, &entry.label, &entry.name]),
                email: pick([&entry.email, &entry.label, &entry.name, &entry.id]),
                status,
                success: entry.success,
                failed: entry.failed,
                recent_10m: entry
                    .recent_requests
                    .last()
                    .map_or(0, |bucket| bucket.success + bucket.failed),
            }
        })
        .collect())
}

fn severity(status: &str) -> u8 {
    match status {
        "active" => 0,
        "error" => 4,
        "unavailable" => 3,
        "disabled" => 2,
        _ => 1,
    }
}

/// Every reading for a set of accounts: the aggregate under the empty account, then each
/// account. `recent_10m` carries a bar scaled to the busiest account.
pub fn readings(accounts: &[Account], updated: u64, stale: bool) -> Vec<Reading> {
    let reading = |account: &str, field: &str, value: String, bar: f64| {
        Reading::new(CLIPROXYAPI, account, field, value, bar, updated, stale)
    };
    let worst = accounts
        .iter()
        .map(|account| account.status.as_str())
        .max_by_key(|status| severity(status))
        .unwrap_or("none");
    let busiest = accounts.iter().map(|a| a.recent_10m).max().unwrap_or(0);
    let mut out = vec![
        reading("", "status", worst.to_owned(), -1.0),
        reading(
            "",
            "success",
            accounts.iter().map(|a| a.success).sum::<u64>().to_string(),
            -1.0,
        ),
        reading(
            "",
            "failed",
            accounts.iter().map(|a| a.failed).sum::<u64>().to_string(),
            -1.0,
        ),
        reading(
            "",
            "recent_10m",
            accounts
                .iter()
                .map(|a| a.recent_10m)
                .sum::<u64>()
                .to_string(),
            -1.0,
        ),
    ];
    for account in accounts {
        let who = account.email.as_str();
        let bar = if busiest == 0 {
            0.0
        } else {
            account.recent_10m as f64 / busiest as f64
        };
        out.push(reading(who, "status", account.status.clone(), -1.0));
        out.push(reading(who, "success", account.success.to_string(), -1.0));
        out.push(reading(who, "failed", account.failed.to_string(), -1.0));
        out.push(reading(
            who,
            "recent_10m",
            account.recent_10m.to_string(),
            bar,
        ));
    }
    out
}

/// One `GET` with a bearer token. Tests fake it.
pub trait Fetch: Send {
    fn get(&self, url: &str, bearer: &str) -> Result<String, String>;
}

pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    pub fn new() -> HttpFetch {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .build()
            .into();
        HttpFetch { agent }
    }
}

impl Default for HttpFetch {
    fn default() -> HttpFetch {
        HttpFetch::new()
    }
}

impl Fetch for HttpFetch {
    fn get(&self, url: &str, bearer: &str) -> Result<String, String> {
        if url.starts_with("https://") {
            return Err("https base URLs are not supported yet, use http://".into());
        }
        let mut response = self
            .agent
            .get(url)
            .header("Authorization", format!("Bearer {bearer}"))
            .header("Accept", "application/json")
            .call()
            .map_err(|error| match error {
                ureq::Error::StatusCode(code) => format!("HTTP {code}"),
                other => other.to_string(),
            })?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProxyInfo {
    pub url: String,
    /// The last 4 characters of the management key, never the key.
    pub key_hint: Option<String>,
    /// True after a good poll, false after a failed one, and unset before the first.
    pub ok: Option<bool>,
    pub fails: u32,
    pub stale: bool,
    pub error: Option<String>,
    pub polled: Option<u64>,
    pub accounts: Vec<Account>,
}

pub struct CliProxy {
    base: String,
    keys: Arc<dyn KeyStore>,
    fetch: Box<dyn Fetch>,
    last: Vec<Reading>,
    info: ProxyInfo,
}

impl CliProxy {
    pub fn new(base: &str, keys: Arc<dyn KeyStore>, fetch: Box<dyn Fetch>) -> CliProxy {
        let base = base.trim().trim_end_matches('/').to_owned();
        CliProxy {
            info: ProxyInfo {
                url: base.clone(),
                ..ProxyInfo::default()
            },
            base,
            keys,
            fetch,
            last: Vec::new(),
        }
    }

    fn fetch_accounts(&mut self) -> Result<Vec<Account>, String> {
        let key = self
            .keys
            .get()?
            .filter(|key| !key.is_empty())
            .ok_or("no management key is set")?;
        self.info.key_hint = Some(key_hint(&key));
        let body = self
            .fetch
            .get(&format!("{}{AUTH_FILES}", self.base), &key)?;
        parse_accounts(&body)
    }

    pub fn poll_at(&mut self, now: SystemTime) -> Vec<Reading> {
        let polled = unix_ms(now);
        self.info.polled = Some(polled);
        match self.fetch_accounts() {
            Ok(accounts) => {
                self.last = readings(&accounts, polled, false);
                self.info.accounts = accounts;
                self.info.ok = Some(true);
                self.info.fails = 0;
                self.info.stale = false;
                self.info.error = None;
            }
            Err(error) => {
                self.info.ok = Some(false);
                self.info.fails += 1;
                self.info.error = Some(error);
                if self.info.fails >= FAILS_TO_STALE {
                    self.info.stale = true;
                    for reading in &mut self.last {
                        reading.stale = true;
                    }
                }
            }
        }
        self.last.clone()
    }
}

impl Source for CliProxy {
    fn id(&self) -> &'static str {
        CLIPROXYAPI
    }

    fn poll(&mut self) -> Vec<Reading> {
        self.poll_at(SystemTime::now())
    }

    fn info(&self) -> Info {
        Info::Proxy(self.info.clone())
    }
}
