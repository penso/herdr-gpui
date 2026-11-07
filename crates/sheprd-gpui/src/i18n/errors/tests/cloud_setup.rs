use super::*;
use crate::cloud_setup::Error;

#[test]
fn every_cloud_setup_variant_has_a_message() {
    assert_localized(&[
        Error::NoDevice("sprite-7".into()),
        Error::NoScript("Daytona"),
        Error::ScriptNul,
        Error::ScriptTooLong { limit: 32 * 1024 },
    ]);
}
