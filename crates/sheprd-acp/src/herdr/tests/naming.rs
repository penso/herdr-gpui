use crate::herdr::naming::{label, replaceable, title};

#[test]
fn a_title_is_the_prompt_on_one_line() {
    assert_eq!(
        title("fix the\n\n  tab\ttitle").as_deref(),
        Some("fix the tab title")
    );
    assert_eq!(title("  \n\t "), None);
}

#[test]
fn a_title_drops_control_characters() {
    assert_eq!(
        title("rename \u{1b}]0;evil\u{7} it").as_deref(),
        Some("rename ]0;evil it")
    );
}

#[test]
fn a_long_title_is_cut_at_a_word_with_an_ellipsis() {
    let prompt = "word ".repeat(40);
    let title = title(&prompt).unwrap();
    assert!(title.chars().count() <= 80);
    assert!(title.ends_with("word\u{2026}"));
}

#[test]
fn one_long_word_is_cut_where_it_must_be() {
    let title = title(&"x".repeat(200)).unwrap();
    assert_eq!(title.chars().count(), 80);
    assert!(title.ends_with('\u{2026}'));
}

#[test]
fn cutting_counts_characters_not_bytes() {
    let title = title(&"\u{e9}t\u{e9} ".repeat(40)).unwrap();
    assert!(title.chars().count() <= 80);
}

#[test]
fn a_label_names_the_agent_then_the_prompt() {
    assert_eq!(label("Claude", "fix the bug"), "Claude: fix the bug");
    let long = label("Claude", "make the tab show the latest prompt I sent");
    assert!(long.chars().count() <= 40);
    assert!(long.starts_with("Claude: make the tab"));
    assert!(long.ends_with('\u{2026}'));
}

#[test]
fn only_the_agent_name_a_given_label_or_a_number_is_replaced() {
    assert!(replaceable("Claude", "Claude", None));
    assert!(replaceable("Claude: old", "Claude", Some("Claude: old")));
    assert!(replaceable("3", "Claude", None));
    assert!(!replaceable("my tab", "Claude", Some("Claude: old")));
    assert!(!replaceable("", "Claude", None));
}
