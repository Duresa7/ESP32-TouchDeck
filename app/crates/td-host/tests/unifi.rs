use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use td_host::{
    integrations::{
        Environment, Error, Registry,
        secrets::{MemorySecrets, Secret, SecretStore},
    },
    sources::{
        Fetch, HttpFetch,
        network::{FetchError as Failure, Request, Response},
    },
};

#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<Result<Response, Failure>>>,
    calls: Mutex<Vec<Request>>,
}
struct Transport(Arc<Script>);
impl Fetch for Transport {
    fn get(&self, _: &str, _: &str) -> Result<String, String> {
        panic!("unexpected bearer request")
    }
    fn post_json(&self, _: &str, _: &str, _: &Value) -> Result<String, String> {
        panic!("unexpected bearer request")
    }
    fn send(&self, r: Request) -> Result<Response, Failure> {
        self.0.calls.lock().unwrap().push(r);
        self.0
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected request")
    }
}
fn response(status: u16, body: &str) -> Result<Response, Failure> {
    Ok(Response {
        status,
        set_cookie: Vec::new(),
        csrf_token: None,
        body: body.into(),
    })
}
fn login(os: bool) -> Result<Response, Failure> {
    let mut r = response(
        200,
        if os {
            "{}"
        } else {
            r#"{"meta":{"rc":"ok"},"data":[]}"#
        },
    )
    .unwrap();
    r.set_cookie = vec!["TOKEN=synthetic-session; Path=/; HttpOnly".into()];
    if os {
        r.csrf_token = Some("synthetic-csrf".into());
    }
    Ok(r)
}
const HEALTH: &str = r#"{"meta":{"rc":"ok"},"data":[{"subsystem":"wlan","num_user":3,"num_guest":2},{"subsystem":"lan","num_user":4,"num_guest":1},{"subsystem":"wan","rx_bytes-r":12000,"tx_bytes-r":3000}]}"#;
fn config(os: bool) -> Value {
    json!({"enabled":true,"url":"https://example.invalid","site":"default","unifi_os":os,"interval_s":5,"tls_pin":""})
}
fn wait_for_calls(script: &Script, n: usize) -> usize {
    let start = Instant::now();
    while script.calls.lock().unwrap().len() < n && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    script.calls.lock().unwrap().len()
}
fn setup(os: bool, replies: Vec<Result<Response, Failure>>) -> (Registry, Arc<Script>) {
    let script = Arc::new(Script {
        replies: Mutex::new(replies.into()),
        ..Default::default()
    });
    let fetch = script.clone();
    let mut registry = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(move || Box::new(Transport(fetch.clone()))),
    })
    .unwrap();
    let i = registry.get("unifi").unwrap();
    i.configure(config(os)).unwrap();
    i.secret_put("username", "synthetic-user").unwrap();
    i.secret_put("password", "synthetic-password").unwrap();
    (registry, script)
}
#[test]
fn both_controllers_produce_readings_and_live_values_with_only_allowlisted_requests() {
    for os in [false, true] {
        let (mut registry, script) = setup(
            os,
            vec![login(os), response(200, HEALTH), response(200, "{}")],
        );
        let i = registry.get("unifi").unwrap();
        i.refresh();
        assert_eq!(i.summary().status.state, "ok");
        assert_eq!(
            i.live()
                .iter()
                .map(|s| (&*s.stream, s.value.clone()))
                .collect::<Vec<_>>(),
            [
                ("clients_total", td_core::live::LiveValue::Number(10.0)),
                ("clients_wired", td_core::live::LiveValue::Number(5.0)),
                ("clients_wireless", td_core::live::LiveValue::Number(5.0)),
                (
                    "wan_receive_bytes_s",
                    td_core::live::LiveValue::Number(12000.0)
                ),
                ("wan_send_bytes_s", td_core::live::LiveValue::Number(3000.0))
            ]
        );
        assert_eq!(i.readings().len(), 5);
        assert!(
            i.live()
                .iter()
                .all(|s| s.account == "default" && s.sample > 0 && s.ttl_ms == 15000)
        );
        i.refresh();
        assert_eq!(script.calls.lock().unwrap().len(), 2);
        let view = serde_json::to_string(&i.view().unwrap()).unwrap();
        for secret in [
            "synthetic-user",
            "synthetic-password",
            "synthetic-session",
            "synthetic-csrf",
        ] {
            assert!(!view.contains(secret));
        }
        let mut disabled = config(os);
        disabled["enabled"] = json!(false);
        i.configure(disabled).unwrap();
        assert!(i.live().is_empty());
        assert_eq!(wait_for_calls(&script, 3), 3);
        let calls = script.calls.lock().unwrap();
        assert_eq!(
            calls[0].url,
            format!(
                "https://example.invalid{}",
                if os { "/api/auth/login" } else { "/api/login" }
            )
        );
        assert!(calls[0].json.is_some());
        assert_eq!(
            calls[1].url,
            format!(
                "https://example.invalid{}/api/s/default/stat/health",
                if os { "/proxy/network" } else { "" }
            )
        );
        assert!(calls[1].json.is_none());
        assert_eq!(calls[1].headers["Cookie"], "TOKEN=synthetic-session");
        if os {
            assert_eq!(calls[1].headers["X-CSRF-Token"], "synthetic-csrf");
        }
        assert_eq!(
            calls[2].url,
            format!(
                "https://example.invalid{}",
                if os { "/api/auth/logout" } else { "/logout" }
            )
        );
    }
}
#[test]
fn failures_are_safe_and_back_off() {
    for failure in [
        response(401, "synthetic-password"),
        response(403, "synthetic-password"),
        Err(Failure::Timeout),
        Err(Failure::TooLarge),
        Err(Failure::Tls),
        response(200, "broken"),
        response(200, &"x".repeat(1048577)),
    ] {
        let (mut registry, script) = setup(
            false,
            vec![
                login(false),
                failure,
                login(false),
                response(401, "synthetic-password"),
            ],
        );
        let i = registry.get("unifi").unwrap();
        i.refresh();
        assert_eq!(i.summary().status.state, "error");
        assert!(i.live().is_empty());
        let view = serde_json::to_string(&i.view().unwrap()).unwrap();
        assert!(!view.contains("synthetic-password"));
        let count = script.calls.lock().unwrap().len();
        i.refresh();
        i.tick();
        assert_eq!(script.calls.lock().unwrap().len(), count);
    }
}
#[test]
fn one_relogin_recovers_and_second_unauthorized_stops() {
    let (mut registry, script) = setup(
        true,
        vec![
            login(true),
            response(401, ""),
            login(true),
            response(200, HEALTH),
        ],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(i.summary().status.state, "ok");
    assert_eq!(script.calls.lock().unwrap().len(), 4);
}
#[test]
fn origin_change_drops_credentials_and_session() {
    let (mut registry, script) = setup(
        false,
        vec![login(false), response(200, HEALTH), response(200, "{}")],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    let mut changed = config(false);
    changed["url"] = json!("https://other.example.invalid");
    let view = i.configure(changed).unwrap();
    assert!(!view.secrets["username"].set && !view.secrets["password"].set);
    i.refresh();
    assert_eq!(wait_for_calls(&script, 3), 3);
    assert!(i.live().is_empty());
}
#[test]
fn invalid_config_never_becomes_a_request() {
    let (mut registry, script) = setup(false, vec![]);
    let i = registry.get("unifi").unwrap();
    for (field, value) in [
        ("site", json!("../other")),
        ("tls_pin", json!("bad")),
        ("url", json!("https://example.invalid/path")),
        ("password", json!("synthetic-password")),
    ] {
        let mut c = config(false);
        c[field] = value;
        assert!(i.configure(c).is_err());
    }
    assert!(script.calls.lock().unwrap().is_empty());
}

static LOGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
struct Capture;
impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        LOGS.lock().unwrap().push(record.args().to_string());
    }
    fn flush(&self) {}
}
#[test]
fn credentials_do_not_appear_in_captured_logs_and_changes_logout() {
    static LOGGER: Capture = Capture;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Trace);
    let (mut registry, script) = setup(
        true,
        vec![login(true), response(200, HEALTH), response(200, "{}")],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    i.secret_put("password", "synthetic-replacement").unwrap();
    assert!(i.live().is_empty());
    assert_eq!(wait_for_calls(&script, 3), 3);
    let text = LOGS.lock().unwrap().join("\n");
    for secret in [
        "synthetic-user",
        "synthetic-password",
        "synthetic-replacement",
        "synthetic-session",
        "synthetic-csrf",
    ] {
        assert!(!text.contains(secret));
    }
}
#[test]
fn another_poll_advances_sample_even_when_values_are_unchanged() {
    let (mut registry, _) = setup(
        false,
        vec![login(false), response(200, HEALTH), response(200, HEALTH)],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    let previous = i.live();
    std::thread::sleep(std::time::Duration::from_millis(5100));
    i.tick();
    assert!(
        i.live()
            .iter()
            .zip(previous)
            .all(|(new, old)| new.sample > old.sample && new.value == old.value)
    );
}

#[test]
fn interval_and_live_bounds_and_bad_upstream_values() {
    for interval in [0, 4000] {
        let (mut registry, _) = setup(false, vec![login(false), response(200, HEALTH)]);
        let i = registry.get("unifi").unwrap();
        let mut c = config(false);
        c["interval_s"] = json!(interval);
        let view = i.configure(c).unwrap();
        assert_eq!(
            view.config["interval_s"],
            if interval == 0 { 5 } else { 3600 }
        );
        i.tick();
        assert!(
            i.live()
                .iter()
                .all(|s| s.ttl_ms == if interval == 0 { 15000 } else { 60000 })
        );
    }
    for body in [
        HEALTH.replace("12000", "-1"),
        HEALTH.replace("12000", "1000000001"),
        HEALTH.replace("12000", "\"12000\""),
        HEALTH.replace("\"lan\"", "\"wlan\""),
        HEALTH.replace("\"num_user\":4,", "\"num_user\":\"4\","),
        HEALTH.replace("\"num_guest\":1", "\"num_guest\":-1"),
        HEALTH.replace("\"ok\"", "\"error\""),
    ] {
        let (mut registry, _) = setup(false, vec![login(false), response(200, &body)]);
        let i = registry.get("unifi").unwrap();
        i.tick();
        assert_eq!(i.summary().status.state, "error");
        assert!(i.live().is_empty());
    }
}
#[test]
fn non_loopback_plain_http_cannot_receive_credentials() {
    for url in [
        "http://example.invalid",
        "http://192.0.2.1",
        "http://192.168.1.10",
        "http://10.0.0.1",
    ] {
        let (mut registry, script) = setup(false, vec![]);
        let i = registry.get("unifi").unwrap();
        let mut c = config(false);
        c["url"] = json!(url);
        i.configure(c).unwrap();
        i.secret_put("username", "synthetic-user").unwrap();
        i.secret_put("password", "synthetic-password").unwrap();
        i.refresh();
        assert_eq!(i.summary().status.state, "error");
        assert!(script.calls.lock().unwrap().is_empty());
    }
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}
fn streams(i: &dyn td_host::integrations::Integration) -> Vec<(String, f64)> {
    i.live()
        .into_iter()
        .map(|s| match s.value {
            td_core::live::LiveValue::Number(v) => (s.stream, v),
            _ => panic!("non-numeric stream"),
        })
        .collect()
}
#[test]
fn recorded_site_health_yields_every_stream() {
    let (mut registry, _) = setup(
        true,
        vec![login(true), response(200, &fixture("unifi-health.json"))],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(i.summary().status.state, "ok");
    assert_eq!(
        streams(i.as_ref()),
        [
            ("clients_total".into(), 21.0),
            ("clients_wired".into(), 5.0),
            ("clients_wireless".into(), 16.0),
            ("wan_receive_bytes_s".into(), 389120.5),
            ("wan_send_bytes_s".into(), 41250.0),
        ]
    );
}
#[test]
fn a_site_without_a_gateway_reports_the_streams_it_has() {
    let (mut registry, _) = setup(
        false,
        vec![
            login(false),
            response(200, &fixture("unifi-health-no-gateway.json")),
        ],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(i.summary().status.state, "ok");
    assert_eq!(
        streams(i.as_ref()),
        [
            ("clients_total".into(), 7.0),
            ("clients_wireless".into(), 7.0)
        ]
    );
    assert_eq!(i.readings().len(), 2);
}
#[test]
fn a_health_reply_without_any_stream_is_an_error() {
    for body in [
        r#"{"meta":{"rc":"ok"},"data":[]}"#,
        r#"{"meta":{"rc":"ok"},"data":[{"subsystem":"wan","status":"unknown"}]}"#,
    ] {
        let (mut registry, _) = setup(false, vec![login(false), response(200, body)]);
        let i = registry.get("unifi").unwrap();
        i.refresh();
        assert_eq!(i.summary().status.state, "error");
        assert!(i.live().is_empty());
    }
}
#[test]
fn a_classic_login_sends_back_every_session_cookie() {
    let mut classic = login(false).unwrap();
    classic.set_cookie = vec![
        "unifises=synthetic-session; Path=/; secure; HttpOnly".into(),
        "csrf_token=synthetic-csrf; Path=/; secure".into(),
    ];
    let (mut registry, script) = setup(false, vec![Ok(classic), response(200, HEALTH)]);
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(i.summary().status.state, "ok");
    assert_eq!(
        script.calls.lock().unwrap()[1].headers["Cookie"],
        "unifises=synthetic-session; csrf_token=synthetic-csrf"
    );
}

fn http(status: &str, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
fn classic_login() -> String {
    http(
        "200 OK",
        "Set-Cookie: unifises=synthetic-session; Path=/\r\n",
        r#"{"meta":{"rc":"ok"},"data":[]}"#,
    )
}
fn controller(replies: Vec<(Duration, Option<String>)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    thread::spawn(move || {
        for (delay, reply) in replies {
            let Ok((mut socket, _)) = listener.accept() else {
                return;
            };
            let mut received = Vec::new();
            let mut buffer = [0; 4096];
            while let Ok(n) = socket.read(&mut buffer) {
                received.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&received).to_ascii_lowercase();
                let Some(end) = text.find("\r\n\r\n") else {
                    if n == 0 {
                        break;
                    }
                    continue;
                };
                let length = text
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map_or(0, |n| n.trim().parse::<usize>().unwrap());
                if n == 0 || received.len() >= end + 4 + length {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&received);
            log.lock()
                .unwrap()
                .push(text.lines().next().unwrap_or_default().to_owned());
            thread::sleep(delay);
            match reply {
                Some(reply) => {
                    let _ = socket.write_all(reply.as_bytes());
                }
                None => thread::sleep(Duration::from_secs(30)),
            }
        }
    });
    (url, seen)
}
fn real(url: &str) -> Registry {
    let mut registry = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(|| Box::new(HttpFetch::new())),
    })
    .unwrap();
    let i = registry.get("unifi").unwrap();
    let mut c = config(false);
    c["url"] = json!(url);
    i.configure(c).unwrap();
    i.secret_put("username", "synthetic-user").unwrap();
    i.secret_put("password", "synthetic-password").unwrap();
    registry
}
#[test]
fn disabling_or_changing_credentials_never_waits_on_a_silent_controller() {
    for operation in ["disable", "secret_put", "secret_delete"] {
        let (url, seen) = controller(vec![
            (Duration::ZERO, Some(classic_login())),
            (Duration::ZERO, Some(http("200 OK", "", HEALTH))),
            (Duration::ZERO, None),
        ]);
        let mut registry = real(&url);
        let i = registry.get("unifi").unwrap();
        i.refresh();
        assert_eq!(i.summary().status.state, "ok", "{operation}");
        let start = Instant::now();
        match operation {
            "disable" => {
                let mut c = config(false);
                c["url"] = json!(url);
                c["enabled"] = json!(false);
                i.configure(c).unwrap();
            }
            "secret_put" => i.secret_put("password", "synthetic-replacement").unwrap(),
            _ => i.secret_delete("password").unwrap(),
        }
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "{operation} took {:?}",
            start.elapsed()
        );
        while seen.lock().unwrap().len() < 3 && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            seen.lock().unwrap()[2],
            "POST /logout HTTP/1.1",
            "{operation}"
        );
    }
}
#[test]
fn a_whole_poll_shares_one_10_second_deadline() {
    let delay = Duration::from_secs(4);
    let (url, seen) = controller(vec![
        (delay, Some(classic_login())),
        (delay, Some(http("401 Unauthorized", "", ""))),
        (delay, Some(classic_login())),
        (delay, Some(http("200 OK", "", HEALTH))),
    ]);
    let mut registry = real(&url);
    let i = registry.get("unifi").unwrap();
    let start = Instant::now();
    i.refresh();
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(11), "poll took {elapsed:?}");
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("UniFi didn't answer within 10 s. Check the controller, then retry.")
    );
    assert_eq!(seen.lock().unwrap().len(), 3);
}
#[test]
fn vault_credentials_bound_to_another_origin_are_never_sent() {
    let vault = Arc::new(MemorySecrets::default());
    let script = Arc::new(Script::default());
    let fetch = script.clone();
    let mut registry = Registry::new(Environment {
        secrets: vault.clone(),
        config_file: None,
        fetch: Arc::new(move || Box::new(Transport(fetch.clone()))),
    })
    .unwrap();
    let i = registry.get("unifi").unwrap();
    i.configure(config(false)).unwrap();
    for field in ["username", "password"] {
        vault
            .put(
                "unifi",
                field,
                Secret {
                    origin: "https://other.example.invalid".into(),
                    value: "synthetic-value".into(),
                },
            )
            .unwrap();
    }
    i.refresh();
    assert_eq!(i.summary().status.state, "not_configured");
    assert!(script.calls.lock().unwrap().is_empty());
}
#[test]
fn an_unreadable_reply_has_its_own_message() {
    let (mut registry, _) = setup(false, vec![login(false), Err(Failure::InvalidReply)]);
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("UniFi sent a reply Open TouchDeck can't read. Check the controller version.")
    );
}
#[test]
fn a_pin_needs_an_https_address() {
    let (mut registry, script) = setup(false, vec![]);
    let i = registry.get("unifi").unwrap();
    let mut c = config(false);
    c["url"] = json!("http://127.0.0.1:8443");
    c["tls_pin"] = json!("ab".repeat(32));
    let Err(Error::ConfigField { field, message }) = i.configure(c) else {
        panic!("a pin over http was accepted");
    };
    assert_eq!(
        (field, message),
        (
            "tls_pin",
            "A certificate pin needs an https:// address. Change the address or clear the pin."
        )
    );
    assert!(script.calls.lock().unwrap().is_empty());
}
#[test]
fn a_pin_change_drops_credentials_and_session() {
    let (mut registry, script) = setup(
        false,
        vec![login(false), response(200, HEALTH), response(200, "{}")],
    );
    let i = registry.get("unifi").unwrap();
    i.refresh();
    let mut changed = config(false);
    changed["tls_pin"] = json!("ab".repeat(32));
    let view = i.configure(changed).unwrap();
    assert!(!view.secrets["username"].set && !view.secrets["password"].set);
    assert_eq!(wait_for_calls(&script, 3), 3);
    assert_eq!(script.calls.lock().unwrap()[2].tls_pin, None);
    i.refresh();
    assert_eq!(script.calls.lock().unwrap().len(), 3);
    assert!(i.live().is_empty());
}
#[test]
fn a_unifi_os_login_without_a_csrf_token_is_refused() {
    let mut reply = login(true).unwrap();
    reply.csrf_token = None;
    let (mut registry, script) = setup(true, vec![Ok(reply)]);
    let i = registry.get("unifi").unwrap();
    i.refresh();
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("UniFi sent a reply Open TouchDeck can't read. Check the controller version.")
    );
    assert_eq!(script.calls.lock().unwrap().len(), 1);
}
