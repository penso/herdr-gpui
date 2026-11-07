use super::*;

#[test]
fn dispatched_titles_keep_the_issue_identity_without_transport_or_control_text() {
    let mut issue = item(&github(), "357", "Windows GUI\n never connects\u{202e}");
    issue.identifier = "#357".into();
    assert_eq!(
        crate::naming::title(&issue),
        "#357 Windows GUI never connects"
    );
    issue.title = "界".repeat(200);
    assert!(crate::naming::title(&issue).chars().count() <= 80);
}
