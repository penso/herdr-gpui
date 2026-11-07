use super::*;
use crate::host_build::Error;
use std::{io, path::PathBuf};

#[test]
fn every_host_build_variant_has_a_message() {
    assert_localized(&[
        Error::Binary {
            program: "herdr-automate",
            path: PathBuf::from("/Applications/Herdr.app/herdr-automate"),
            source: io::Error::other("gone"),
        },
        Error::Checksum,
        Error::Platform("Plan9 mips".into()),
    ]);
}
