use super::*;
use std::ffi::OsStr;

#[test]
fn a_given_valid_id_is_the_endpoint() {
    let given = EndpointId::random();
    assert_eq!(endpoint_id(Some(OsStr::new(given.as_str()))), given);
}

#[test]
fn a_missing_or_invalid_id_gets_a_new_endpoint() {
    let fresh = endpoint_id(None);
    assert_ne!(endpoint_id(None), fresh);
    for invalid in ["", "nope", "ABCDEF0123456789ABCDEF0123456789", "../escape"] {
        let id = endpoint_id(Some(OsStr::new(invalid)));
        assert_ne!(id.as_str(), invalid);
        assert_eq!(id.as_str().len(), 32);
    }
}
