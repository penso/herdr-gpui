//! `herdr_credentials::Error`: reading or writing a saved sign-in, and on
//! Windows the Credential Manager failure it wraps, from
//! `error-credentials.ftl`. No message carries the secret.

use super::Localize;
use crate::i18n::fl;
use herdr_credentials::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            #[cfg(windows)]
            Self::CredentialManager(error) => error.localize(),
            Self::Encoding(_) => fl!("error-credentials-encoding"),
            Self::Io(_) => fl!("error-credentials-io"),
            #[cfg(target_os = "macos")]
            Self::KeychainRead(_) => fl!("error-credentials-keychain-read"),
            #[cfg(target_os = "macos")]
            Self::KeychainWrite(_) => fl!("error-credentials-keychain-write"),
            Self::Permissions => fl!("error-credentials-permissions"),
            Self::Policy => fl!("error-credentials-policy"),
            #[cfg(target_os = "linux")]
            Self::SecretServiceRead(_) => fl!("error-credentials-secret-service-read"),
            #[cfg(target_os = "linux")]
            Self::SecretServiceUnavailable => fl!("error-credentials-secret-service-unavailable"),
            #[cfg(target_os = "linux")]
            Self::SecretServiceWrite(_) => fl!("error-credentials-secret-service-write"),
            Self::Unsupported => fl!("error-credentials-unsupported"),
            // keep-sorted end
        }
    }
}

#[cfg(windows)]
impl Localize for herdr_credentials::CredentialManagerError {
    fn localize(&self) -> String {
        match self {
            Self::Read(_) => fl!("error-credentials-credential-manager-read"),
            Self::Write(_) => fl!("error-credentials-credential-manager-write"),
        }
    }
}
