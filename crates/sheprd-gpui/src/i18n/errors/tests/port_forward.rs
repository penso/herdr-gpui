use super::*;
use crate::port_forward::Error;

#[test]
fn every_port_forward_variant_has_a_message() {
    assert_localized(&[Error::Duplicate(65535), Error::Limit(10000), Error::Port]);
}
