#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write;

fn asset(bytes: &[u8]) -> release::Asset {
    release::Asset {
        target: "x86_64-pc-windows-msvc".into(),
        name: "Herdr-20261017.1-x86_64-pc-windows-msvc-setup.exe".into(),
        size: bytes.len() as u64,
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

/// A staged "installer" with these bytes, held as `prepare` holds a download.
fn staged(bytes: &[u8], listed: &[u8]) -> Prepared {
    let stage = tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir()
        .unwrap();
    let installer = stage.path().join("setup.exe");
    File::create(&installer).unwrap().write_all(bytes).unwrap();
    let file = hold(&installer).unwrap();
    Prepared {
        file,
        stage,
        installer,
        asset: asset(listed),
    }
}

#[test]
fn installer_runs_silently_closes_the_app_and_starts_it_again() {
    let args = installer_args(Path::new(r"C:\Users\a b\Temp\herdr-update-1\install.log"));
    assert_eq!(
        args,
        [
            "/VERYSILENT",
            "/SUPPRESSMSGBOXES",
            "/NORESTART",
            "/CLOSEAPPLICATIONS",
            "/RELAUNCH=1",
            r"/LOG=C:\Users\a b\Temp\herdr-update-1\install.log",
        ]
    );
}

#[test]
fn held_installer_cannot_be_replaced_or_deleted() {
    let prepared = staged(b"installer", b"installer");
    let error = OpenOptions::new()
        .write(true)
        .open(&prepared.installer)
        .unwrap_err();
    // ERROR_SHARING_VIOLATION: the held handle shares reading only.
    assert_eq!(error.raw_os_error(), Some(32));
    assert!(fs::remove_file(&prepared.installer).is_err());
    // Reading, as Windows does to run it, still works.
    assert_eq!(fs::read(&prepared.installer).unwrap(), b"installer");
}

#[test]
fn installer_that_differs_from_the_signed_manifest_is_refused_before_its_signature() {
    let prepared = staged(b"tampered", b"original");
    let error = prepared
        .verify("CN=Herdr UI", &AtomicBool::new(false))
        .unwrap_err();
    assert!(matches!(error, crate::Error::ArchiveDigest), "{error:?}");
}

#[test]
fn unsigned_installer_is_refused_even_when_its_digest_matches() {
    // This test executable is a local build, never Authenticode-signed.
    let bytes = fs::read(env::current_exe().unwrap()).unwrap();
    let prepared = staged(&bytes, &bytes);
    let error = prepared
        .verify("CN=Herdr UI", &AtomicBool::new(false))
        .unwrap_err();
    assert!(
        matches!(error, crate::Error::Windows(Error::Untrusted(_))),
        "{error:?}"
    );
}

#[test]
fn cancelled_install_never_returns_a_guard() {
    let prepared = staged(b"installer", b"installer");
    let stage = prepared.stage.path().to_owned();
    assert!(matches!(
        install_and_restart(prepared, &AtomicBool::new(true)),
        Err(crate::Error::Cancelled)
    ));
    // Dropping the uncommitted download deletes it.
    assert!(!stage.exists());
}

#[test]
fn local_builds_name_no_publisher_and_disable_updates() {
    // Release builds get HERDR_WINDOWS_SIGNER from the release workflow.
    if publisher().is_none() {
        assert!(matches!(unsupported(), Some(Error::MissingPublisher)));
    }
}

/// Opt-in: `HERDR_TEST_SIGNED_FILE` names an Authenticode-signed executable
/// (CI uses the Microsoft-signed `OpenConsole.exe` in the pinned Herdr
/// archive). Windows must trust it, and the signer this module reads must be
/// exactly the subject `Get-AuthenticodeSignature` prints, which is what the
/// release workflow's `WINDOWS_SIGNING_SUBJECT` is compared with.
#[test]
#[ignore = "requires HERDR_TEST_SIGNED_FILE, an Authenticode-signed executable"]
fn signer_matches_what_powershell_reports() {
    let path =
        PathBuf::from(env::var_os("HERDR_TEST_SIGNED_FILE").expect("set HERDR_TEST_SIGNED_FILE"));
    let output = herdr_client::new_command("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-AuthenticodeSignature -LiteralPath $env:HERDR_TEST_SIGNED_FILE).SignerCertificate.Subject",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let expected = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert!(!expected.is_empty(), "PowerShell reports no signer");
    let file = hold(&path).unwrap();
    authenticode::verify(&file, &path, &expected).unwrap();
    let error = authenticode::verify(&file, &path, "CN=Someone Else").unwrap_err();
    assert!(
        matches!(&error, Error::Signer { found } if *found == expected),
        "{error:?}"
    );
    eprintln!("verified Authenticode signer: {expected}");
}
