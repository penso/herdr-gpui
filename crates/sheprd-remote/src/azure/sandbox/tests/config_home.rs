//! The sandbox relay runs under the configuration root this app resolved.

use super::super::{relay, setup::relay_command};
use crate::cloud::tests::relay::config_home;
use std::ffi::OsStr;

#[test]
fn the_relay_command_always_names_its_config_home() {
    let settings = super::settings();
    let command = relay_command(&settings, "4f1c").unwrap();
    let expected = sheprd_client::local_var("XDG_CONFIG_HOME");
    assert_eq!(config_home(&command), Some(expected.as_deref()));
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            OsStr::new(relay::HELPER),
            OsStr::new(&settings.sandbox_account()),
            OsStr::new("4f1c"),
        ]
    );
}
