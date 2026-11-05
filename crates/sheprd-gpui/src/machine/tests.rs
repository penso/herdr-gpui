#![allow(clippy::expect_used)]
use super::*;

fn name(text: &str) -> Result<Name> {
    Name::try_from(text.to_owned())
}

#[test]
fn takes_plain_names() {
    for text in ["default", "work-2", "a_b", "9"] {
        assert_eq!(name(text).expect("valid").as_str(), text);
    }
}

#[test]
fn refuses_names_that_would_leave_the_folder_or_look_like_options() {
    let long = "a".repeat(MAX_NAME_BYTES + 1);
    for text in [
        "", ".", "..", "../x", "a/b", "a\\b", "-x", "_x", "a b", "é", &long,
    ] {
        assert!(matches!(name(text), Err(Error::InvalidName)), "{text:?}");
    }
}

#[test]
fn takes_the_longest_name() {
    assert!(name(&"a".repeat(MAX_NAME_BYTES)).is_ok());
}

#[test]
fn a_saved_name_is_checked_as_it_is_read() {
    let saved: Name = serde_json::from_str("\"work\"").expect("valid");
    assert_eq!(serde_json::to_string(&saved).expect("saves"), "\"work\"");
    assert!(serde_json::from_str::<Name>("\"../../etc\"").is_err());
}
