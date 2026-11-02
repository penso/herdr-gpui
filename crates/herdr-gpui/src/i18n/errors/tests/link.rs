use super::*;
use crate::link::follow::Error;
use std::path::PathBuf;

#[test]
fn every_link_variant_has_a_message() {
    let path = || PathBuf::from("/tmp/checkout");
    assert_localized(&[
        Error::Unreadable {
            path: path(),
            source: std::io::Error::other("gone"),
        },
        Error::NotFolder(path()),
        Error::NotCheckout(path()),
        #[cfg(windows)]
        Error::Remote(path()),
    ]);
}
