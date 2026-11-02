use super::*;
use herdr_remote::cloud::Value;
use herdr_remote::opensandbox::{Error, Status};

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("opensandbox.api_url"),
        Error::Field(Value::Key("opensandbox.image")),
        Error::Encoding(Value::Key("HERDR_OPENSANDBOX_API_URL")),
        Error::MissingKey,
        Error::Authentication,
        Error::Forbidden,
        Error::Gone("sb-1".into()),
        Error::Stopped("OOMKilled".into()),
        Error::StartTimeout,
        Error::Status(Status {
            code: 500,
            message: None,
        }),
        Error::Status(Status {
            code: 404,
            message: Some("sandbox not found".into()),
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Json(json()),
        Error::Header(header()),
        Error::Socket(herdr_remote::cloud::socket::Error::Closed),
        Error::Shell("permission denied".into()),
        Error::Output(io()),
        Error::Executable(io()),
        Error::Cancelled,
        Error::Account,
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::Storage(remote()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
