use std::sync::Arc;
use td_host::integrations::{
    Environment, Registry,
    secrets::{MemorySecrets, Secret, SecretStore},
};

#[test]
fn proxmox_is_discoverable_without_credentials() {
    let mut registry = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(|| Box::new(td_host::sources::HttpFetch::new())),
    })
    .unwrap();
    let view = registry
        .get("proxmox")
        .expect("Proxmox registered")
        .view()
        .unwrap();
    assert_eq!(view.summary.name, "Proxmox");
    assert_eq!(view.summary.status.state, "not_configured");
    assert!(!view.summary.enabled);
}
use serde_json::json;
use std::sync::Mutex;
use td_host::sources::{
    Fetch,
    network::{FetchError, GetRequest},
};

const RESPONSE: &str = include_str!("fixtures/proxmox-resources.json");
struct Fixture {
    reply: Result<String, FetchError>,
    calls: Arc<Mutex<Vec<String>>>,
}
impl Fetch for Fixture {
    fn get(&self, _: &str, _: &str) -> Result<String, String> {
        panic!("unexpected bearer GET")
    }
    fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<String, String> {
        panic!("writes forbidden")
    }
    fn request(&self, request: &GetRequest<'_>) -> Result<String, FetchError> {
        assert_eq!(
            request.url,
            "https://example.invalid:8006/api2/json/cluster/resources"
        );
        assert!(request.authorization.starts_with("PVEAPIToken="));
        self.calls.lock().unwrap().push(request.url.into());
        self.reply.clone()
    }
}
fn configured(reply: Result<String, FetchError>) -> (Registry, Arc<Mutex<Vec<String>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let records = calls.clone();
    let mut registry = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(move || {
            Box::new(Fixture {
                reply: reply.clone(),
                calls: records.clone(),
            })
        }),
    })
    .unwrap();
    let integration = registry.get("proxmox").unwrap();
    integration.configure(json!({"enabled":true,"url":"https://example.invalid:8006","interval_s":5,"tls_pin":""})).unwrap();
    for field in ["token_id", "token_secret"] {
        integration
            .secret_put(field, "fixture-private-value")
            .unwrap();
    }
    (registry, calls)
}
#[test]
fn recorded_resources_produce_readings_live_values_and_ordered_presets() {
    let (mut registry, calls) = configured(Ok(RESPONSE.into()));
    let integration = registry.get("proxmox").unwrap();
    integration.tick();
    assert_eq!(integration.summary().status.state, "ok");
    let live = integration.live();
    let number = |account: &str, stream: &str| {
        live.iter()
            .find(|s| s.account == account && s.stream == stream)
            .unwrap()
            .value
            .clone()
    };
    use td_core::live::LiveValue::Number;
    assert_eq!(number("node/example.invalid", "cpu_pct"), Number(25.0));
    assert_eq!(number("qemu/100", "ram_pct"), Number(50.0));
    assert_eq!(number("lxc/101", "running"), Number(0.0));
    assert_eq!(number("cluster", "running_guests"), Number(1.0));
    assert_eq!(number("cluster", "total_guests"), Number(2.0));
    assert!(live.iter().all(|s| s.sample > 0 && s.ttl_ms == 15000));
    assert_eq!(integration.readings().len(), live.len());
    let names: Vec<_> = integration
        .summary()
        .presets
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(
        names,
        [
            "Node CPU graph",
            "Node CPU",
            "Node RAM graph",
            "Node RAM",
            "Guest status",
            "Guest status",
            "Running guests"
        ]
    );
    integration.refresh();
    integration.tick();
    assert_eq!(calls.lock().unwrap().len(), 1);
    let serialized = serde_json::to_string(&integration.view().unwrap()).unwrap();
    assert!(!serialized.contains("fixture-private-value"));
    let changed = integration.configure(json!({"enabled":true,"url":"https://other.example.invalid:8006","interval_s":5,"tls_pin":""})).unwrap();
    assert!(changed.secrets.values().all(|s| !s.set));
    integration.tick();
    assert_eq!(calls.lock().unwrap().len(), 1);
}
#[test]
fn one_odd_resource_is_skipped_and_counted_without_blanking_the_poll() {
    let (mut registry, _) = configured(Ok(
        include_str!("fixtures/proxmox-odd-resources.json").into()
    ));
    let integration = registry.get("proxmox").unwrap();
    integration.tick();
    let summary = integration.summary();
    assert_eq!(summary.status.state, "ok");
    assert_eq!(
        summary.status.message.as_deref(),
        Some("Unreadable Proxmox resources skipped: 2.")
    );
    let live = integration.live();
    let number = |account: &str, stream: &str| {
        live.iter()
            .find(|s| s.account == account && s.stream == stream)
            .map(|s| s.value.clone())
    };
    use td_core::live::LiveValue::Number;
    assert_eq!(
        number("node/example.invalid", "cpu_pct"),
        Some(Number(25.0))
    );
    assert_eq!(number("node/lost.example.invalid", "cpu_pct"), None);
    assert_eq!(number("qemu/100", "cpu_pct"), Some(Number(100.0)));
    assert_eq!(number("qemu/100", "ram_pct"), Some(Number(100.0)));
    assert_eq!(number("lxc/103", "running"), Some(Number(0.0)));
    assert_eq!(number("qemu/102", "running"), None);
    assert_eq!(number("qemu/104", "running"), None);
    assert_eq!(number("cluster", "running_guests"), Some(Number(1.0)));
    assert_eq!(number("cluster", "total_guests"), Some(Number(3.0)));
    let accounts: Vec<_> = summary
        .presets
        .iter()
        .map(|p| p.action["account"].as_str().unwrap().to_owned())
        .collect();
    assert!(accounts.contains(&"node/lost.example.invalid".to_owned()));
    assert!(!accounts.iter().any(|a| a == "qemu/102" || a == "qemu/104"));
}
#[test]
fn a_secret_stored_for_another_origin_is_not_used() {
    let secrets = Arc::new(MemorySecrets::default());
    for field in ["token_id", "token_secret"] {
        secrets
            .put(
                "proxmox",
                field,
                Secret {
                    origin: "https://other.example.invalid:8006".into(),
                    value: "fixture-private-value".into(),
                },
            )
            .unwrap();
    }
    let calls = Arc::new(Mutex::new(Vec::new()));
    let records = calls.clone();
    let mut registry = Registry::new(Environment {
        secrets,
        config_file: None,
        fetch: Arc::new(move || {
            Box::new(Fixture {
                reply: Ok(RESPONSE.into()),
                calls: records.clone(),
            })
        }),
    })
    .unwrap();
    let integration = registry.get("proxmox").unwrap();
    let view = integration.configure(json!({"enabled":true,"url":"https://example.invalid:8006","interval_s":5,"tls_pin":""})).unwrap();
    assert!(view.secrets.values().all(|s| !s.set));
    integration.tick();
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(integration.summary().status.state, "not_configured");
}
#[test]
fn failures_are_safe_and_clear_live_values() {
    for (reply, message) in [
        (
            Err(FetchError::Unauthorized),
            "Proxmox rejected the token. Check its read-only permissions.",
        ),
        (
            Err(FetchError::Timeout),
            "Proxmox didn't answer within 10 s. Check the connection, then retry.",
        ),
        (
            Err(FetchError::TooLarge),
            "Proxmox sent more than 1 MiB. Check the server.",
        ),
        (
            Err(FetchError::Tls),
            "Couldn't verify the Proxmox certificate. Check the SHA-256 pin or system trust store.",
        ),
        (
            Ok("fixture-private-value".into()),
            "Couldn't read the Proxmox response. Check the server.",
        ),
        (
            Ok("x".repeat(1024 * 1024 + 1)),
            "Proxmox sent more than 1 MiB. Check the server.",
        ),
    ] {
        let (mut registry, calls) = configured(reply);
        let integration = registry.get("proxmox").unwrap();
        integration.tick();
        assert_eq!(integration.summary().status.state, "error");
        assert_eq!(
            integration.summary().status.message.as_deref(),
            Some(message)
        );
        assert!(integration.live().is_empty());
        integration.refresh();
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(
            !serde_json::to_string(&integration.view().unwrap())
                .unwrap()
                .contains("fixture-private-value")
        );
    }
}
#[test]
fn a_pin_change_forgets_the_token_but_an_interval_change_keeps_it() {
    let (mut registry, _) = configured(Ok(RESPONSE.into()));
    let integration = registry.get("proxmox").unwrap();
    let url = "https://example.invalid:8006";
    integration
        .configure(json!({"enabled":true,"url":url,"interval_s":10,"tls_pin":""}))
        .unwrap();
    assert!(integration.view().unwrap().secrets.values().all(|s| s.set));
    let pin = "ab".repeat(32);
    let view = integration
        .configure(json!({"enabled":true,"url":url,"interval_s":10,"tls_pin":pin}))
        .unwrap();
    assert!(view.secrets.values().all(|s| !s.set));
}
