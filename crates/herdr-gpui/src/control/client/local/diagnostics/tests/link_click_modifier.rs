use super::*;

/// `link_click_modifier` names keys, never a secret, in either form, so a
/// bundle shows the combinations a user set rather than only their count.
#[test]
fn link_click_modifiers_are_kept_as_text_and_as_a_list() {
    assert_eq!(
        summary("link_click_modifier = \"ctrl+shift\"\n"),
        "link_click_modifier = \"ctrl+shift\"\n"
    );
    assert_eq!(
        summary("link_click_modifier = [\"none\", \"cmd\"]\n"),
        "link_click_modifier = [\"none\", \"cmd\"]\n"
    );
    // A list that is not all text is only counted, as any other list is.
    assert_eq!(
        summary("link_click_modifier = [\"none\", 1]\n"),
        "link_click_modifier = <list of 2>\n"
    );
}
