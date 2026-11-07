use super::*;

#[test]
fn a_shell_command_refuses_names_that_are_not_distributions() {
    for name in ["", "-d", "two words", "a;b"] {
        assert!(
            matches!(
                wsl_shell_command(name),
                Err(crate::Error::Wsl(Error::InvalidDistro))
            ),
            "{name}"
        );
    }
}

#[cfg(windows)]
#[test]
fn a_shell_command_reads_its_script_from_stdin_in_the_distribution() {
    let command = wsl_shell_command("Ubuntu-24.04").unwrap();
    assert!(
        command
            .get_program()
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(r"system32\wsl.exe")
    );
    // `--exec` passes `-s` to `/bin/sh` untouched; the script never becomes
    // an argument `wsl.exe` could expand.
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(
        args,
        [
            "--distribution",
            "Ubuntu-24.04",
            "--cd",
            "~",
            "--exec",
            "/bin/sh",
            "-s"
        ]
    );
}

#[cfg(not(windows))]
#[test]
fn a_shell_command_needs_windows() {
    assert!(matches!(
        wsl_shell_command("Ubuntu"),
        Err(crate::Error::Wsl(Error::Unsupported))
    ));
}

#[test]
fn the_api_bridge_checks_each_herdr_and_quotes_the_session() {
    let script = api_bridge_script("work's");
    let check = script
        .find("--session 'work'\\''s' remote-api-bridge --check")
        .unwrap();
    let run = script
        .find("exec \"$path\" --session 'work'\\''s' remote-api-bridge\n")
        .unwrap();
    // A Herdr too old for the bridge is skipped, never run.
    assert!(check < run);
    assert!(script.contains("herdr-api-bridge-v1"));
}

#[test]
fn the_api_bridge_refuses_what_a_distribution_or_session_cannot_be() {
    assert!(matches!(
        wsl_api_bridge("a;b", "default"),
        Err(crate::Error::Wsl(Error::InvalidDistro))
    ));
    assert!(matches!(
        wsl_api_bridge("Ubuntu", "a b"),
        Err(crate::Error::InvalidSession)
    ));
}

#[cfg(not(windows))]
#[test]
fn the_api_bridge_needs_windows() {
    assert!(matches!(
        wsl_api_bridge("Ubuntu", "default"),
        Err(crate::Error::Wsl(Error::Unsupported))
    ));
    assert!(matches!(
        list_running_distros(),
        Err(crate::Error::Wsl(Error::Unsupported))
    ));
}
