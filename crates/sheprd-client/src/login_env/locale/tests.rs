use super::*;
use std::ffi::OsStr;

/// An environment like the one launchd gives an app opened from the Dock,
/// Finder, or at login: a `PATH` and no locale.
fn bare(program: &str) -> Command {
    let mut command = Command::new(program);
    command.env_clear().env("PATH", "/usr/bin:/bin");
    command
}

fn lang(command: &Command) -> Option<Option<&OsStr>> {
    command
        .get_envs()
        .find(|(key, _)| *key == "LANG")
        .map(|(_, value)| value)
}

/// The encoding a child's C library picks, which is what `pbcopy` on macOS
/// reads its input as: with no locale the C locale's ASCII (`US-ASCII` on
/// macOS, `ANSI_X3.4-1968` with glibc), so `für` pastes as `f√ºr`.
#[test]
fn a_bare_launch_runs_children_in_utf8() {
    let mut command = bare("locale");
    command.arg("charmap");
    let charmap = |command: &mut Command| {
        let output = command.output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_ne!(charmap(&mut command), "UTF-8", "fixture is not bare");
    fill_in(&mut command, |_| None);
    assert_eq!(lang(&command), Some(Some(OsStr::new(FALLBACK))));
    assert_eq!(charmap(&mut command), "UTF-8");
}

/// Any locale someone chose, the command's own, the login shell's, or this
/// process's, wins over the fallback, even `C`.
#[test]
fn a_locale_that_is_set_is_kept() {
    for name in ENCODING_VARIABLES {
        let mut command = bare("true");
        command.env(name, "C");
        fill_in(&mut command, |_| None);
        assert_eq!(lang(&command).is_some(), name == "LANG", "{name}");
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value == Some(OsStr::new("C")))
        );

        let mut command = bare("true");
        fill_in(&mut command, |variable| {
            (variable == name).then(|| "C".into())
        });
        assert_eq!(lang(&command), None, "{name} inherited");
    }
}

/// An empty value selects no locale, and a variable removed from the command
/// hides the inherited one, so neither prevents the fallback.
#[test]
fn empty_and_removed_variables_count_as_unset() {
    let mut command = bare("true");
    command.env("LC_ALL", "").env("LC_CTYPE", "");
    fill_in(&mut command, |_| Some("".into()));
    assert_eq!(lang(&command), Some(Some(OsStr::new(FALLBACK))));

    // Not `bare`: after `env_clear` a removal is not recorded at all.
    let mut command = Command::new("true");
    command.env_remove("LANG");
    fill_in(&mut command, |variable| {
        (variable == "LANG").then(|| "de_DE.UTF-8".into())
    });
    assert_eq!(lang(&command), Some(Some(OsStr::new(FALLBACK))));
}
