use super::*;
use herdr_remote::cloud::Value;
use herdr_remote::coder::{Error, Status};
use std::path::PathBuf;

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Url("coder.url"),
        Error::Field(Value::Key("coder.organization")),
        Error::Encoding(Value::Key("HERDR_CODER_OAUTH_CLIENT_SECRET")),
        Error::Missing("url"),
        Error::Authentication,
        Error::Client,
        Error::Forbidden,
        Error::Status(Status {
            code: 500,
            message: None,
        }),
        Error::Status(Status {
            code: 400,
            message: Some("Validation failed".into()),
        }),
        Error::Network(network()),
        Error::Read(io()),
        Error::Size,
        Error::json(json()),
        Error::Header(header()),
        Error::Listen {
            address: "127.0.0.1:8085".into(),
            source: io(),
        },
        Error::Redirect(io()),
        Error::Cancelled,
        Error::Timeout,
        Error::Authorization("access_denied".into()),
        Error::Token,
        Error::Random(getrandom::Error::UNSUPPORTED),
        Error::Workspace("build failed".into()),
        Error::Deleted,
        Error::BuildTimeout,
        Error::DeleteTimeout,
        Error::Cli,
        Error::Deployment,
        Error::Catalog {
            path: PathBuf::from("/tmp/coder.json"),
            source: io(),
        },
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::Storage(remote()),
        Error::Worker(crate::cloud::Worker::SignIn),
        Error::AmbiguousTemplate {
            name: "Docker".into(),
            matches: "docker, docker-gpu".into(),
        },
        Error::Template {
            name: Some("rust".into()),
            available: "docker, kubernetes".into(),
        },
        Error::Template {
            name: None,
            available: "docker, kubernetes".into(),
        },
        Error::Preset {
            template: "docker".into(),
            name: "large".into(),
            available: "small, medium".into(),
        },
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
