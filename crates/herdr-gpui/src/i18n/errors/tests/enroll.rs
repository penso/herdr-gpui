use super::*;
use herdr_remote_control::{app, app::enroll::Error, control};
use std::path::PathBuf;

#[test]
fn every_enroll_variant_has_a_message() {
    assert_localized(&[
        Error::ApnsSize,
        Error::Config {
            path: PathBuf::from("/home/ada/.config/herdr-remote/config.toml"),
            source: "expected `=`".into(),
        },
        Error::NoHome,
        Error::NotStarted(control::Error::NoAnswer),
        Error::Outdated,
        Error::Unexpected,
    ]);
    assert_localized(&[app::Error::Enroll(Error::Outdated)]);
}
