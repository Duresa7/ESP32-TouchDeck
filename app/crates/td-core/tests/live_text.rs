use serde_json::Value;
use td_core::live::fit_text;

#[test]
fn fit_text_turns_every_shared_vector_into_its_output() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../../schema/live/text-vectors.json"
    ))
    .unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    for kind in ["clean", "fold", "cut"] {
        assert!(cases.iter().any(|case| case["kind"] == kind), "{kind}");
    }
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let max = usize::try_from(case["max_bytes"].as_u64().unwrap()).unwrap();
        let fitted = fit_text(case["input"].as_str().unwrap(), max);
        assert_eq!(fitted, case["output"].as_str().unwrap(), "{name}");
        assert!(fitted.len() <= max, "{name}");
    }
}
