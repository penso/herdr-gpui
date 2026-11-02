//! What a cloud provider refused, shared by every provider's `Field` and
//! `Encoding`: one message per sentence, chosen by `$value`, with a setting
//! or environment variable named as written.

use crate::i18n::fl;
use herdr_remote::cloud::Value;

/// `value` is not valid.
pub(super) fn invalid(value: Value) -> String {
    match value {
        Value::Key(key) => fl!("error-remote-cloud-value-invalid-key", key = key),
        value => fl!("error-remote-cloud-value-invalid", value = selector(value)),
    }
}

/// `value` is not UTF-8. Every provider but containers reads text that
/// may not be.
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "daytona",
    feature = "opensandbox",
    feature = "sprites"
))]
pub(super) fn not_utf8(value: Value) -> String {
    match value {
        Value::Key(key) => fl!("error-remote-cloud-value-encoding-key", key = key),
        value => fl!("error-remote-cloud-value-encoding", value = selector(value)),
    }
}

/// The Fluent selector for each value but a key.
fn selector(value: Value) -> &'static str {
    match value {
        Value::Account => "account",
        Value::AzureAccessToken => "azure-access-token",
        Value::AzureClientSecret => "azure-client-secret",
        Value::AzureDeviceAccount => "azure-device-account",
        Value::AzureSandboxEndpoint => "azure-sandbox-endpoint",
        Value::AzureSandboxId => "azure-sandbox-id",
        Value::AzureSshPublicKey => "azure-ssh-public-key",
        Value::BoatApiKey => "boat-api-key",
        Value::BoatAutoStop => "boat-auto-stop",
        Value::BoatPending => "boat-pending",
        Value::BoatPendingPath => "boat-pending-path",
        Value::BoatPendingSize => "boat-pending-size",
        Value::BoatSandboxId => "boat-sandbox-id",
        Value::BoatSshPrivateKey => "boat-ssh-private-key",
        Value::BoatSshPublicKey => "boat-ssh-public-key",
        Value::CoderIdentifier => "coder-identifier",
        Value::CoderWorkspace => "coder-workspace",
        Value::ContainerEngine => "container-engine",
        Value::DaytonaApiKey => "daytona-api-key",
        Value::DaytonaSandboxId => "daytona-sandbox-id",
        Value::DaytonaSnapshot => "daytona-snapshot",
        Value::ExecSessionId => "exec-session-id",
        Value::ExecdEndpoint => "execd-endpoint",
        Value::OpenSandboxApiKey => "opensandbox-api-key",
        Value::RemoteCommand => "remote-command",
        Value::SandboxId => "sandbox-id",
        Value::SpriteName => "sprite-name",
        Value::SpritePrefix => "sprite-prefix",
        Value::SpritesApiToken => "sprites-api-token",
        Value::WorkspaceName => "workspace-name",
        Value::Key(_) => "key",
    }
}
