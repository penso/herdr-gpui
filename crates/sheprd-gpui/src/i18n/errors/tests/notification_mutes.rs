use super::*;
use crate::notification_mutes::Error;

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Invalid,
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
    ]);
}
