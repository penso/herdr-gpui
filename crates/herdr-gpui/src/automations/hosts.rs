//! Which machines the section reads: this one always, and every other
//! connected device, each once however many sessions reach it.

use crate::{HerdrWindow, i18n::fl, teleport::Host};
use herdr_client::ConnectTarget;

/// One machine `herdr-automate` runs on. Two sessions of the same host share
/// one daemon, so they are one machine here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Machine {
    Local,
    Ssh(String),
    #[cfg(feature = "cloud")]
    Cloud {
        provider: herdr_client::CloudProvider,
        account: String,
        id: String,
    },
}

impl Machine {
    /// The machine `target` reaches, if a host script can run there.
    pub(crate) fn of(target: &ConnectTarget) -> Option<Self> {
        match target {
            ConnectTarget::Local | ConnectTarget::Session { .. } => Some(Self::Local),
            ConnectTarget::Ssh { target, .. } => Some(Self::Ssh(target.clone())),
            #[cfg(feature = "cloud")]
            ConnectTarget::Cloud {
                provider,
                account,
                id,
                ..
            } => Some(Self::Cloud {
                provider: *provider,
                account: account.clone(),
                id: id.clone(),
            }),
            ConnectTarget::Socket(_) | ConnectTarget::Wsl { .. } => None,
        }
    }

    pub(crate) fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// How the machine is reached, for its card: `SSH`, a provider's name.
    pub(crate) fn kind(&self) -> Option<&'static str> {
        match self {
            Self::Local => None,
            Self::Ssh(_) => Some("SSH"),
            #[cfg(feature = "cloud")]
            Self::Cloud { provider, .. } => Some(crate::cloud::name(*provider)),
        }
    }

    /// How an error names the machine when no endpoint labels it.
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Local => fl!("automations-this-computer-name"),
            Self::Ssh(target) => target.clone(),
            #[cfg(feature = "cloud")]
            Self::Cloud { id, .. } => id.clone(),
        }
    }

    pub(crate) fn icon(&self) -> &'static str {
        match self {
            Self::Local | Self::Ssh(_) => "icons/computer.svg",
            #[cfg(feature = "cloud")]
            Self::Cloud { .. } => "icons/devices.svg",
        }
    }
}

/// A machine to read, as the window reaches it.
#[derive(Clone, Debug)]
pub(crate) struct Wanted {
    pub(crate) machine: Machine,
    pub(crate) label: String,
    /// The endpoint that reaches it, for commands and panes.
    pub(crate) endpoint: Option<String>,
    pub(crate) host: Host,
}

/// What this machine is called when no local endpoint names it.
pub(crate) fn this_computer() -> String {
    fl!("automations-this-computer")
}

impl HerdrWindow {
    /// This machine first, then every enabled, connected device on another
    /// machine, in the window's device order.
    pub(crate) fn automation_hosts(&self) -> Vec<Wanted> {
        let mut wanted: Vec<Wanted> = Vec::new();
        let local = self
            .endpoints
            .iter()
            .find(|endpoint| Machine::of(&endpoint.connection.target) == Some(Machine::Local));
        if let Ok(host) = Host::new(&ConnectTarget::Local) {
            wanted.push(Wanted {
                machine: Machine::Local,
                label: local.map_or_else(this_computer, |e| e.label.clone()),
                endpoint: local.map(|endpoint| endpoint.id.clone()),
                host,
            });
        }
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let target = &endpoint.connection.target;
            let Some(machine) = Machine::of(target) else {
                continue;
            };
            if !endpoint.enabled
                || self.endpoint_snapshot(index).is_none()
                || wanted.iter().any(|known| known.machine == machine)
            {
                continue;
            }
            let Ok(host) = Host::new(target) else {
                continue;
            };
            wanted.push(Wanted {
                machine,
                label: endpoint.label.clone(),
                endpoint: Some(endpoint.id.clone()),
                host,
            });
        }
        wanted
    }
}
