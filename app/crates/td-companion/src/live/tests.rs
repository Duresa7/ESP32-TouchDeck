use super::*;
use crate::live::fake::FakeControl;
use crate::photos::encode::tests::noise_png;
use std::collections::BTreeMap;

#[test]
fn the_cover_thread_makes_only_covers_for_current_samples_then_wakes_a_round() {
    let shared = Arc::new(Shared::default());
    let fake = Arc::new(FakeControl::default());
    fake.add_picture("sha256:old", noise_png(100, 100, 1));
    fake.add_picture("sha256:new", noise_png(100, 100, 2));
    fake.publish(&[("cover", LiveValue::Image(Some("sha256:old".into())))]);
    let old = fake.samples().remove(0);
    fake.publish(&[("cover", LiveValue::Image(Some("sha256:new".into())))]);
    let new = fake.samples().remove(0);
    let source = StreamKey {
        integration: "media".into(),
        account: String::new(),
        stream: "cover".into(),
    };
    let job = |sample: &LiveSample| CoverJob {
        source: source.clone(),
        sample: sample.clone(),
        side: 64,
    };
    let controls: Controls = Arc::new(BTreeMap::from([(
        "media".to_owned(),
        fake.clone() as Arc<dyn td_host::integrations::Control>,
    )]));
    let (jobs, queue) = sync_channel(1);
    jobs.send(vec![job(&old), job(&new)]).unwrap();
    drop(jobs);
    make_covers(Arc::downgrade(&shared), controls, queue);
    let covers = shared.covers.lock().clone();
    assert_eq!(covers.len(), 1);
    let made = covers[&(source, new.sample, 64)].as_ref().unwrap();
    assert_eq!(image::load_from_memory(&made.bytes).unwrap().width(), 64);
    assert!(*shared.wake.lock());
}

#[test]
fn review_corrupt_picture_is_requested_once_across_rounds() {
    let shared = Arc::new(Shared::default());
    let fake = Arc::new(FakeControl::default());
    fake.add_picture("bad", b"broken".to_vec());
    fake.publish(&[("cover", LiveValue::Image(Some("bad".into())))]);
    let job = CoverJob {
        source: StreamKey {
            integration: "media".into(),
            account: String::new(),
            stream: "cover".into(),
        },
        sample: fake.samples().remove(0),
        side: 64,
    };
    let controls: Controls = Arc::new(BTreeMap::from([(
        "media".into(),
        fake.clone() as Arc<dyn td_host::integrations::Control>,
    )]));
    let (jobs, queue) = sync_channel(20);
    for _ in 0..20 {
        jobs.send(vec![job.clone()]).unwrap();
    }
    drop(jobs);
    make_covers(Arc::downgrade(&shared), controls, queue);
    assert_eq!(*fake.picture_calls.lock(), 1);
}
