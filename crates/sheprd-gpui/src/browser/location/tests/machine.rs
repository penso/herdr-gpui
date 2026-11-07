use super::*;

fn location(name: &str) -> Location {
    Location::Machine {
        name: crate::machine::Name::try_from(name.to_owned()).unwrap(),
    }
}

#[test]
fn a_machine_is_saved_by_its_name() {
    let saved = serde_json::to_string(&location("work")).unwrap();
    assert_eq!(saved, r#"{"kind":"machine","name":"work"}"#);
    assert_eq!(
        serde_json::from_str::<Location>(&saved).unwrap(),
        location("work")
    );
}

#[test]
fn a_saved_machine_cannot_name_a_path() {
    let saved = r#"{"kind":"machine","name":"../../secrets"}"#;
    assert!(serde_json::from_str::<Location>(saved).is_err());
}

#[test]
fn a_machine_gets_no_web_view() {
    assert!(!location("work").is_page());
}
