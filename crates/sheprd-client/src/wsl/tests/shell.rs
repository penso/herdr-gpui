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
