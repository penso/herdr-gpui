use super::*;
use std::os::unix::process::ExitStatusExt;

#[test]
fn ownership_selects_the_installed_format_not_available_tools() -> Result<()> {
    let executable = Path::new("/usr/bin/herdr-gpui");
    for (program, response, expected) in [
        (
            "/usr/bin/dpkg-query",
            "herdr-gpui:amd64: /usr/bin/herdr-gpui\n",
            Format::Deb,
        ),
        ("/usr/bin/rpm", "herdr-gpui\n", Format::Rpm),
        ("/usr/bin/pacman", "herdr-gpui\n", Format::Arch),
    ] {
        let package = locate(executable, |command| {
            if command.get_program() == program {
                Ok(response.into())
            } else {
                Ok("unrelated-package\n".into())
            }
        })?;
        assert_eq!(
            package,
            Package {
                format: expected,
                executable: executable.into()
            }
        );
    }
    assert!(matches!(
        locate(executable, |_| Ok("unrelated\n".into())),
        Err(Error::UnknownPackage)
    ));
    assert!(
        matches!(
            locate(executable, |_| Ok("herdr-gpui\n".into())),
            Err(Error::UnknownPackage)
        ),
        "ambiguous rpm/pacman ownership"
    );
    assert!(matches!(
        locate(executable, |_| Err(Error::Cancelled)),
        Err(Error::Cancelled)
    ));
    for output in [
        "other: /usr/bin/herdr-gpui",
        "herdr-gpui: /opt/herdr-gpui",
        "diversion by herdr-gpui: /usr/bin/herdr-gpui",
    ] {
        assert!(!owns(Format::Deb, executable, output));
    }
    Ok(())
}

#[test]
fn package_commands_pass_a_literal_absolute_path_and_fixed_manager() {
    let path = Path::new("/tmp/private dir/Herdr-20261010.1-x86_64-unknown-linux-gnu.deb");
    for (format, program, expected) in [
        (
            Format::Deb,
            "/usr/bin/apt-get",
            vec!["install", "--yes", "--no-remove", "--"],
        ),
        (Format::Rpm, "/usr/bin/dnf", vec!["install", "--assumeyes"]),
        (
            Format::Arch,
            "/usr/bin/pacman",
            vec!["-U", "--noconfirm", "--"],
        ),
    ] {
        let command = install_command(format, path);
        assert_eq!(command.get_program(), program);
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(&args[..args.len() - 1], expected);
        assert_eq!(args.last().copied(), Some(path.as_os_str()));
    }
}

#[test]
fn failed_and_denied_installation_never_report_success() {
    for code in [126, 127] {
        assert!(matches!(
            transaction_result(std::process::ExitStatus::from_raw(code << 8), String::new()),
            Err(Error::PackageAuthorization)
        ));
    }
    assert!(
        matches!(transaction_result(std::process::ExitStatus::from_raw(1 << 8), "dependency failure".into()), Err(Error::PackageFailed { detail, .. }) if detail == "dependency failure")
    );
}

#[test]
fn installer_output_is_drained_and_failures_are_reported() -> Result<()> {
    let mut command = Command::new("/bin/sh");
    // No real package manager or elevation is involved in process tests.
    command.args(["-c", "i=0; while [ $i -lt 2000 ]; do printf 'progress\\n' ; i=$((i+1)); done; printf 'dependency failure\\n' >&2; exit 1"]);
    let mut updates = 0;
    let result = transact(command, |detail| {
        updates += 1;
        assert!(detail.chars().count() <= 240);
    });
    assert!(matches!(result, Err(Error::PackageFailed { .. })));
    assert!(updates > 0);
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "exit 0"]);
    transact(command, |_| {})?;
    Ok(())
}
