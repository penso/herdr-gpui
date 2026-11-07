use super::*;
use crate::dispatch::Error;

#[test]
fn every_dispatch_variant_has_a_message() {
    assert_localized(&[
        Error::Busy,
        Error::HostUnavailable("build-box".into()),
        Error::InvalidHistory,
    ]);
}
