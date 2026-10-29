use super::*;
use core::prelude::v1::test;

/// The preview names its statuses by messages, which `fl!` cannot check.
#[test]
fn preview_statuses_name_messages() {
    let none: &[&str] = &[];
    for (_, message) in PREVIEW {
        assert_eq!(i18n::takes(message), Some(none), "{message}");
    }
}
