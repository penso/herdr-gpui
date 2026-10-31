use super::*;
use herdr_remote::cloud::Value;
use herdr_remote::sprites::{Error, Status};

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("sprites.api_url"),
        Error::Field(Value::SpriteName),
        Error::Encoding(Value::Key("HERDR_SPRITES_API_URL")),
        Error::MissingToken,
        Error::Authentication,
        Error::Forbidden,
        Error::Gone("herdr-1".into()),
        Error::Status(Status {
            code: 500,
            message: None,
        }),
        Error::Status(Status {
            code: 409,
            message: Some("sprite already exists".into()),
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Json(json()),
        Error::Header(header()),
        Error::Socket(herdr_remote::cloud::socket::Error::Closed),
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
