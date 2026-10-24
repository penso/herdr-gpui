use super::*;

/// Whatever went wrong opening the shell, beneath the scan's own error.
fn cause(host: &Host) -> Error {
    match super::super::open(host).err() {
        Some(Error::ListeningPorts(crate::listening_ports::Error::Scan(cause))) => *cause,
        other => panic!("{other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn a_name_that_is_not_a_distribution_starts_no_shell() {
    let error = cause(&Host::Wsl("-d".into()));
    assert!(
        matches!(
            error,
            Error::Usage(herdr_usage::Error::Client(herdr_client::Error::Wsl(
                herdr_client::wsl::Error::InvalidDistro
            )))
        ),
        "{error:?}"
    );
}

#[cfg(not(windows))]
#[test]
fn a_distribution_is_only_scanned_from_windows() {
    let error = cause(&Host::Wsl("Ubuntu".into()));
    assert!(
        matches!(error, Error::Usage(herdr_usage::Error::WslHostUnsupported)),
        "{error:?}"
    );
}

/// Scans a real distribution through `wsl.exe` and prints what it found:
/// `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-gpui
/// live_wsl_ports -- --ignored --nocapture`.
#[cfg(windows)]
#[test]
#[ignore = "opens a shell in a real WSL distribution"]
fn live_wsl_ports() {
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    let mut shell = super::super::open(&Host::Wsl(distro)).unwrap();
    let ports = super::super::scan(&mut shell).unwrap();
    for (owner, ports) in &ports {
        println!("{owner:?}: {ports:?}");
    }
}
