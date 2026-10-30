use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use td_host::integrations::{
    Integration,
    this_pc::{CounterSample, CounterSource, ThisPc},
};
struct Fake {
    calls: Arc<AtomicUsize>,
    gpu: bool,
}
impl CounterSource for Fake {
    fn collect(&mut self) -> CounterSample {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) as u64;
        let mut values = BTreeMap::from([
            ("cpu_pct".into(), 25.0),
            ("ram_pct".into(), 50.0),
            ("ram_used_bytes".into(), 512.0),
            ("ram_total_bytes".into(), 1024.0),
            ("disk_active_pct".into(), 10.0),
        ]);
        if self.gpu {
            values.insert("gpu_pct".into(), 30.0);
        }
        CounterSample {
            values,
            totals: [
                "disk_read_bytes_s",
                "disk_write_bytes_s",
                "network_receive_bytes_s",
                "network_send_bytes_s",
            ]
            .into_iter()
            .map(|id| (id.into(), 1000 + n * 400))
            .collect(),
        }
    }
}
fn pc(gpu: bool) -> (ThisPc, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        ThisPc::new(Box::new(Fake {
            calls: calls.clone(),
            gpu,
        })),
        calls,
    )
}
#[test]
fn counter_values_have_units_and_rates_start_with_the_second_sample() {
    let (mut pc, _) = pc(true);
    let now = Instant::now();
    pc.sample_at(now);
    assert_eq!(pc.readings().len(), 6);
    assert_eq!(pc.readings()[0].value, "25.0 %");
    assert_eq!(pc.readings()[0].bar, 0.25);
    assert_eq!(pc.readings()[2].value, "512.0 bytes");
    pc.sample_at(now + Duration::from_secs(2));
    assert_eq!(pc.readings().len(), 10);
    assert_eq!(pc.readings()[6].value, "200.0 bytes/s");
    assert_eq!(pc.readings()[9].value, "200.0 bytes/s");
}
#[test]
fn missing_gpu_does_not_suppress_other_streams() {
    let (mut pc, _) = pc(false);
    pc.sample_at(Instant::now());
    assert_eq!(
        pc.summary().status.message.as_deref(),
        Some("Unavailable: GPU usage.")
    );
    assert!(pc.readings().iter().any(|r| r.field == "cpu_pct"));
    assert!(!pc.readings().iter().any(|r| r.field == "gpu_pct"));
}
#[test]
fn disabled_collects_nothing_and_resume_resets_rate_baseline() {
    let (mut pc, calls) = pc(true);
    pc.configure(json!({"enabled":false})).unwrap();
    pc.tick();
    pc.refresh();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    pc.configure(json!({"enabled":true})).unwrap();
    let now = Instant::now();
    pc.sample_at(now);
    pc.sample_at(now + Duration::from_millis(999));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    pc.sample_at(now + Duration::from_secs(1));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    pc.configure(json!({"enabled":false})).unwrap();
    assert!(pc.readings().is_empty());
    pc.configure(json!({"enabled":true})).unwrap();
    pc.tick();
    assert!(!pc.readings().iter().any(|r| r.field == "disk_read_bytes_s"));
}
#[test]
fn presets_follow_copy_order_and_layout_action_contract() {
    let (pc, _) = pc(true);
    let summary = pc.summary();
    assert_eq!(
        summary
            .presets
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        [
            "CPU usage",
            "RAM usage",
            "RAM used",
            "RAM total",
            "GPU usage",
            "Disk activity",
            "Disk read",
            "Disk write",
            "Network receive",
            "Network send"
        ]
    );
    for preset in summary.presets {
        let action: td_core::layout::Action = serde_json::from_value(preset.action).unwrap();
        assert!(
            matches!(action, td_core::layout::Action::Widget { source, field, account } if source == "this_pc" && field == preset.id && account.is_empty())
        );
    }
}

#[test]
fn invalid_config_and_secret_writes_leave_config_unchanged() {
    let (mut pc, _) = pc(true);
    for config in [
        json!({}),
        json!({"enabled":"yes"}),
        json!({"enabled":true,"extra":1}),
    ] {
        assert!(pc.configure(config).is_err());
    }
    assert!(pc.secret_put("key", "fixture").is_err());
    assert!(pc.secret_delete("key").is_err());
    assert_eq!(pc.view().unwrap().config, json!({"enabled":true}));
    assert!(pc.view().unwrap().secrets.is_empty());
}

#[test]
fn unavailable_platform_has_no_readings_and_explains_why() {
    struct Unsupported;
    impl CounterSource for Unsupported {
        fn collect(&mut self) -> CounterSample {
            CounterSample::default()
        }
        fn supported(&self) -> bool {
            false
        }
    }
    let mut pc = ThisPc::new(Box::new(Unsupported));
    pc.tick();
    assert!(pc.readings().is_empty());
    assert_eq!(
        pc.summary().status.message.as_deref(),
        Some("This PC needs Windows.")
    );
}

#[test]
fn counter_reset_omits_rate_then_uses_the_new_baseline() {
    struct Reset(usize);
    impl CounterSource for Reset {
        fn collect(&mut self) -> CounterSample {
            let total = [1000, 100, 500][self.0];
            self.0 += 1;
            CounterSample {
                totals: BTreeMap::from([("disk_read_bytes_s".into(), total)]),
                ..Default::default()
            }
        }
    }
    let mut pc = ThisPc::new(Box::new(Reset(0)));
    let now = Instant::now();
    pc.sample_at(now);
    pc.sample_at(now + Duration::from_secs(1));
    assert!(pc.readings().is_empty());
    pc.sample_at(now + Duration::from_secs(3));
    assert_eq!(pc.readings()[0].value, "200.0 bytes/s");
}
