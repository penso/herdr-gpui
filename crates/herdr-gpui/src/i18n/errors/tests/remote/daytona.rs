use super::*;
use herdr_remote::daytona::{Error, Status};

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("daytona.api_url"),
        Error::Field("Daytona sandbox ID"),
        Error::Encoding("HERDR_DAYTONA_API_URL"),
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
