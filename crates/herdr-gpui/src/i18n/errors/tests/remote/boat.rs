use super::*;
use herdr_remote::boat::{Error, Status};
use herdr_remote::cloud::Value;
use std::{path::PathBuf, time::SystemTime};

#[test]
fn every_variant_has_a_message() {
    let clock = SystemTime::UNIX_EPOCH
        .duration_since(SystemTime::now())
        .unwrap_err();
    assert_localized(&[
        Error::Url("boat.api_url"),
        Error::Field(Value::Key("boat.machine_type")),
        Error::Encoding(Value::Key("HERDR_BOAT_API_URL")),
        Error::MissingKey,
        Error::Authentication,
        Error::Forbidden,
        Error::Status(Status {
            code: 500,
            message: None,
        }),
        Error::Status(Status {
            code: 422,
            message: Some("machine type unknown".into()),
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Json(json()),
        Error::Header(header()),
        Error::Sandbox("out of capacity".into()),
        Error::Deleted,
        Error::StartTimeout,
        Error::Cancelled,
        Error::PendingIo {
            path: PathBuf::from("/tmp/boat-pending.json"),
            source: io(),
        },
        Error::PendingBusy,
        Error::PendingExpired,
        Error::PendingCreation(Box::new(Error::Size)),
        Error::Clock(clock),
        Error::Gateway,
        Error::SshMissing(io()),
        Error::SshKey(io()),
        Error::SshKeygen,
        Error::GatewayFile(io()),
        Error::SaveDevice {
            id: "sb-1".into(),
            source: remote(),
        },
        Error::Setup {
            id: "sb-1".into(),
            source: remote(),
        },
        Error::Account,
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::KeyFile(remote()),
        Error::Storage(remote()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
