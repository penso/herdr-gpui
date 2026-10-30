//! Where a device runs and how it is reached, for the mark before its name in
//! the sidebar. Read from the connection target alone: upstream's host catalog
//! keeps no record of how a host was added, so an SSH host is recognized by
//! the names Coder's and Tailscale's own setups give it.

use crate::i18n::fl;
use herdr_client::ConnectTarget;
use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// This machine's own daemon.
    Local,
    Ssh,
    /// An SSH host on the user's tailnet.
    Tailscale,
    /// A WSL distribution on this Windows machine.
    Wsl,
    /// A machine a cloud provider created. `ssh` is a host the provider's CLI
    /// wrote into `~/.ssh/config` rather than one saved as a cloud device.
    #[cfg(feature = "cloud")]
    Cloud {
        provider: crate::cloud::CloudProvider,
        ssh: bool,
    },
}

impl Origin {
    pub(crate) fn of(target: &ConnectTarget) -> Self {
        match target {
            ConnectTarget::Local | ConnectTarget::Session { .. } | ConnectTarget::Socket(_) => {
                Self::Local
            }
            ConnectTarget::Wsl { .. } => Self::Wsl,
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud { provider, .. } => Self::Cloud {
                provider: *provider,
                ssh: false,
            },
            ConnectTarget::Ssh { target, .. } => Self::of_ssh(&host_name(target)),
        }
    }

    fn of_ssh(host: &str) -> Self {
        // `coder config-ssh` names each workspace `<agent>.<workspace>.<owner>.coder`.
        #[cfg(feature = "coder")]
        if host.ends_with(".coder") {
            return Self::Cloud {
                provider: crate::cloud::CloudProvider::Coder,
                ssh: true,
            };
        }
        if host.ends_with(".ts.net") || host.parse().is_ok_and(tailnet) {
            return Self::Tailscale;
        }
        Self::Ssh
    }

    /// The embedded mark, drawn in the row's text color.
    pub(crate) fn icon(self) -> &'static str {
        match self {
            Self::Local => "icons/computer.svg",
            // Add Device marks SSH hosts and WSL distributions this way too.
            Self::Ssh | Self::Wsl => "icons/terminal.svg",
            Self::Tailscale => "icons/network/tailscale.svg",
            #[cfg(feature = "cloud")]
            Self::Cloud { provider, .. } => crate::cloud::icon(provider),
        }
    }

    /// What the mark stands for, as its tooltip says it. A cloud provider's
    /// noun is a key the message selects its translated word by.
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Local => fl!("sidebar-origin-local"),
            Self::Ssh => fl!("sidebar-origin-ssh"),
            Self::Tailscale => fl!("sidebar-origin-tailscale"),
            Self::Wsl => fl!("sidebar-origin-wsl"),
            #[cfg(feature = "cloud")]
            Self::Cloud {
                provider,
                ssh: false,
            } => fl!(
                "sidebar-origin-cloud",
                provider = crate::cloud::name(provider),
                noun = crate::cloud::noun(provider)
            ),
            #[cfg(feature = "cloud")]
            Self::Cloud {
                provider,
                ssh: true,
            } => fl!(
                "sidebar-origin-cloud-ssh",
                provider = crate::cloud::name(provider),
                noun = crate::cloud::noun(provider)
            ),
        }
    }
}

/// Tailscale's address ranges: 100.64.0.0/10 and fd7a:115c:a1e0::/48.
fn tailnet(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            first == 100 && (64..128).contains(&second)
        }
        IpAddr::V6(ip) => ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
    }
}

/// The host an SSH target names, lowercased, without its user, port, URL
/// scheme, IPv6 brackets, or trailing root dot.
pub(crate) fn host_name(target: &str) -> String {
    let target = target.trim();
    let target = target.strip_prefix("ssh://").unwrap_or(target);
    let host = target.rsplit_once('@').map_or(target, |(_, host)| host);
    let host = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or(bracketed),
        None if host.matches(':').count() == 1 => host.split(':').next().unwrap_or(host),
        None => host,
    };
    host.trim().trim_end_matches('.').to_lowercase()
}

#[cfg(test)]
mod tests;
