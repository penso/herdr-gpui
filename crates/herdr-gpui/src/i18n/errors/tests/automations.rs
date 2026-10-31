use super::*;
use crate::automations::Error;

#[test]
fn every_automations_variant_has_a_message() {
    let host = || "build-box".to_owned();
    let path = || "~/.config/herdr/nightly.luau".to_owned();
    assert_localized(&[
        Error::Approved {
            path: path(),
            host: host(),
        },
        Error::Build(crate::host_build::Error::Checksum),
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
