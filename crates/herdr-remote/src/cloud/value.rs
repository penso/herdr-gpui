//! What a cloud provider refused as not valid or not UTF-8. A setting or
//! environment variable is named as written; every other value is a
//! variant, so the app can translate each sentence whole. `Display` writes
//! each to open the sentence it starts.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    // keep-sorted start
    Account,
    AzureAccessToken,
    AzureClientSecret,
    AzureDeviceAccount,
    AzureSandboxEndpoint,
    AzureSandboxId,
    AzureSshPublicKey,
    BoatApiKey,
    BoatAutoStop,
    BoatPending,
    BoatPendingPath,
    BoatPendingSize,
    BoatSandboxId,
    BoatSshPrivateKey,
    BoatSshPublicKey,
    CoderIdentifier,
    CoderWorkspace,
    ContainerEngine,
    DaytonaApiKey,
    DaytonaSandboxId,
    DaytonaSnapshot,
    ExecdEndpoint,
    ExecSessionId,
    /// A setting such as `daytona.api_url` or an environment variable,
    /// shown as written in every language.
    Key(&'static str),
    OpenSandboxApiKey,
    RemoteCommand,
    SandboxId,
    SpriteName,
    SpritePrefix,
    SpritesApiToken,
    WorkspaceName,
    // keep-sorted end
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            // keep-sorted start
            Self::Account => "Account",
            Self::AzureAccessToken => "Azure access token",
            Self::AzureClientSecret => "Azure client secret",
            Self::AzureDeviceAccount => "Saved Azure device account",
            Self::AzureSandboxEndpoint => "Azure sandbox endpoint",
            Self::AzureSandboxId => "Azure sandbox ID",
            Self::AzureSshPublicKey => "Azure SSH public key",
            Self::BoatApiKey => "Boat API key",
            Self::BoatAutoStop => "boat.auto_stop (never or 1–2592000 seconds)",
            Self::BoatPending => "Boat pending creation",
            Self::BoatPendingPath => "Boat pending path",
            Self::BoatPendingSize => "Boat pending creation size",
            Self::BoatSandboxId => "Boat sandbox ID",
            Self::BoatSshPrivateKey => "Boat SSH private key",
            Self::BoatSshPublicKey => "Boat SSH public key",
            Self::CoderIdentifier => "Coder identifier",
            Self::CoderWorkspace => "Coder workspace",
            Self::ContainerEngine => "Container engine",
            Self::DaytonaApiKey => "Daytona API key",
            Self::DaytonaSandboxId => "Daytona sandbox ID",
            Self::DaytonaSnapshot => "Daytona snapshot name",
            Self::ExecdEndpoint => "Execd endpoint",
            Self::ExecSessionId => "Exec session ID",
            Self::Key(key) => key,
            Self::OpenSandboxApiKey => "OpenSandbox API key",
            Self::RemoteCommand => "Remote command",
            Self::SandboxId => "Sandbox ID",
            Self::SpriteName => "Sprite name",
            Self::SpritePrefix => "Sprite prefix",
            Self::SpritesApiToken => "Sprites API token",
            Self::WorkspaceName => "Workspace name",
            // keep-sorted end
        })
    }
}
