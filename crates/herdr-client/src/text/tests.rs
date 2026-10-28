use super::*;

#[test]
fn transport_envelopes_do_not_become_titles() {
    assert_eq!(caption("<pasted_content id=\"9d74\">"), None);
    assert_eq!(caption("<pasted_content id=\"cut"), None);
    assert_eq!(
        caption("<pasted_content>\nFix the Windows connection\n</pasted_content>"),
        Some("Fix the Windows connection")
    );
    assert_eq!(caption("</pasted_content>"), None);
    assert_eq!(caption("Fix <div> rendering"), Some("Fix <div> rendering"));
    assert_eq!(caption("  My chosen tab  "), Some("  My chosen tab  "));
    assert_eq!(
        caption("<pasted_content_example>"),
        Some("<pasted_content_example>")
    );
    assert_eq!(caption("  "), None);
}
