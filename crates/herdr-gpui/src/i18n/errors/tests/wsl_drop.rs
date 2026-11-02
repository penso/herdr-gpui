use super::*;
use crate::window::wsl_drop::Error;

#[test]
fn every_wsl_drop_variant_has_a_message() {
    assert_localized(&[Error::Busy, Error::TargetGone]);
}
