//! The Keychain grants the app keeps across launches.
use super::*;
use crate::usage::access;

#[test]
fn saved_grants_keep_only_known_providers() {
    let granted = access::parse_saved(r#"{"providers":["zed","no-such-provider"]}"#).unwrap();
    assert_eq!(granted.len(), 1);
    assert!(granted.contains(&provider("zed")));
}
