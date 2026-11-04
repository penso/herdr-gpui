use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Why a saved credential could not be read or written. The display text is
/// shown to the user, so none of it carries the secret.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[cfg(windows)]
    #[error(transparent)]
    CredentialManager(#[from] crate::credential_manager::Error),
    #[error("Invalid GitHub credential encoding.")]
    Encoding(#[source] std::str::Utf8Error),
    #[error("Cannot access private credential file.")]
    Io(#[source] io::Error),
    #[cfg(target_os = "macos")]
    #[error("Cannot read GitHub Keychain entry. Unlock your login Keychain or set GH_TOKEN.")]
    KeychainRead(#[source] security_framework::base::Error),
    #[cfg(target_os = "macos")]
    #[error("GitHub Keychain update failed. Unlock your login Keychain and try again.")]
    KeychainWrite(#[source] security_framework::base::Error),
    #[error(
        "Cannot access private credential file. Require an owned directory and regular 0600 file; symlinks are rejected."
    )]
    Permissions,
    #[error(
        "No secure credential store configured. Explicitly opt in with allow_plaintext_credentials = true in the [github] or [coder] table, or use GH_TOKEN / GITHUB_TOKEN for GitHub."
    )]
    Policy,
    #[cfg(target_os = "linux")]
    #[error(
        "Cannot read GitHub sign-in from the desktop keyring. Unlock your keyring or set GH_TOKEN."
    )]
    SecretServiceRead(#[source] Box<oo7::Error>),
    #[cfg(target_os = "linux")]
    #[error(
        "No desktop keyring (Secret Service) is running. Start GNOME Keyring, KWallet, or KeePassXC, or set [github] allow_plaintext_credentials = true, or use GH_TOKEN / GITHUB_TOKEN."
    )]
    SecretServiceUnavailable,
    #[cfg(target_os = "linux")]
    #[error("GitHub keyring update failed. Unlock your desktop keyring and try again.")]
    SecretServiceWrite(#[source] Box<oo7::Error>),
    #[error(
        "No GitHub credential store is available on this platform. Use GH_TOKEN / GITHUB_TOKEN."
    )]
    Unsupported,
    // keep-sorted end
}
