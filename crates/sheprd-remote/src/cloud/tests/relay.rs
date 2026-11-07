//! An exec relay reads its provider's table from the configuration root this
//! app resolved, not from whatever its inherited environment says.

use super::super::relay::{command, pin_config_home};
use std::{ffi::OsStr, process::Command};

/// The `XDG_CONFIG_HOME` that `command` sets (`Some(Some(_))`) or removes
/// (`Some(None)`), or `None` when it leaves the inherited one.
pub(crate) fn config_home(command: &Command) -> Option<Option<&OsStr>> {
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
fn the_relay_command_runs_this_executable_under_its_config_home() {
    let command = command("__test-exec").unwrap();
    assert_eq!(
        command.get_program(),
        std::env::current_exe().unwrap().as_os_str()
    );
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [OsStr::new("__test-exec")]
    );
    let expected = sheprd_client::local_var("XDG_CONFIG_HOME");
    assert_eq!(config_home(&command), Some(expected.as_deref()));
}
