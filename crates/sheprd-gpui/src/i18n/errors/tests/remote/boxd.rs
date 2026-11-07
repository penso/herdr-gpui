use super::*;
use sheprd_remote::boxd::{Code, Error, Status};
use sheprd_remote::cloud::Value;

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("boxd.api_url"),
        Error::Field(Value::BoxdMachine),
        Error::Field(Value::BoxdApiKey),
        Error::Encoding(Value::Key("HERDR_BOXD_API_URL")),
        Error::MissingKey,
        Error::Authentication,
        Error::Forbidden,
        Error::Gone("sheprd-calm-otter".into()),
        Error::Failed("sheprd-calm-otter".into()),
        Error::NotReady("sheprd-calm-otter".into()),
        Error::Status(Status {
            code: Code::Grpc(5),
            message: None,
        }),
        Error::Status(Status {
            code: Code::Grpc(6),
            message: Some("a machine with that name already exists".into()),
        }),
        Error::Status(Status {
            code: Code::Http(502),
            message: None,
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::Wire,
        Error::Header(header()),
        Error::Socket(sheprd_remote::cloud::socket::Error::Closed),
        Error::Stream(io()),
        Error::Output(io()),
        Error::Executable(io()),
        Error::Cancelled,
        Error::Account,
        Error::Cloud(sheprd_remote::cloud::Error::Cancelled),
        Error::Storage(remote()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
