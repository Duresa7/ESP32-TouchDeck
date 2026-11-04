use std::time::{Duration, Instant};

use td_companion::{
    api::NoQuery,
    devices::{DeviceId, LinkState},
    features::{
        integrations::List,
        layouts::{LayoutBody, LayoutPath, Push},
    },
    jobs::JobState,
    testing::TestCompanion,
};
use td_core::{Layout, live::LiveValue};
use td_host::integrations::LiveSample;

const GRAPH: &[u8] = include_bytes!("../../../../schema/examples/graph.json");

fn sample(stream: &str, sample: u64, value: f64) -> LiveSample {
    LiveSample {
        account: String::new(),
        stream: stream.into(),
        sample,
        ttl_ms: 5_000,
        value: LiveValue::Number(value),
    }
}

#[tokio::test]
async fn a_graph_layout_gets_only_its_bound_stream_about_once_a_second() {
    let td = TestCompanion::start().await;
    let device = td.plug_device("mock0001").await;
    let graph = Layout::parse(GRAPH).unwrap();
    let job = td
        .call::<Push>(
            LayoutPath {
                device: device.as_str().into(),
                orientation: graph.orientation,
            },
            NoQuery,
            LayoutBody::new(&graph),
        )
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);

    let deck = td.deck("mock0001");
    for n in 1..=4 {
        td.live_sample("this_pc", sample("cpu_pct", n, n as f64 * 10.0));
        td.live_sample("this_pc", sample("ram_pct", n, 50.0));
        let deadline = Instant::now() + Duration::from_secs(3);
        while deck.live("page", "cpu", "value").map(|live| live.n) < Some(n as u32) {
            assert!(Instant::now() < deadline, "sample {n} never arrived");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let sets = deck.live_sets();
    assert!(
        sets.iter()
            .all(|set| set["tile"] == "cpu" && set["stream"] == "value")
    );
    let values: Vec<f64> = sets
        .iter()
        .map(|set| set["value"].as_f64().unwrap())
        .collect();
    assert_eq!(values, [10.0, 20.0, 30.0, 40.0]);
    let at: Vec<u64> = sets
        .iter()
        .map(|set| set["at_ms"].as_u64().unwrap())
        .collect();
    for gap in at.windows(2).map(|pair| pair[1] - pair[0]) {
        assert!(
            (700..=1_600).contains(&gap),
            "updates {gap} ms apart: {at:?}"
        );
    }
    let shown = deck.live("page", "cpu", "value").unwrap();
    assert_eq!(
        (shown.samples, shown.stale),
        (vec![10.0, 20.0, 30.0, 40.0], false)
    );

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let list = td.call::<List>((), NoQuery, ()).await.unwrap();
        let this_pc = list.iter().find(|i| i.id == "this_pc").unwrap();
        let delivery = this_pc
            .status
            .delivery
            .iter()
            .find(|d| d.device == device.as_str());
        if let Some(delivery) = delivery
            && delivery.sent == 4
        {
            assert_eq!(delivery.error, None);
            break;
        }
        assert!(Instant::now() < deadline, "delivery never reported 4 sent");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn push_graph(td: &TestCompanion, device: &DeviceId) {
    let graph = Layout::parse(GRAPH).unwrap();
    let job = td
        .call::<Push>(
            LayoutPath {
                device: device.as_str().into(),
                orientation: graph.orientation,
            },
            NoQuery,
            LayoutBody::new(&graph),
        )
        .await
        .unwrap();
    assert_eq!(td.wait(&job).await.state, JobState::Succeeded);
}

fn since_ms(td: &TestCompanion, device: &DeviceId) -> Option<u64> {
    match td.device(device)?.link {
        LinkState::Connected { since_ms, .. } => Some(since_ms),
        _ => None,
    }
}

fn hellos(deck: &td_link::MockDeck) -> usize {
    deck.received()
        .iter()
        .filter(|request| request["m"] == "hello" && request["proto"] == 2)
        .count()
}

#[tokio::test]
async fn a_session_the_device_dropped_comes_back_and_this_pc_values_flow_again() {
    let td = TestCompanion::start().await;
    let device = td.plug_device("mock0001").await;
    push_graph(&td, &device).await;
    let deck = td.deck("mock0001");
    let shows = |value: f64| {
        deck.live("page", "cpu", "value")
            .is_some_and(|live| !live.stale && live.samples.last() == Some(&value))
    };
    td.live_sample("this_pc", sample("cpu_pct", 1, 10.0));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !shows(10.0) {
        assert!(Instant::now() < deadline, "the first value never arrived");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let first = since_ms(&td, &device).unwrap();
    let hellos_before = hellos(&deck);

    deck.drop_session();
    td.live_sample("this_pc", sample("cpu_pct", 2, 70.0));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !shows(70.0) {
        assert!(
            Instant::now() < deadline,
            "values never flowed again after the Device dropped the session"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let again = since_ms(&td, &device).expect("the Device is connected again");
    assert!(again > first, "a new session, not the dropped one");
    assert_eq!(hellos(&deck), hellos_before + 1);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let list = td.call::<List>((), NoQuery, ()).await.unwrap();
        let this_pc = list.iter().find(|i| i.id == "this_pc").unwrap();
        if let Some(delivery) = this_pc
            .status
            .delivery
            .iter()
            .find(|d| d.device == device.as_str())
            && delivery.error.is_none()
            && delivery.sent >= 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "This PC's delivery status never cleared"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_device_that_keeps_refusing_sessions_is_retried_with_backoff_not_a_storm() {
    let td = TestCompanion::start().await;
    let device = td.plug_device("mock0001").await;
    push_graph(&td, &device).await;
    let deck = td.deck("mock0001");
    let hellos_before = hellos(&deck);

    deck.refuse_sessions(true);
    deck.drop_session();
    let watched = Instant::now();
    let mut sample_n = 1;
    while watched.elapsed() < Duration::from_secs(8) {
        td.live_sample("this_pc", sample("cpu_pct", sample_n, sample_n as f64));
        sample_n += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let retries = hellos(&deck) - hellos_before;
    assert!(
        (2..=7).contains(&retries),
        "{retries} new sessions in 8 s; the backoff allows 250, 500, 1000, 2000 and 4000 ms waits"
    );
    assert!(
        deck.live("page", "cpu", "value")
            .is_none_or(|live| live.stale)
    );

    deck.refuse_sessions(false);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !deck
        .live("page", "cpu", "value")
        .is_some_and(|live| !live.stale)
    {
        assert!(
            Instant::now() < deadline,
            "the Device never came back once it accepted sessions again"
        );
        td.live_sample("this_pc", sample("cpu_pct", sample_n, 1.0));
        sample_n += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
