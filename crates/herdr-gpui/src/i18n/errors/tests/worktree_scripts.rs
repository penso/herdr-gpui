use super::*;
use crate::worktree_scripts::Error;
use std::sync::Arc;

#[test]
fn every_worktree_scripts_variant_has_a_message() {
    assert_localized(&[
        Error::Busy,
        Error::Checkout,
        Error::Encoding(String::from_utf8(vec![0xff]).unwrap_err().utf8_error()),
        Error::NotGit,
        Error::Nul,
        Error::Parse(toml::from_str::<toml::Table>("x").unwrap_err()),
        Error::Read(std::io::Error::other("disk on fire")),
        Error::Remote(herdr_client::Error::ResponseType),
        Error::Request(Arc::new(crate::Error::NotConnected)),
        Error::Response,
        Error::Size { limit: 65536 },
        Error::UnsupportedHost,
    ]);
}
