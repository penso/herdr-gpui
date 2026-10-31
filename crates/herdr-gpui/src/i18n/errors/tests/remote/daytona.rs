use super::*;
use herdr_remote::cloud::Value;
use herdr_remote::daytona::{Error, Status};

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("daytona.api_url"),
        Error::Field(Value::DaytonaSandboxId),
        Error::Encoding(Value::Key("HERDR_DAYTONA_API_URL")),
        Error::MissingKey,
        Error::Authentication,
        Error::Forbidden,
        Error::Status(Status {
            code: 503,
            message: None,
        }),
        Error::Status(Status {
            code: 400,
            message: Some("invalid snapshot".into()),
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Json(json()),
        Error::Header(header()),
        Error::Sandbox("error".into()),
        Error::Deleted,
        Error::Snapshot("build failed".into()),
        Error::StartTimeout,
        Error::Cancelled,
        Error::Gateway,
        Error::SshMissing(io()),
        Error::GatewayFile(io()),
        Error::Account,
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::Storage(remote()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}

/// Every provider's `Field` and `Encoding` share these messages; Daytona
/// has both variants, so it carries each value through.
#[test]
fn every_refused_value_has_both_messages() {
    let values = [
        // keep-sorted start
        Value::Account,
        Value::AzureAccessToken,
        Value::AzureClientSecret,
        Value::AzureDeviceAccount,
        Value::AzureSandboxEndpoint,
        Value::AzureSandboxId,
        Value::AzureSshPublicKey,
        Value::BoatApiKey,
        Value::BoatAutoStop,
        Value::BoatPending,
        Value::BoatPendingPath,
        Value::BoatPendingSize,
        Value::BoatSandboxId,
        Value::BoatSshPrivateKey,
        Value::BoatSshPublicKey,
        Value::CoderIdentifier,
        Value::CoderWorkspace,
        Value::ContainerEngine,
        Value::DaytonaApiKey,
        Value::DaytonaSandboxId,
        Value::DaytonaSnapshot,
        Value::ExecdEndpoint,
        Value::ExecSessionId,
        Value::OpenSandboxApiKey,
        Value::RemoteCommand,
        Value::SandboxId,
        Value::SpriteName,
        Value::SpritePrefix,
        Value::SpritesApiToken,
        Value::WorkspaceName,
        // keep-sorted end
    ];
    // A value added without a line above fails to build here.
    for value in values {
        match value {
            Value::Key(_) => unreachable!("keys are shown as written"),
            Value::Account
            | Value::AzureAccessToken
            | Value::AzureClientSecret
            | Value::AzureDeviceAccount
            | Value::AzureSandboxEndpoint
            | Value::AzureSandboxId
            | Value::AzureSshPublicKey
            | Value::BoatApiKey
            | Value::BoatAutoStop
            | Value::BoatPending
            | Value::BoatPendingPath
            | Value::BoatPendingSize
            | Value::BoatSandboxId
            | Value::BoatSshPrivateKey
            | Value::BoatSshPublicKey
            | Value::CoderIdentifier
            | Value::CoderWorkspace
            | Value::ContainerEngine
            | Value::DaytonaApiKey
            | Value::DaytonaSandboxId
            | Value::DaytonaSnapshot
            | Value::ExecSessionId
            | Value::ExecdEndpoint
            | Value::OpenSandboxApiKey
            | Value::RemoteCommand
            | Value::SandboxId
            | Value::SpriteName
            | Value::SpritePrefix
            | Value::SpritesApiToken
            | Value::WorkspaceName => {}
        }
    }
    assert_localized(&values.map(Error::Field));
    assert_localized(&values.map(Error::Encoding));
    let key = Value::Key("daytona.target");
    assert_localized(&[Error::Field(key), Error::Encoding(key)]);
}
