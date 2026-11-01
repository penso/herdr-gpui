//! What can go wrong reading or driving `herdr-automate` on a host.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("{path} is already approved on {host}")]
    Approved { path: String, host: String },
    #[error(transparent)]
    Build(#[from] crate::host_build::Error),
    #[error("herdr-automate: the command failed")]
    Failed,
    #[error("herdr-automate printed output this app cannot read")]
    Json(#[source] serde_json::Error),
    #[error("herdr-automate is not installed on this host")]
    Missing,
    #[error("This app has no herdr-automate build for {platform}")]
    NoBuild { platform: String },
    #[error("{host} has no automation script {path}")]
    NoScript { path: String, host: String },
    #[error("{host} has not been read yet; try again in a moment")]
    NotRead { host: String },
    #[error("Automations are turned off in Herdr UI: Settings, Sidebar, Show automations")]
    Off,
    #[error(
        "herdr-automate on this host speaks report protocol {found}; this app reads {expected}. Update this app."
    )]
    Protocol { found: u32, expected: u32 },
    #[error("herdr-automate: {0}")]
    Refused(String),
    #[error("Could not run herdr-automate: {0}")]
    Run(#[source] herdr_client::Error),
    #[error("No connected host is named {0}")]
    UnknownHost(String),
    #[error(
        "Could not read the version of the herdr-automate this app ships, so it cannot be installed"
    )]
    UnknownVersion,
    #[error("Automations cannot reach this kind of host")]
    UnsupportedHost,
    #[error("herdr-automate: unexpected version output: {0}")]
    Version(String),
    // keep-sorted end
}
