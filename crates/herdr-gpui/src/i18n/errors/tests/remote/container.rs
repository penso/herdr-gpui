use super::*;
use herdr_remote::cloud::Value;
use herdr_remote::container::Error;

#[test]
fn every_variant_has_a_message() {
    assert_localized(&[
        Error::Field(Value::Key("[container] cpus")),
        Error::NoEngine,
        Error::Spawn {
            engine: "docker",
            source: io(),
        },
        Error::Failed {
            engine: "podman",
            action: "pull",
            detail: "manifest unknown".into(),
        },
        Error::Timeout {
            engine: "docker",
            action: "inspect",
        },
        Error::Gone("herdr-1".into()),
        Error::Suspended("herdr-1".into()),
        Error::NameTaken("herdr-1".into()),
        Error::NotOurs("herdr-1".into()),
        Error::Limit(5),
        Error::Cancelled,
        Error::BuildContext(io()),
        Error::ImageNotOurs("herdr/project:abc".into()),
        Error::Cloud(herdr_remote::cloud::Error::Cancelled),
        Error::Storage(remote()),
    ]);
}

#[test]
fn the_bridge_shows_the_client_error() {
    assert_localized(&[Error::Bridge(client())]);
}
