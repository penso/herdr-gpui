use super::*;
use crate::window::file_drop::Error;

#[test]
fn every_file_drop_variant_has_a_message() {
    assert_localized(&[
        Error::Control,
        Error::EmptyPath,
        Error::Encoding,
        Error::Size,
    ]);
}
