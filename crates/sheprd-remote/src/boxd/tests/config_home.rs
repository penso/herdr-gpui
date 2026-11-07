//! The relay child reads its account from the configuration root this app
//! resolved, not from whatever its inherited environment says.
#![allow(clippy::unwrap_used)]

use super::super::connect::{pin_config_home, relay_command};
use std::{ffi::OsStr, process::Command};

fn config_home(command: &Command) -> Option<Option<&OsStr>> {
    command
        .get_envs()
        .find(|(key, _)| *key == "XDG_CONFIG_HOME")
        .map(|(_, value)| value)
}

#[test]
fn an_adopted_config_home_reaches_the_relay() {
    let mut command = Command::new("sheprd");
    command.env("XDG_CONFIG_HOME", "/shell/config");
    pin_config_home(&mut command, Some("/adopted".into()));
    assert_eq!(config_home(&command), Some(Some(OsStr::new("/adopted"))));
}

#[test]
fn without_a_resolved_config_home_an_inherited_one_is_removed() {
    let mut command = Command::new("sheprd");
    command.env("XDG_CONFIG_HOME", "/shell/config");
    pin_config_home(&mut command, None);
    assert_eq!(config_home(&command), Some(None));
}

#[test]
fn the_relay_command_always_names_its_config_home() {
    let command = relay_command(&super::settings("https://boxd.sh:9443"), "m-1").unwrap();
    let expected = sheprd_client::local_var("XDG_CONFIG_HOME");
    assert_eq!(config_home(&command), Some(expected.as_deref()));
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(
        args,
        [
            OsStr::new(super::super::relay::HELPER),
            OsStr::new("https://boxd.sh:9443"),
            OsStr::new("m-1"),
        ]
    );
}
