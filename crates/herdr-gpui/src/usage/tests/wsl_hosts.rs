use super::*;
use crate::usage::Host;
use std::collections::HashSet;

fn read_wsl(distro: &str) -> crate::Result<Vec<(Provider, crate::Result<Report>)>> {
    let mut jar = CookieJar::default();
    let mut found = Vec::new();
    super::super::read(
        &Host::Wsl(distro.into()),
        &UsageConfig::default(),
        &HashSet::new(),
        &HashSet::new(),
        &mut jar,
        |provider, report| {
            found.push((provider, report));
            true
        },
    )?;
    Ok(found)
}

#[cfg(windows)]
#[test]
fn a_name_that_is_not_a_distribution_starts_no_shell() {
    let error = read_wsl("a;b").err().unwrap();
    assert!(
        matches!(
            error,
            Error::Client(herdr_client::Error::Wsl(
                herdr_client::wsl::Error::InvalidDistro
            ))
        ),
        "{error:?}"
    );
}

#[cfg(not(windows))]
#[test]
fn a_distribution_is_only_reached_from_windows() {
    let error = read_wsl("Ubuntu").err().unwrap();
    assert!(matches!(error, Error::WslHostUnsupported), "{error:?}");
}

/// Reads a distribution's real sign-ins through `wsl.exe` and prints what each
/// provider found: `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-gpui
/// live_wsl_usage -- --ignored --nocapture`. Prints typed results only, never
/// credentials.
#[cfg(windows)]
#[test]
#[ignore = "opens a shell in a real WSL distribution and calls agent services"]
fn live_wsl_usage() {
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    for (provider, report) in read_wsl(&distro).unwrap() {
        match report {
            Ok(report) => println!(
                "{}: plan {:?}, windows {}",
                provider.id(),
                report.account.plan,
                report.windows.len()
            ),
            Err(error) => println!("{}: error {error:?}", provider.id()),
        }
    }
}
