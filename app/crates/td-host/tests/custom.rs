use std::sync::Arc;
use td_host::integrations::{Environment, Registry, secrets::MemorySecrets};
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
    fn custom_get(
        &self,
        _: &str,
        _: &[(String, String)],
        _: Option<&str>,
    ) -> Result<String, td_host::sources::custom_http::HttpError> {
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
    use td_host::sources::custom_http::HttpError;
    #[derive(Clone)]
    struct Failed(HttpError);
    impl td_host::sources::Fetch for Failed {
        fn get(&self, _: &str, _: &str) -> Result<String, String> {
            unreachable!()
        }
        fn post_json(&self, _: &str, _: &str, _: &serde_json::Value) -> Result<String, String> {
            unreachable!()
        }
        fn custom_get(
            &self,
            _: &str,
            _: &[(String, String)],
            _: Option<&str>,
        ) -> Result<String, HttpError> {
            Err(self.0)
        }
    }
    for error in [
        HttpError::Credentials,
        HttpError::Timeout,
        HttpError::Oversized,
        HttpError::Tls,
        HttpError::Redirect,
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
        assert_eq!(i.summary().status.message.as_deref(), Some(error.message()));
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
