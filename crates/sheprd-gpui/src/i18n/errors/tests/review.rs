use super::*;
use crate::review::Error;

#[test]
fn every_review_variant_has_a_message() {
    assert_localized(&[Error::InvalidViewed, Error::NoBase, Error::TooLarge]);
}
