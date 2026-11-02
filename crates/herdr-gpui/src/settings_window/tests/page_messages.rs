//! Messages the behaviour pages name as data: each must be an English
//! message that takes no arguments.
use super::*;

#[test]
fn position_names_are_messages() {
    let none: &[&str] = &[];
    let ids = pages::CLIPBOARD_POSITIONS
        .iter()
        .map(|(_, id)| *id)
        .chain(pages::TOAST_POSITIONS.iter().map(|(_, id)| *id));
    for id in ids {
        assert_eq!(crate::i18n::takes(id), Some(none), "{id}");
    }
}

#[test]
fn override_label_follows_herdr_through_its_message() {
    assert_eq!(pages::override_label("On", Some("Off")), "Off");
    assert_eq!(
        pages::override_label("Top left", None),
        "Use Herdr setting · \u{2068}Top left\u{2069}"
    );
}
