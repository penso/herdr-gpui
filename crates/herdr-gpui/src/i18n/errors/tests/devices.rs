use super::listening_ports::exit_status;
use super::*;
use crate::menu::devices::Error;

#[test]
fn every_devices_variant_has_a_message() {
    let detail = || "machine add: refused".to_owned();
    assert_localized(&[
        Error::Adding,
        Error::Exists("Build box".into()),
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
    ]);
}

/// Each sentence `menu::devices::setup` passes to `SetupInput`.
#[test]
fn every_setup_input_sentence_has_a_message() {
    let sentences = [
        "This device has no saved profile.",
        "Device names are at most 128 bytes, without control characters.",
        "Enter an SSH target such as user@hostname or an SSH alias.",
        "Session names use letters, digits, dots, underscores, or hyphens (at most 64 bytes), excluding '.' and '..'.",
        "Herdr UI has no state directory.",
        "The setup paths must be UTF-8.",
        "Setting up a device in a terminal is unavailable on Windows.",
    ];
    assert_localized(&sentences.map(Error::SetupInput));
    assert_verbatim(&[Error::SetupInput("A sentence with no message yet.")]);
}
