use super::*;

#[test]
fn orchestrator_tabs_saved_by_earlier_builds_are_dropped_on_load() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let page = store
        .open(scope.clone(), "w_1", url("https://a.test/"), None)
        .unwrap();
    let orchestrator = store
        .open(
            scope,
            "w_1",
            Some(Location::Orchestrator {
                repo: crate::browser::OrchestratorRepo::new("/tmp/repo".into()).unwrap(),
            }),
            None,
        )
        .unwrap();
    let bytes = serde_json::to_vec(&Saved {
        tabs: store.tabs.clone(),
    })
    .unwrap();
    // The file still reads, so the user's other tabs come back.
    let restored = Store::with_tabs(parse(&bytes).unwrap(), None);
    assert!(restored.get(page).is_some());
    assert!(restored.get(orchestrator).is_none());
}
