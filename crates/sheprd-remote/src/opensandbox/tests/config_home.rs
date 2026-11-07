//! The OpenSandbox relay runs under the configuration root this app resolved.

use super::super::{connect::relay_command, relay};
use crate::cloud::tests::relay::config_home;
use std::ffi::OsStr;

#[test]
fn the_relay_command_always_names_its_config_home() {
    let command = relay_command(&super::settings("https://sandbox.example.com"), "sb-1").unwrap();
    let expected = sheprd_client::local_var("XDG_CONFIG_HOME");
    assert_eq!(config_home(&command), Some(expected.as_deref()));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            OsStr::new(relay::HELPER),
            OsStr::new("https://sandbox.example.com"),
            OsStr::new("sb-1"),
        ]
    );
}
