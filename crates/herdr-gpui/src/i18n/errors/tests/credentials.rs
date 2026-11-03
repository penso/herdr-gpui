use super::*;
use herdr_credentials::Error;
use std::io;

// `KeychainRead` and `KeychainWrite` (macOS) wrap a `security_framework`
// error, which this crate does not depend on, so no test builds them; their
// messages are checked by the compiler and `just check-i18n` only.

#[test]
fn every_credentials_variant_has_a_message() {
    let bytes = vec![0xff_u8];
    let utf8 = std::str::from_utf8(&bytes).unwrap_err();
    assert_localized(&[
        Error::Encoding(utf8),
        Error::Io(io::Error::other("disk on fire")),
        Error::Permissions,
        Error::Policy,
        Error::Unsupported,
    ]);
}

#[cfg(target_os = "linux")]
#[test]
fn every_secret_service_variant_has_a_message() {
    let error = || Box::new(oo7::Error::DBus(oo7::dbus::Error::Deleted));
    assert_localized(&[
        Error::SecretServiceRead(error()),
        Error::SecretServiceUnavailable,
        Error::SecretServiceWrite(error()),
    ]);
}

#[cfg(windows)]
#[test]
fn every_credential_manager_variant_has_a_message() {
    use herdr_credentials::CredentialManagerError;
    let errors = [
        CredentialManagerError::Read(io::Error::other("denied")),
        CredentialManagerError::Write(io::Error::other("denied")),
    ];
    assert_localized(&errors);
    assert_localized(&errors.map(Error::CredentialManager));
}
