use super::agent_groups_text;

#[test]
fn agent_groups_mark_the_one_shown() {
    let groups = ["demo".to_owned(), "other".to_owned()];
    assert_eq!(agent_groups_text(None, &groups), "  demo\n  other");
    assert_eq!(agent_groups_text(Some("other"), &groups), "  demo\n* other");
    assert_eq!(
        agent_groups_text(None, &[]),
        "No coordinator sessions are listed"
    );
}
