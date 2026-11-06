use super::*;
use serde_json::json;

fn doc(root: Value) -> Value {
    json!({"spaces": [{"id": "space", "folder": "/project", "name": "test", "tabs": [
        {"id": "tab", "name": "", "root": root}
    ]}]})
}

fn block() -> Value {
    json!({"type": "block", "content": {"terminal": "t1"}})
}

#[test]
fn document_size_tab_count_tree_depth_and_pane_count_are_bounded() {
    let mut large = doc(block());
    large["extra"] = json!("x".repeat(MAX_BYTES));
    assert!(Spaces::read(large).is_none());
    let mut tabs = doc(block());
    let tab = tabs["spaces"][0]["tabs"][0].clone();
    tabs["spaces"][0]["tabs"] = json!(vec![tab; MAX_TABS + 1]);
    assert!(Spaces::read(tabs).is_none());
    let mut deep = block();
    for _ in 0..17 {
        deep = json!({"type": "row", "children": [deep]});
    }
    assert!(Spaces::read(doc(deep)).is_none());
    let wide = json!({"type": "row", "children": vec![block(); 17]});
    assert!(Spaces::read(doc(wide)).is_none());
    assert!(Spaces::read(doc(block())).is_some());
}

#[test]
fn automatic_names_use_the_repository_before_the_current_subfolder() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let repo = gix::init(&root).unwrap();
    drop(repo);
    let child = root.join("src");
    std::fs::create_dir(&child).unwrap();
    assert_eq!(Names::default().name(child.to_str().unwrap()), "project");
    assert_eq!(Names::default().name(""), "~");
    let plain = dir.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(Names::default().name(plain.to_str().unwrap()), "plain");
}
