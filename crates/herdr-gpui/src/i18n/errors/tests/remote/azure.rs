use super::*;
use herdr_remote::azure::Error;
use herdr_remote::cloud::Value;

/// `Stream`, `StreamHeader`, and `Tls` hold `tungstenite` and `rustls`
/// errors, which this crate does not depend on, so they cannot be built
/// here.
#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Field(Value::Key("azure.subscription_id")),
        Error::Encoding(Value::AzureSshPublicKey),
        Error::MissingSubscription,
        Error::MissingCredentials,
        Error::Authentication("AADSTS7000215: Invalid client secret".into()),
        Error::Forbidden("AuthorizationFailed".into()),
        Error::Status(500, "InternalServerError".into()),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Json(json()),
        Error::Header(header()),
        Error::Vm("OSProvisioningTimedOut".into()),
        Error::Deleted,
        Error::StartTimeout,
        Error::Sandbox("ImagePullBackOff".into()),
        Error::SandboxDeleted,
        Error::SandboxTimeout,
        Error::DeleteTimeout,
        Error::StreamClosed,
        Error::StreamMessage,
        Error::Executable(io()),
        Error::Cancelled,
        Error::Gateway,
        Error::SshMissing(io()),
        Error::SshKeygen,
        Error::AzMissing,
        Error::AzFailed("ERROR: Please run 'az login'".into()),
        Error::Process(io()),
        Error::Storage(Box::new(io())),
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::SaveDevice {
            id: "herdr-vm-1".into(),
            source: remote(),
        },
        Error::Account,
        Error::NotRunning,
        Error::SkuNotAvailable("Standard_D4s_v5 in westeurope".into()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
