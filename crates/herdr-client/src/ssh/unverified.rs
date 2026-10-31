//! SSH hosts the user chose to connect to without verifying their host keys.
//!
//! The bridge's policy is `StrictHostKeyChecking=yes`, and options given on the
//! command line beat the user's config. A host that keeps no keys, as
//! `coder config-ssh` writes for `*.coder` (`UserKnownHostsFile=/dev/null`),
//! can then never connect: no key can be trusted for it. The app lets the user
//! opt such a host out explicitly, and pushes that list here; every `ssh` this
//! crate starts for a listed target is told not to check its key. Reading the
//! list never blocks on I/O, so command builders stay safe on the UI thread.
use std::{
    collections::BTreeSet,
    sync::{PoisonError, RwLock},
};

/// At most this many targets are kept, as the app's config also bounds them.
pub const MAX_UNVERIFIED_HOSTS: usize = 256;

static HOSTS: RwLock<BTreeSet<String>> = RwLock::new(BTreeSet::new());

/// Whether `ssh` checks a host's key against the known hosts files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostKeys {
    #[default]
    Verify,
    /// Accept whatever key the host offers. Anyone able to intercept the
    /// connection can impersonate the host.
    Skip,
}

impl HostKeys {
    /// What the user chose for `target`.
    pub fn of(target: &str) -> Self {
        let hosts = HOSTS.read().unwrap_or_else(PoisonError::into_inner);
        if hosts.contains(target) {
            Self::Skip
        } else {
            Self::Verify
        }
    }

    /// Options placed before [`super::POLICY`], which they override. No known
    /// hosts file is read either: with checking off, a recorded key that no
    /// longer matches still connects but turns off agent and port forwarding.
    pub(crate) fn options(self) -> &'static [&'static str] {
        match self {
            Self::Verify => &[],
            Self::Skip => &[
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
            ],
        }
    }
}

/// Replace the targets whose host keys are not verified. Targets beyond
/// [`MAX_UNVERIFIED_HOSTS`] are dropped; the app's config refuses them first.
pub fn set_unverified_hosts<I>(targets: I)
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let targets = bounded(targets);
    let previous = std::mem::replace(
        &mut *HOSTS.write().unwrap_or_else(PoisonError::into_inner),
        targets.clone(),
    );
    // Retired after the list's lock is released; the registry takes its own.
    for target in previous.symmetric_difference(&targets) {
        super::mux::host_keys_changed(target);
    }
}

fn bounded<I>(targets: I) -> BTreeSet<String>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    targets
        .into_iter()
        .map(Into::into)
        .take(MAX_UNVERIFIED_HOSTS)
        .collect()
}

/// Record the user's choice for one target, effective for the next `ssh`.
pub fn set_host_keys(target: &str, keys: HostKeys) {
    let changed = {
        let mut hosts = HOSTS.write().unwrap_or_else(PoisonError::into_inner);
        match keys {
            HostKeys::Skip if hosts.len() < MAX_UNVERIFIED_HOSTS => hosts.insert(target.to_owned()),
            HostKeys::Skip => false,
            HostKeys::Verify => hosts.remove(target),
        }
    };
    if changed {
        super::mux::host_keys_changed(target);
    }
}

#[cfg(test)]
mod tests;
