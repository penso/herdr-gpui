use super::*;

#[test]
fn px0_is_a_separate_singleton_per_daemon_and_workspace() {
    let mut store = Store::default();
    let local = Scope::local("/tmp/local.sock".as_ref());
    let remote = Scope::endpoint("other");
    let first = store
        .open_editor_tab(local.clone(), "w1", Editor::Px0, None)
        .unwrap();
    assert_eq!(
        Some(first),
        store.open_editor_tab(local.clone(), "w1", Editor::Px0, None)
    );
    let code = store
        .open_editor_tab(local.clone(), "w1", Editor::VsCode, None)
        .unwrap();
    assert_ne!(first, code);
    assert_ne!(
        first,
        store
            .open_editor_tab(remote, "w1", Editor::Px0, None)
            .unwrap()
    );
    assert_ne!(
        first,
        store
            .open_editor_tab(local.clone(), "w2", Editor::Px0, None)
            .unwrap()
    );
    let closed = store.close(first).unwrap();
    let second = store
        .open_editor_tab(local.clone(), "w1", Editor::Px0, None)
        .unwrap();
    assert_eq!(store.reopen_page(closed), Some(second));
    let saved = serde_json::to_vec(&Saved {
        tabs: store.tabs.clone(),
    })
    .unwrap();
    let restored = Store::with_tabs(parse(&saved).unwrap(), None);
    let tab = restored.px0_tab(&local, "w1").unwrap();
    assert!(tab.place.is_editor());
    assert!(tab.origin.is_none());
    assert_eq!(tab.title, "px0");
    assert_eq!(restored.code_tab(&local, "w1").unwrap().id, code);
}
