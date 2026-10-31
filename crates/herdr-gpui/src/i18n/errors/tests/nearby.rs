use super::*;
use crate::remote::nearby::Error;

#[test]
fn every_nearby_variant_has_a_message() {
    assert_localized(&[
        Error::Malformed(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Pipe("stdin"),
        Error::Spawn(std::io::Error::other("gone")),
        Error::TooLarge { limit: 4096 },
    ]);
}
