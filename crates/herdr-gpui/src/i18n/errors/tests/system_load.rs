use super::*;
use crate::system_load::Error;

#[test]
fn every_system_load_variant_has_a_message() {
    assert_localized(&[
        Error::Output,
        Error::Remote(Box::new(crate::Error::NotConnected)),
        Error::Unsupported("Plan 9".into()),
    ]);
}
