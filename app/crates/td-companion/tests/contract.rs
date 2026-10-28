#[test]
fn api_bindings_are_current() {
    assert_eq!(
        td_companion::bindings::generate(),
        include_str!("../../../ui/src/companion/api.gen.ts"),
        "run `pnpm gen:api`"
    );
}
#[test]
fn api_contract_is_documented() {
    let doc = include_str!("../../../../docs/companion-api.md");
    for f in td_companion::api::registry() {
        for e in f.endpoints {
            assert!(doc.contains(e.name), "{}", e.name);
        }
    }
    for (name, _, _) in td_companion::bindings::AUTH_ENDPOINTS {
        assert!(doc.contains(name));
    }
    for name in td_companion::bindings::EVENTS
        .iter()
        .chain(td_companion::bindings::ERRORS)
    {
        assert!(doc.contains(name), "{name}");
    }
}
#[test]
fn test_registry_is_consistent() {
    let mut routes = std::collections::HashSet::new();
    for feature in td_companion::api::registry() {
        for e in feature.endpoints {
            assert!(e.name.starts_with(&format!("{}.", feature.name)));
            assert!(routes.insert((e.method.to_string(), e.path)));
            let params: Vec<_> = e
                .path
                .split('/')
                .filter_map(|s| s.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
                .collect();
            for param in &params {
                assert!(e.path_type.contains(&format!("{param}:")));
            }
            assert_eq!(e.path_type.matches(':').count(), params.len());
        }
    }
}
