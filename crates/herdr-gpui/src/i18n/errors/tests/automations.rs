use super::*;
use crate::automations::Error;
use std::{io, path::PathBuf};

#[test]
fn every_automations_variant_has_a_message() {
    let host = || "build-box".to_owned();
    let path = || "~/.config/herdr/nightly.luau".to_owned();
    assert_localized(&[
        Error::Approved {
            path: path(),
            host: host(),
        },
        Error::Binary {
            path: PathBuf::from("/Applications/Herdr.app/herdr-automate"),
            source: io::Error::other("gone"),
        },
        Error::Checksum,
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Missing,
        Error::NoBuild {
            platform: "linux-riscv64".into(),
        },
        Error::NoScript {
            path: path(),
            host: host(),
        },
        Error::NotRead { host: host() },
        Error::Off,
        Error::Platform("Plan9 mips".into()),
        Error::Protocol {
            found: 3,
            expected: 2,
        },
        Error::Refused("script is not approved".into()),
        Error::Run(herdr_client::Error::Full),
        Error::UnknownHost(host()),
        Error::UnknownVersion,
        Error::UnsupportedHost,
    ]);
}
