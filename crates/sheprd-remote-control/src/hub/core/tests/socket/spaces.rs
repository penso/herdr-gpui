use super::*;

fn block(address: &str) -> Value {
    json!({"type": "block", "content": {"terminal": address}})
}

fn space(id: &str, name: &str, address: &str) -> Value {
    json!({"id": id, "name": name, "folder": "/project", "tabs": [
        {"id": format!("{id}-tab"), "name": "", "root": block(address)}
    ]})
}

#[test]
fn desktop_spaces_keep_order_tabs_panes_names_and_status() {
    let mut first = space("sp-z", "nexus/workflows", "t1");
    first["tabs"][0]["name"] = json!("build");
    first["tabs"][0]["root"] = json!({"type": "row", "children": [block("t1"),
        {"type": "column", "children": [block("t2"), {"type": "block", "content": {"mod": "notes"}}]}
    ]});
    first["tabs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "second", "name": "", "root": block("t3")}));
    let mut unnamed = space("sp-a", "", "t4");
    unnamed["folder"] = json!("/home/test");
    let mut records = ["t1", "t2", "t3", "t4", "t5"].map(record).to_vec();
    records[1]["status"]["state"] = json!("blocked");
    records[3]["cwd"] = json!("/work/gsource");
    let mut remote = space("sp-remote", "other machine", "t5");
    remote["machine"] = json!("box");
    let (source, fake) = fake_with_doc(
        false,
        json!({"spaces": [first, unnamed,
        space("box/sp-other", "remote prefix", "t5"), remote]}),
        records,
    );
    let layout = source.poll().unwrap();
    assert_eq!(layout.working, 3);
    assert_eq!(
        layout
            .workspaces
            .iter()
            .map(|workspace| workspace.label.as_str())
            .collect::<Vec<_>>(),
        ["nexus/workflows", "gsource"]
    );
    let tabs = &layout.workspaces[0].terminals;
    assert_eq!(
        tabs.iter()
            .map(|tab| tab.label.as_str())
            .collect::<Vec<_>>(),
        ["build", "2"]
    );
    assert_eq!(
        tabs[0]
            .panes
            .iter()
            .map(|pane| pane.pane.clone())
            .collect::<Vec<_>>(),
        [id("t1"), id("t2")]
    );
    assert_eq!(
        tabs[0].agent.as_ref().unwrap().status,
        sheprd_client::protocol::AgentStatus::Blocked
    );
    assert_eq!(tabs[0].agent.as_ref().unwrap().changed_ms, None);
    assert_eq!(layout.workspaces[1].terminals[0].label, "1");
    assert!(layout.terminal(&id("t5")).is_none());
    assert!(layout.connected);
    drop(source);
    fake.server.join().unwrap();
}

fn saved_tabs(source: &mut Arc<Source>, fake: &Fake) {
    let path = fake._dir.path().join("browser-tabs.json");
    std::fs::write(
        &path,
        json!({"tabs": [
            {"id": 4, "scope": "test", "workspace_id": "saved", "title": "saved tab",
             "location": {"kind": "core_terminal", "terminal": "t1"}}
        ]})
        .to_string(),
    )
    .unwrap();
    let source = Arc::get_mut(source).unwrap();
    source.tabs = Some(path);
    source.scope = Some("test".into());
}

#[test]
fn absent_and_malformed_desktop_documents_keep_saved_tabs_and_fallback() {
    for doc in [
        Value::Null,
        json!({"spaces": "broken"}),
        json!({"spaces": [{"id": "bad"}]}),
        json!({"spaces": [space("sp", "name", "t1")], "padding": "x".repeat(1024 * 1024)}),
    ] {
        let (mut source, fake) = fake_with_doc(false, doc, vec![record("t1"), record("t2")]);
        saved_tabs(&mut source, &fake);
        let layout = source.poll().unwrap();
        assert_eq!(
            layout.workspace("saved").unwrap().terminals[0].label,
            "saved tab"
        );
        assert_eq!(
            layout.workspace("core:terminals").unwrap().terminals[0].panes[0].pane,
            id("t2")
        );
        assert!(layout.connected);
        drop(source);
        fake.server.join().unwrap();
    }
}

#[test]
fn an_existing_empty_document_keeps_saved_tabs() {
    let (mut source, fake) = fake_with_doc(false, json!({"spaces": []}), vec![record("t1")]);
    saved_tabs(&mut source, &fake);
    let layout = source.poll().unwrap();
    assert_eq!(
        layout.workspace("saved").unwrap().terminals[0].label,
        "saved tab"
    );
    assert!(layout.workspace("core:terminals").is_none());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn desktop_tab_dots_keep_the_desktop_attention_order() {
    for (states, expected) in [
        (
            ["working", "done"],
            sheprd_client::protocol::AgentStatus::Done,
        ),
        (
            ["done", "blocked"],
            sheprd_client::protocol::AgentStatus::Blocked,
        ),
        (
            ["idle", "working"],
            sheprd_client::protocol::AgentStatus::Working,
        ),
    ] {
        let mut space = space("sp", "project", "t1");
        space["tabs"][0]["root"] = json!({"type": "row", "children": [block("t1"), block("t2")]});
        let mut records = vec![record("t1"), record("t2")];
        for (record, status) in records.iter_mut().zip(states) {
            record["status"]["state"] = json!(status);
        }
        let (source, fake) = fake_with_doc(false, json!({"spaces": [space]}), records);
        let layout = source.poll().unwrap();
        assert_eq!(
            layout.workspaces[0].terminals[0]
                .agent
                .as_ref()
                .unwrap()
                .status,
            expected
        );
        drop(source);
        fake.server.join().unwrap();
    }
}
