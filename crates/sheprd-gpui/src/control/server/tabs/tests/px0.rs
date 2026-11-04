use super::*;

#[test]
fn px0_reports_an_editor_and_round_trips_its_open_request() {
    let scope = Scope::local(Path::new("/tmp/herdr.sock"));
    let mut store = Store::default();
    let id = store
        .open_editor_tab(scope, "w1", crate::browser::Editor::Px0, None)
        .unwrap();
    let info = info(store.get(id).unwrap());
    assert_eq!(info.kind, TabKind::Editor);
    assert_eq!(info.workspace, "w1");
    assert_eq!(info.title, "px0");
    let wire = serde_json::to_string(&TabOp::Px0).unwrap();
    assert_eq!(wire, r#"{"op":"px0"}"#);
    assert_eq!(serde_json::from_str::<TabOp>(&wire).unwrap(), TabOp::Px0);
    assert_eq!(
        TabOp::Px0.wait_seconds(),
        crate::control::protocol::tabs::ARRIVE_WAIT_SECONDS
    );
}
