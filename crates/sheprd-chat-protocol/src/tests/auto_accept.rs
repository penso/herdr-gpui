use super::*;

fn option(id: &str, kind: PermissionKind) -> PermissionOption {
    PermissionOption {
        id: id.into(),
        kind,
        name: id.into(),
    }
}

#[test]
fn auto_accepting_is_part_of_the_transcript() {
    let mut transcript = Transcript::new("OpenCode");
    assert!(!transcript.auto_accept);
    transcript.apply(&Change::AutoAccept { on: true }).unwrap();
    assert!(transcript.auto_accept);
    // An older transcript without the field reads as asking first.
    let old: Transcript = serde_json::from_value(serde_json::json!({"agent": "Claude"})).unwrap();
    assert!(!old.auto_accept);
    let command = serde_json::to_value(Command::SetAutoAccept { on: true }).unwrap();
    assert_eq!(
        command,
        serde_json::json!({"command": "set_auto_accept", "on": true})
    );
}

#[test]
fn the_allow_option_prefers_once_and_may_be_missing() {
    let mut permission = Permission {
        call_id: None,
        id: PermissionId(1),
        options: vec![
            option("always", PermissionKind::AllowAlways),
            option("once", PermissionKind::AllowOnce),
            option("no", PermissionKind::RejectOnce),
        ],
        title: "Edit".into(),
    };
    assert_eq!(
        permission.allow_option().map(|o| o.id.as_str()),
        Some("once")
    );
    permission.options.remove(1);
    assert_eq!(
        permission.allow_option().map(|o| o.id.as_str()),
        Some("always")
    );
    permission.options.remove(0);
    assert!(permission.allow_option().is_none());
}
