//! `herdr_client::ssh::Error` and the `SshFailure` it classifies a closed
//! bridge by, from `error-client-ssh.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::{SshFailure, ssh::Error};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Cancelled => fl!("error-client-ssh-cancelled"),
            Self::Closed => fl!("error-client-ssh-closed"),
            Self::InvalidTarget => fl!("error-client-ssh-invalid-target"),
            Self::KnownHostsUnavailable => fl!("error-client-ssh-known-hosts-unavailable"),
            Self::OutputLimit => fl!("error-client-ssh-output-limit"),
            Self::Refused(failure) => {
                fl!("error-client-ssh-refused", error = failure.localize())
            }
            Self::Timeout => fl!("error-client-ssh-timeout"),
            Self::Unsupported => fl!("error-client-ssh-unsupported"),
            // keep-sorted end
        }
    }
}

impl Localize for SshFailure {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Auth => fl!("error-client-ssh-failure-auth"),
            Self::HerdrMissing => fl!("error-client-ssh-failure-herdr-missing"),
            Self::HostKey => fl!("error-client-ssh-failure-host-key"),
            Self::LocalNetworkDenied => fl!("error-client-ssh-failure-local-network-denied"),
            Self::NoRoute => fl!("error-client-ssh-failure-no-route"),
            Self::Other => fl!("error-client-ssh-failure-other"),
            Self::Unreachable => fl!("error-client-ssh-failure-unreachable"),
            // keep-sorted end
        }
    }
}
