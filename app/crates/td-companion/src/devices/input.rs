use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Instant;
#[derive(Default)]
pub(super) struct InputHub { pending: BTreeMap<(String, String), Value> }
impl InputHub {
    pub fn push(&mut self, _event: Value) {}
    pub fn take(&mut self) -> Vec<Value> { std::mem::take(&mut self.pending).into_values().collect() }
}
#[derive(Default)]
pub(super) struct PressGate;
impl PressGate { pub fn accept(&mut self, _event: &Value, _now: Instant) -> bool { false } }
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn input(seq: u64, value: f64) -> Value { json!({"m":"v2.input","page":"main","tile":"volume","stream":"value","phase":"move","seq":seq,"value":value}) }
    #[test]
    fn coalesces_clamps_and_rejects_replayed_sequences() {
        let mut hub = InputHub::default();
        hub.push(input(1, 0.2)); hub.push(input(3, 2.0)); hub.push(input(2, 0.1));
        let batch = hub.take(); assert_eq!(batch.len(), 1); assert_eq!(batch[0]["value"], 1.0);
        hub.push(input(3, 0.1)); assert!(hub.take().is_empty());
        let mut end = input(4, -1.0); end["phase"] = json!("end"); hub.push(end);
        let batch = hub.take(); assert_eq!(batch[0]["value"], 0.0); assert_eq!(batch[0]["phase"], "end");
    }
    #[test]
    fn hostile_flood_is_bounded_and_malformed_input_is_dropped() {
        let mut hub = InputHub::default();
        for seq in 1..10000 { let mut e=input(seq, 0.5); e["tile"]=json!(format!("t{seq}")); hub.push(e); }
        assert_eq!(hub.take().len(), 64);
        for (field, value) in [("page",json!("x".repeat(10000))), ("stream",json!("muted")), ("phase",json!("begin")), ("seq",json!(u64::MAX)), ("value",Value::Null)] {
            let mut e=input(10001,0.5); e[field]=value; hub.push(e);
        }
        assert!(hub.take().is_empty());
    }
    #[test]
    fn parts_are_bounded_deduplicated_and_limited_to_ten_per_second() {
        let mut gate=PressGate::default(); let now=Instant::now();
        let mut e=json!({"page":"main","tile":"media","part":"next","seq":1});
        assert!(gate.accept(&e,now)); assert!(!gate.accept(&e,now));
        for seq in 2..=10 { e["seq"]=json!(seq); assert!(gate.accept(&e,now)); }
        e["seq"]=json!(11); assert!(!gate.accept(&e,now));
        e["seq"]=json!(12); assert!(gate.accept(&e,now+std::time::Duration::from_secs(1)));
        e["part"]=json!("launch"); e["seq"]=json!(13); assert!(!gate.accept(&e,now+std::time::Duration::from_secs(2)));
    }
}
