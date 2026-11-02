use super::*;
use crate::mods::Error;

#[test]
fn every_mods_variant_has_a_message() {
    assert_localized(&[
        Error::Busy("clock".into()),
        Error::DisabledName("../clock".into()),
        Error::NotRunning("clock".into()),
        Error::Spawn {
            name: "clock".into(),
            source: std::io::Error::other("no threads left"),
        },
        Error::TooManyDisabled(64),
    ]);
}

#[test]
fn a_mods_own_diagnostics_are_shown_as_they_are() {
    assert_verbatim(&[Error::Mod(herdr_mods::Error::NoMain)]);
}
