use serde_json::json;
use td_companion::{
    api::NoQuery,
    features::{
        assets::{Create, CreateAsset},
        jobs::{Get, JobPath, List},
    },
    testing::TestCompanion,
};

#[tokio::test]
async fn finished_jobs_are_kept_fifty_for_ten_minutes() {
    let td = TestCompanion::start().await;
    // An open socket keeps the window session alive across the clock jump.
    let _events = td.events().await;
    let create = || -> CreateAsset {
        serde_json::from_value(json!({"source": {"kind": "app", "path": "/no/such/program"}}))
            .unwrap()
    };
    let mut ids = Vec::new();
    for _ in 0..51 {
        let job = td.call::<Create>((), NoQuery, create()).await.unwrap();
        td.wait(&job).await;
        ids.push(job.id);
    }
    let listed: Vec<String> = td
        .call::<List>((), NoQuery, ())
        .await
        .unwrap()
        .into_iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(listed, ids[1..]);
    let first = JobPath {
        job: ids[0].clone(),
    };
    let gone = td.request::<Get>(first, NoQuery, &()).send().await.unwrap();
    assert_eq!(gone.status(), 404);

    td.clock().advance(std::time::Duration::from_secs(601));
    assert!(td.call::<List>((), NoQuery, ()).await.unwrap().is_empty());
}
