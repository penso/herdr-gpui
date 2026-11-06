use super::super::FIELDS;

/// The fields name their messages as data, which `fl!` cannot check.
#[test]
fn every_field_names_english_messages_without_arguments() {
    let none: &[&str] = &[];
    for (label, placeholder) in FIELDS {
        assert_eq!(crate::i18n::takes(label), Some(none), "{label}");
        assert_eq!(crate::i18n::takes(placeholder), Some(none), "{placeholder}");
    }
}
