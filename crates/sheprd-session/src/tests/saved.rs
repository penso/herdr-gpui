use crate::{MAX_SAVED_BYTES, MAX_SAVED_TABS, SavedTabs, saved::Error};
use serde_json::{Value, json};

fn parse(tabs: Value) -> SavedTabs {
    SavedTabs::parse(&serde_json::to_vec(&json!({ "tabs": tabs })).unwrap()).unwrap()
}

fn tab(id: u64, location: Value) -> Value {
    json!({"id": id, "scope": "s", "workspace_id": "w", "title": "t", "location": location})
}

fn ids(saved: &SavedTabs) -> Vec<u64> {
    saved.core_tabs("s").map(|tab| tab.id).collect()
}

#[test]
fn only_typing_core_tabs_of_the_scope_are_listed() {
    let core = json!({"kind": "core_terminal", "panes": "t1"});
    let saved = parse(json!([
        tab(1, core.clone()),
        tab(2, json!({"kind": "core_terminal", "panes": "t2", "observe": true})),
        tab(3, json!({"kind": "web", "url": "https://example.com"})),
        tab(4, Value::Null),
        tab(7, json!({"kind": "review", "panes": "t3"})),
        {"id": 5, "scope": "other", "workspace_id": "w", "title": "t", "location": core.clone()},
        {"id": 6, "scope": "s", "workspace_id": "", "title": "t", "location": core},
    ]));
    assert_eq!(ids(&saved), [1]);
}

#[test]
fn a_tab_from_before_splits_names_one_terminal() {
    let saved = parse(json!([tab(
        1,
        json!({"kind": "core_terminal", "terminal": "t4"})
    )]));
    let tabs: Vec<_> = saved.core_tabs("s").collect();
    assert_eq!(tabs[0].panes.first().as_str(), "t4");
}

#[test]
fn an_unreadable_tab_costs_only_itself() {
    let twice = json!({"axis": "row", "first": "t1", "second": "t1"});
    let saved = parse(json!([
        tab(1, json!({"kind": "core_terminal", "panes": twice})),
        tab(2, json!({"kind": "core_terminal", "panes": "t2"})),
    ]));
    assert_eq!(ids(&saved), [2]);
}

#[test]
fn saved_tabs_are_bounded() {
    let large = vec![b' '; MAX_SAVED_BYTES + 1];
    assert!(matches!(SavedTabs::parse(&large), Err(Error::Large)));
    let tabs = vec![tab(1, Value::Null); MAX_SAVED_TABS + 1];
    let many = serde_json::to_vec(&json!({ "tabs": tabs })).unwrap();
    assert!(matches!(SavedTabs::parse(&many), Err(Error::Many)));
    assert!(matches!(SavedTabs::parse(b"{"), Err(Error::Format(_))));
}
