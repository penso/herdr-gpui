//! What the daemon advertises over Bonjour, and turning it off.

use super::*;
use crate::server::bonjour::{SERVICE, host_name, txt};

const FP: &str = "9E:92:A0:DF:1C:E0:96:7E:C3:4C:0C:01:46:00:5F:BC:51:65:43:30:E7:D2:6E:C9:B5:80:50:55:75:5E:24:F6";

#[test]
fn the_record_names_the_version_a_fingerprint_prefix_and_the_pairing_dialog() {
    assert_eq!(SERVICE, "_herdr-remote._tcp.local.");
    assert_eq!(
        txt(FP, true),
        vec![
            ("v", "1".to_owned()),
            ("fp", "9E92A0DF1CE0967E".to_owned()),
            ("pairing", "1".to_owned()),
        ]
    );
    assert_eq!(txt(FP, false)[2], ("pairing", "0".to_owned()));
}

#[test]
fn host_names_keep_to_letters_digits_and_hyphens() {
    assert_eq!(
        host_name("Fabien's MacBook Pro"),
        "fabien-s-macbook-pro.local."
    );
    assert_eq!(host_name("studio"), "studio.local.");
    assert_eq!(host_name("∆"), "herdr.local.");
}

#[test]
fn bonjour_is_on_unless_turned_off() {
    assert!(serve(&["serve", "--state-dir", "/s"]).bonjour);
    assert!(!serve(&["serve", "--state-dir", "/s", "--no-bonjour"]).bonjour);
    assert!(USAGE.contains("--no-bonjour"));
}
