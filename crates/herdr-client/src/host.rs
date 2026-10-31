//! Which machine a host-bound operation runs on: this one, an SSH host, a
//! WSL distribution, or a cloud machine reached through its connection.
//! Plan usage, system load, listening ports, and Git checkpoints all ask a
//! host, so it lives here beside [`ConnectTarget`] rather than in any one
//! of them.

use crate::ConnectTarget;

/// A machine an operation runs on. A remote host is asked over SSH and a WSL
/// distribution through `wsl.exe`, so its own files and credentials are used
/// and never leave it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Host {
    Local,
    Ssh(String),
    /// A WSL distribution, asked through a `wsl.exe` shell as a remote host
    /// is over SSH: its files and sign-ins are its own, not this machine's.
    Wsl(String),
    /// An Azure machine, by its saved account and ID. Only its CPU and memory
    /// are sampled, through the command its connection uses; see `load_of`.
    #[cfg(feature = "azure")]
    Azure {
        account: String,
        id: String,
    },
}

impl Host {
    /// The host whose sign-ins a device's usage comes from, or `None` for a
    /// cloud machine: it is reached only through its provider's command, and reading
    /// this machine's sign-ins instead would misreport them as the workspace's.
    pub fn of(target: &ConnectTarget) -> Option<Self> {
        match target {
            ConnectTarget::Ssh { target, .. } => Some(Self::Ssh(target.clone())),
            ConnectTarget::Wsl { distro, .. } => Some(Self::Wsl(distro.clone())),
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud { .. } => None,
            ConnectTarget::Local | ConnectTarget::Session { .. } | ConnectTarget::Socket(_) => {
                Some(Self::Local)
            }
        }
    }

    /// The host whose CPU and memory a device shows: `of`'s, and also an
    /// Azure machine's, sampled through its own connection command.
    pub fn load_of(target: &ConnectTarget) -> Option<Self> {
        #[cfg(feature = "azure")]
        if let ConnectTarget::Cloud {
            provider: crate::CloudProvider::Azure,
            account,
            id,
            ..
        } = target
        {
            return Some(Self::Azure {
                account: account.clone(),
                id: id.clone(),
            });
        }
        Self::of(target)
    }
}
