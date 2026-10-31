use std::sync::Arc;
use td_host::integrations::{Environment, Registry, secrets::MemorySecrets};
use td_host::sources::network::{FetchError, GetRequest};
#[test]
fn custom_is_available_with_disabled_empty_config() {
    let mut registry = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(|| Box::new(td_host::sources::HttpFetch::new())),
    })
    .unwrap();
    let custom = registry
        .get("custom")
        .expect("Custom Integration registered");
    let view = custom.view().unwrap();
    assert_eq!(
        view.config,
        serde_json::json!({"enabled":false,"sources":[]})
    );
    assert!(view.summary.presets.is_empty());
}

#[derive(Clone)]
struct Reply(&'static str);
impl td_host::sources::Fetch for Reply {
    fn get(&self, _: &str, _: &str) -> Result<String, String> {
        panic!("GET must use bounded transport");
    }
    fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<String, String> {
        panic!("POST forbidden");
    }
    fn request(&self, _: &GetRequest<'_>) -> Result<String, FetchError> {
        Ok(self.0.into())
    }
}
fn configured(body: &'static str) -> Registry {
    let mut r = Registry::new(Environment {
        secrets: Arc::new(MemorySecrets::default()),
        config_file: None,
        fetch: Arc::new(move || Box::new(Reply(body))),
    })
    .unwrap();
    r.get("custom")
        .unwrap()
        .configure(config("https://example.invalid/metrics"))
        .unwrap();
    r
}
fn config(url: &str) -> serde_json::Value {
    serde_json::json!({"enabled":true,"sources":[{"id":"temperature","name":"Temperature","interval_s":1,"kind":"number","input":{"type":"http","url":url,"value_path":"$.readings[0].value","headers":{"Authorization":"auth"}}}]})
}
#[test]
fn recorded_http_reply_produces_live_samples_and_ordered_presets() {
    let mut r = configured(r#"{"readings":[{"value":23.5}]}"#);
    let i = r.get("custom").unwrap();
    i.secret_put("auth", "synthetic-header").unwrap();
    i.refresh();
    assert_eq!(i.live()[0].value, td_core::live::LiveValue::Number(23.5));
    assert_eq!(i.live()[0].stream, "temperature");
    assert_eq!(i.live()[0].account, "");
    assert_eq!(i.live()[0].ttl_ms, 3000);
    let first = i.live()[0].sample;
    i.refresh();
    assert_eq!(i.live()[0].sample, first);
    let presets = i.summary().presets;
    assert_eq!(
        presets.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["Temperature value", "Temperature graph"]
    );
    assert!(
        !serde_json::to_string(&i.view().unwrap())
            .unwrap()
            .contains("synthetic-header")
    );
}
#[test]
fn origins_clear_secrets_and_malformed_values_never_emit_samples() {
    for body in [
        "not json",
        r#"{"readings":[{"value":"23.5"}]}"#,
        r#"{"readings":[]}"#,
        r#"{"readings":[{"value":1000000001}]}"#,
    ] {
        let mut r = configured(body);
        let i = r.get("custom").unwrap();
        i.secret_put("auth", "synthetic-header").unwrap();
        i.refresh();
        assert!(i.live().is_empty());
        assert_eq!(i.summary().status.state, "error");
        assert!(
            !serde_json::to_string(&i.view().unwrap())
                .unwrap()
                .contains("synthetic-header")
        );
        assert!(
            i.configure(config("https://example.invalid/other"))
                .unwrap()
                .secrets["auth"]
                .set
        );
        assert!(
            !i.configure(config("https://other.example.invalid/"))
                .unwrap()
                .secrets["auth"]
                .set
        );
    }
}

#[test]
fn failures_have_safe_status_and_bounds_are_enforced() {
    #[derive(Clone)]
    struct Failed(FetchError);
    impl td_host::sources::Fetch for Failed {
        fn get(&self, _: &str, _: &str) -> Result<String, String> {
            unreachable!()
        }
        fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<String, String> {
            unreachable!()
        }
        fn request(&self, _: &GetRequest<'_>) -> Result<String, FetchError> {
            Err(self.0)
        }
    }
    for (error, message) in [
        (
            FetchError::Unauthorized,
            "The server refused access. Check the secret headers.",
        ),
        (FetchError::Timeout, "The server didn't answer within 10 s."),
        (FetchError::TooLarge, "The HTTP reply is over 1 MiB."),
        (
            FetchError::Tls,
            "Can't verify the server certificate. Check the TLS fingerprint.",
        ),
        (
            FetchError::Redirect,
            "The server redirected. Use its final address.",
        ),
        (
            FetchError::Transport,
            "Couldn't fetch the value. Check the HTTP address.",
        ),
    ] {
        let mut r = Registry::new(Environment {
            secrets: Arc::new(MemorySecrets::default()),
            config_file: None,
            fetch: Arc::new(move || Box::new(Failed(error))),
        })
        .unwrap();
        let i = r.get("custom").unwrap();
        i.configure(config("https://example.invalid/")).unwrap();
        i.secret_put("auth", "synthetic-header").unwrap();
        i.tick();
        assert_eq!(i.summary().status.message.as_deref(), Some(message));
        assert!(i.live().is_empty());
        assert!(
            !serde_json::to_string(&i.view().unwrap())
                .unwrap()
                .contains("synthetic-header")
        );
        for interval in [0, 3601] {
            let mut bad = config("https://example.invalid/");
            bad["sources"][0]["interval_s"] = interval.into();
            assert!(i.configure(bad).is_err());
        }
        let mut bad = config("https://example.invalid/");
        bad["sources"][0]["input"]["method"] = "POST".into();
        assert!(i.configure(bad).is_err());
    }
}
#[test]
fn text_is_available_internally_without_device_presets() {
    let mut r = configured(r#"{"readings":[{"value":"Ready"}]}"#);
    let i = r.get("custom").unwrap();
    let mut config = config("https://example.invalid/");
    config["sources"][0]["kind"] = "text".into();
    i.configure(config).unwrap();
    i.secret_put("auth", "synthetic-header").unwrap();
    i.tick();
    assert_eq!(
        i.live()[0].value,
        td_core::live::LiveValue::Text("Ready".into())
    );
    assert!(i.summary().presets.is_empty());
    assert_eq!(i.readings()[0].value, "Ready");
}

struct Grant(Option<(String, Vec<String>)>);
impl td_host::integrations::CommandApproval for Grant {
    fn approved(&self, program: &str, args: &[String]) -> bool {
        self.0
            .as_ref()
            .is_some_and(|(p, a)| p == program && a.as_slice() == args)
    }
}
#[test]
fn a_command_runs_only_when_its_exact_program_and_arguments_are_approved() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom-command.rs");
    assert!(
        std::process::Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&program)
            .status()
            .unwrap()
            .success()
    );
    let program = program.to_str().unwrap().to_owned();
    let marker = dir.path().join("ran");
    let args = vec!["marker".to_owned(), marker.to_str().unwrap().to_owned()];
    let mut changed = args.clone();
    changed.push("--changed".into());
    for (grant, runs) in [
        (None, false),
        (Some(Grant(None)), false),
        (Some(Grant(Some((program.clone(), changed)))), false),
        (Some(Grant(Some((program.clone(), args.clone())))), true),
    ] {
        let mut r = Registry::new(Environment {
            secrets: Arc::new(MemorySecrets::default()),
            config_file: None,
            fetch: Arc::new(|| Box::new(Reply("{}"))),
        })
        .unwrap();
        let i = r.get("custom").unwrap();
        if let Some(grant) = grant {
            i.command_approval(Arc::new(grant));
        }
        i.configure(serde_json::json!({"enabled":true,"sources":[{"id":"load","name":"Load","interval_s":1,"kind":"number","input":{"type":"command","program":program,"args":args,"value_path":null}}]}))
            .unwrap();
        i.tick();
        assert_eq!(marker.exists(), runs);
        if runs {
            assert_eq!(i.live()[0].value, td_core::live::LiveValue::Number(42.0));
        } else {
            assert!(i.live().is_empty());
            assert_eq!(
                i.summary().status.message.as_deref(),
                Some("Waiting for approval on this PC.")
            );
        }
    }
}

fn shared_registry(secrets: Arc<MemorySecrets>) -> Registry {
    Registry::new(Environment {
        secrets,
        config_file: None,
        fetch: Arc::new(|| Box::new(td_host::sources::HttpFetch::new())),
    })
    .unwrap()
}
fn http_source(url: &str, pin: Option<&str>, interval_s: u64) -> serde_json::Value {
    serde_json::json!({"enabled":true,"sources":[{"id":"s","name":"S","interval_s":interval_s,"kind":"number","input":{"type":"http","url":url,"tls_pin":pin,"value_path":"$.v","headers":{"Authorization":"auth"}}}]})
}
fn recording_server(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v", listener.local_addr().unwrap());
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap_or(0);
            let _ = stream.write_all(reply.as_bytes());
            if sent
                .send(String::from_utf8_lossy(&request[..n]).into_owned())
                .is_err()
            {
                return;
            }
        }
    });
    (url, received)
}
fn fixture_program(dir: &std::path::Path) -> String {
    let program = dir.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/custom-command.rs");
    assert!(
        std::process::Command::new("rustc")
            .arg(source)
            .arg("-o")
            .arg(&program)
            .status()
            .unwrap()
            .success()
    );
    program.to_str().unwrap().to_owned()
}

#[test]
fn a_trickled_http_body_ends_at_the_deadline() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = stream.read(&mut [0; 4096]);
        let body = b"{\"v\":123456.0}";
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );
        for byte in body {
            if stream.write_all(&[*byte]).is_err() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    let mut r = shared_registry(Arc::default());
    let i = r.get("custom").unwrap();
    i.configure(serde_json::json!({"enabled":true,"sources":[{"id":"slow","name":"Slow","interval_s":1,"kind":"number","input":{"type":"http","url":url,"value_path":"$.v"}}]}))
        .unwrap();
    let start = std::time::Instant::now();
    i.tick();
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(11_500),
        "{elapsed:?}"
    );
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("The server didn't answer within 10 s.")
    );
    assert!(i.live().is_empty());
}

#[test]
fn changing_or_clearing_the_pin_deletes_the_secret_headers() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use td_host::integrations::secrets::SecretStore;
    let rogue = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let pin: String = Sha256::digest(rogue.cert.der().as_ref())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![rogue.cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(rogue.signing_key.serialize_der()).into(),
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "https://localhost:{}/v",
        listener.local_addr().unwrap().port()
    );
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        let connection = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut tls = rustls::StreamOwned::new(connection, socket);
        let mut request = [0; 4096];
        let n = tls.read(&mut request).unwrap_or(0);
        let _ = tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\n{\"v\":7}");
        let _ = sent.send(String::from_utf8_lossy(&request[..n]).into_owned());
    });
    let secrets = Arc::new(MemorySecrets::default());
    let mut r = shared_registry(secrets.clone());
    let i = r.get("custom").unwrap();
    i.configure(http_source(&url, None, 1)).unwrap();
    i.secret_put("auth", "Bearer synthetic-sentinel").unwrap();
    let view = i.configure(http_source(&url, Some(&pin), 1)).unwrap();
    assert!(!view.secrets["auth"].set);
    assert!(secrets.get("custom", "auth").unwrap().is_none());
    i.refresh();
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("Enter the secret headers.")
    );
    assert!(
        received
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_err()
    );
    i.secret_put("auth", "Bearer synthetic-sentinel").unwrap();
    let view = i.configure(http_source(&url, None, 1)).unwrap();
    assert!(!view.secrets["auth"].set);
    assert!(secrets.get("custom", "auth").unwrap().is_none());
}

#[test]
fn a_pin_needs_an_https_address() {
    let mut r = shared_registry(Arc::default());
    let i = r.get("custom").unwrap();
    let pin = "ab".repeat(32);
    let Err(td_host::integrations::Error::ConfigField { field, message }) =
        i.configure(http_source("http://127.0.0.1:8080/v", Some(&pin), 1))
    else {
        panic!("a pin over plain HTTP was accepted");
    };
    assert_eq!(
        (field, message),
        (
            "sources",
            "A certificate pin needs an https:// address. Change the address or clear the pin."
        )
    );
    i.configure(http_source("https://example.invalid/v", Some(&pin), 1))
        .unwrap();
}

#[test]
fn a_secret_bound_to_another_origin_is_never_sent() {
    use td_host::integrations::secrets::{Secret, SecretStore};
    let (url, received) = recording_server("HTTP/1.1 200 OK\r\nContent-Length: 7\r\n\r\n{\"v\":7}");
    let secrets = Arc::new(MemorySecrets::default());
    let mut r = shared_registry(secrets.clone());
    let i = r.get("custom").unwrap();
    i.configure(http_source(&url, None, 1)).unwrap();
    secrets
        .put(
            "custom",
            "auth",
            Secret {
                origin: "https://other.example.invalid".into(),
                value: "Bearer synthetic-sentinel".into(),
            },
        )
        .unwrap();
    i.tick();
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("Enter the secret headers.")
    );
    assert!(
        received
            .recv_timeout(std::time::Duration::from_millis(500))
            .is_err()
    );
}

#[test]
fn an_origin_change_deletes_the_stored_secret() {
    use td_host::integrations::secrets::SecretStore;
    let secrets = Arc::new(MemorySecrets::default());
    let mut r = shared_registry(secrets.clone());
    let i = r.get("custom").unwrap();
    i.configure(http_source("https://example.invalid/v", None, 1))
        .unwrap();
    i.secret_put("auth", "Bearer synthetic-sentinel").unwrap();
    i.configure(http_source("https://example.invalid/other", None, 1))
        .unwrap();
    assert!(secrets.get("custom", "auth").unwrap().is_some());
    i.configure(http_source("https://other.example.invalid/v", None, 1))
        .unwrap();
    assert!(secrets.get("custom", "auth").unwrap().is_none());
}

#[test]
fn a_failing_source_backs_off_between_polls() {
    let (url, received) = recording_server("HTTP/1.1 500 Error\r\nContent-Length: 0\r\n\r\n");
    let mut r = shared_registry(Arc::default());
    let i = r.get("custom").unwrap();
    let mut config = http_source(&url, None, 1);
    config["sources"][0]["input"]["headers"] = serde_json::json!({});
    i.configure(config).unwrap();
    let start = std::time::Instant::now();
    let mut polls = Vec::new();
    while start.elapsed() < std::time::Duration::from_millis(6500) {
        i.tick();
        while received.try_recv().is_ok() {
            polls.push(start.elapsed().as_millis() / 100);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let gaps: Vec<_> = polls.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(polls.len(), 3, "{polls:?}");
    assert!(gaps[0] >= 19 && gaps[1] >= 39, "{gaps:?}");
}

struct Answer(std::sync::atomic::AtomicBool);
impl td_host::integrations::CommandApproval for Answer {
    fn approved(&self, _: &str, _: &[String]) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
#[test]
fn waiting_for_approval_does_not_back_off_and_a_late_answer_runs_promptly() {
    let dir = tempfile::tempdir().unwrap();
    let program = fixture_program(dir.path());
    let mut r = shared_registry(Arc::default());
    let i = r.get("custom").unwrap();
    let answer = Arc::new(Answer(false.into()));
    i.command_approval(answer.clone());
    i.configure(serde_json::json!({"enabled":true,"sources":[{"id":"load","name":"Load","interval_s":200,"kind":"number","input":{"type":"command","program":program,"args":["value"],"value_path":null}}]}))
        .unwrap();
    for _ in 0..3 {
        i.tick();
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    assert_eq!(
        i.summary().status.message.as_deref(),
        Some("Waiting for approval on this PC.")
    );
    answer.0.store(true, std::sync::atomic::Ordering::SeqCst);
    i.tick();
    assert_eq!(i.live()[0].value, td_core::live::LiveValue::Number(42.0));
}
