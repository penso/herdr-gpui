use super::*;

#[test]
fn accented_text_is_about_forty_percent_longer() {
    let english = "Open the command palette and search for a workspace";
    let accented = pseudo::accented(english);
    let ratio = accented.chars().count() as f32 / english.chars().count() as f32;
    assert!((1.25..1.6).contains(&ratio), "{ratio}: {accented}");
    assert!(
        !accented.chars().any(|ch| ch.is_ascii_alphabetic()),
        "{accented}"
    );
}

#[test]
fn text_without_letters_is_left_alone() {
    assert_eq!(pseudo::accented("  — 42 "), "  — 42 ");
    assert_eq!(pseudo::bidi("  "), "  ");
}

#[test]
fn marks_wrap_whole_messages() {
    assert!(pseudo::is_marked("[Ȧ]"));
    assert!(!pseudo::is_marked("[Ȧ"));
    assert!(!pseudo::is_marked("Settings"));
}
