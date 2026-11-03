//! `herdr_usage`'s errors from `error-usage.ftl`. A provider's command line
//! stays a placeable; the panel's text carries no account details, as the
//! crate's `Display` carries none.

use super::Localize;
use crate::i18n::fl;
use herdr_usage::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Client(error) => error.localize(),
            #[cfg(feature = "azure")]
            Self::CloudHostUnsupported => fl!("error-usage-cloud-host-unsupported"),
            Self::Command(command) => fl!(
                "error-usage-command",
                kind = command_step(command),
                command = *command
            ),
            Self::Connect => fl!("error-usage-connect"),
            Self::Header(_) => fl!("error-usage-header"),
            Self::Json(_) => fl!("error-usage-json"),
            Self::KeychainAccess => fl!("error-usage-keychain-access"),
            Self::KeychainDenied => fl!("error-usage-keychain-denied"),
            Self::MissingCurl => fl!("error-usage-missing-curl"),
            Self::MixedSecrets => fl!("error-usage-mixed-secrets"),
            Self::Network(_) => fl!("error-usage-network"),
            Self::NoPlan => fl!("error-usage-no-plan"),
            Self::NotSignedIn => fl!("error-usage-not-signed-in"),
            Self::Process { operation, .. } => fl!(
                "error-usage-process",
                kind = process_step(operation),
                operation = *operation
            ),
            Self::RateLimited => fl!("error-usage-rate-limited"),
            Self::Rejected => fl!("error-usage-rejected"),
            Self::Size => fl!("error-usage-size"),
            Self::Status(status) => fl!("error-usage-status", status = *status),
            Self::Timeout => fl!("error-usage-timeout"),
            Self::Unreachable => fl!("error-usage-unreachable"),
            Self::WslHostUnsupported => fl!("error-usage-wsl-host-unsupported"),
            // keep-sorted end
        }
    }
}

/// The message key for a failed usage step the providers word in English;
/// a command line, or a step not listed, is shown as it is.
fn command_step(command: &str) -> &'static str {
    match command {
        "agy 1.1.11 or later is needed for usage reports" => "agy-version", // i18n: data
        "agy could not report usage; run agy to sign in" => "agy-sign-in",  // i18n: data
        "find the Gemini CLI's OAuth client to refresh the sign-in" => "gemini-client", // i18n: data
        "read CodeRabbit usage" => "coderabbit", // i18n: data
        "read the Windsurf plan cache" => "windsurf", // i18n: data
        _ => "other",
    }
}

/// The message key for a process the usage probes could not run; one not
/// listed is shown as it is.
fn process_step(operation: &str) -> &'static str {
    match operation {
        "read a keychain item" => "keychain",             // i18n: data
        "read the browser's cookie key" => "cookie-key",  // i18n: data
        "run a provider command" => "provider-command",   // i18n: data
        "start the remote usage shell" => "remote-shell", // i18n: data
        _ => "other",
    }
}
