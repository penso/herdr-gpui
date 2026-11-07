use super::listening_ports::exit_status;
use super::*;
use crate::menu::devices::Error;

#[test]
fn every_devices_variant_has_a_message() {
    let detail = || "machine add: refused".to_owned();
    assert_localized(&[
        Error::Adding,
        Error::Exists("Build box".into()),
        Error::InvalidLabel,
        Error::InvalidSession,
        Error::InvalidTarget,
        Error::NonUtf8SetupPath,
        Error::NoProfile,
        Error::NoStateDirectory,
        Error::Remove {
            status: exit_status(),
            detail: String::new(),
        },
        Error::Remove {
            status: exit_status(),
            detail: detail(),
        },
        Error::Rename {
            status: exit_status(),
            detail: String::new(),
        },
        Error::Rename {
            status: exit_status(),
            detail: detail(),
        },
        Error::ServerRestart,
        Error::Setup {
            status: exit_status(),
            detail: String::new(),
        },
        Error::Setup {
            status: exit_status(),
            detail: detail(),
        },
        Error::SetupTimeout,
        Error::TailscaleJson(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::TailscaleTimeout,
        Error::TerminalSetupUnavailable,
    ]);
}
