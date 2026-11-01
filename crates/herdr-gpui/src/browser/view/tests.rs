#[test]
fn long_titles_are_shortened_on_a_character_boundary() {
    assert_eq!(super::tab_label("Example Domain"), "Example Domain");
    let long = "Rust Programming Language — Official Site";
    assert_eq!(
        super::tab_label(long),
        "Rust Programming Language \u{2014}\u{2026}"
    );
    assert_eq!(super::tab_label(&"é".repeat(40)).chars().count(), 29);
}
