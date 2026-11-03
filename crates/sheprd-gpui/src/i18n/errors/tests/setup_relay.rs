use super::*;
use crate::setup_relay::Error;

#[test]
fn every_setup_relay_variant_has_a_message() {
    assert_localized(&[
        Error::Closed,
        Error::Connect(std::io::Error::other("disk on fire")),
        Error::Dir("/tmp/herdr-setup".into()),
        Error::Frame,
        Error::Terminal,
    ]);
}
