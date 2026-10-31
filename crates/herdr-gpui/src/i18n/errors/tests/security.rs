use super::*;
use crate::security::Error;

#[test]
fn every_security_variant_has_a_message() {
    assert_localized(&[
        Error::Github(Box::new(crate::Error::NotConnected)),
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Output,
        Error::Unread,
    ]);
}
