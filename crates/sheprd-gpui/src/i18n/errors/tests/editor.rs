use super::*;
use crate::editor::Error;
use std::sync::Arc;

#[test]
fn every_editor_variant_has_a_message() {
    assert_localized(&[
        Error::Busy,
        Error::Command,
        Error::NoPane,
        Error::Path,
        Error::Remote,
        Error::RemoteBusy,
        Error::RemoteFailed,
        Error::RemoteLaunch(std::io::Error::other("nvim: not found")),
        Error::Request(Arc::new(crate::Error::NotConnected)),
        Error::Response,
        Error::Unsupported,
    ]);
}
