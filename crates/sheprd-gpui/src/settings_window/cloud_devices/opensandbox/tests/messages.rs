use super::super::FIELDS;

/// The fields name their labels' messages as data, which `fl!` cannot check.
#[test]
fn every_field_names_an_english_message_without_arguments() {
    let none: &[&str] = &[];
    for (label, _) in FIELDS {
        assert_eq!(crate::i18n::takes(label), Some(none), "{label}");
    }
}
