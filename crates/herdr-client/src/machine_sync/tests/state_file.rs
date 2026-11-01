use super::*;

#[test]
fn a_state_round_trips_through_its_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("client").join(STATE_FILE);
    assert_eq!(load_state_path(&path).unwrap(), None);
    let mut saved = state(1, "me@laptop");
    saved.excluded.insert(id(9));
    store_state_path(&path, Some(&saved)).unwrap();
    assert_eq!(load_state_path(&path).unwrap(), Some(saved));
    store_state_path(&path, None).unwrap();
    assert_eq!(load_state_path(&path).unwrap(), None);
    // Turning off what is already off is not an error.
    store_state_path(&path, None).unwrap();
}

#[test]
fn a_state_other_catalogs_could_not_save_is_refused() {
    let invalid = [
        ("not-an-id", "me", "me@laptop"),
        (&id(1)[..], " ", "me@laptop"),
        (&id(1)[..], "me", "-oProxyCommand=x"),
        (&id(1)[..], "me", "me:secret@laptop"),
    ];
    for (id, label, target) in invalid {
        assert!(matches!(
            SyncState::new(id.to_owned(), label.to_owned(), target.to_owned()),
            Err(Error::StateValue)
        ));
    }
    let mut excluded = state(1, "me@laptop");
    excluded.excluded.insert("x".to_owned());
    assert!(matches!(encode_state(&excluded), Err(Error::StateValue)));
}

#[test]
fn unknown_versions_and_fields_are_refused() {
    let valid = encode_state(&state(1, "me@laptop")).unwrap();
    assert!(parse_state(valid.as_bytes()).is_ok());
    let newer = valid.replace("\"version\": 1", "\"version\": 2");
    assert!(matches!(
        parse_state(newer.as_bytes()),
        Err(Error::StateValue)
    ));
    let extra = valid.replace("\"version\": 1", "\"version\": 1, \"x\": 0");
    assert!(matches!(
        parse_state(extra.as_bytes()),
        Err(Error::StateSchema(_))
    ));
}
