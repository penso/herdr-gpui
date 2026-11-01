use super::*;
use herdr_orchestrator::{Error, azure};

#[test]
fn every_azure_variant_has_a_message() {
    let repository = || "contoso/project/repo".to_owned();
    assert_localized(&[
        azure::Error::Address("contoso/repo".into()),
        azure::Error::Authentication,
        azure::Error::Directory(herdr_client::local_path::PathError::MissingHome),
        azure::Error::EnvironmentRefused,
        azure::Error::Forbidden {
            repository: repository(),
        },
        azure::Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        azure::Error::Network {
            origin: "https://dev.azure.com".into(),
            source: Box::new(ureq::Error::HostNotFound),
        },
        azure::Error::NoOrganization("contoso".into()),
        azure::Error::NoStore(herdr_credentials::Error::Policy),
        azure::Error::NotFound {
            repository: repository(),
        },
        azure::Error::Organization("two words".into()),
        azure::Error::RateLimit,
        azure::Error::Read(std::io::Error::other("pipe closed")),
        azure::Error::Record,
        azure::Error::SignIn {
            repository: repository(),
        },
        azure::Error::Size,
        azure::Error::Status(500),
        azure::Error::Store(herdr_credentials::Error::Permissions),
        azure::Error::Token,
        azure::Error::TooManyOrganizations,
    ]);
    #[cfg(target_os = "linux")]
    assert_localized(&[azure::Error::NoKeyring(
        herdr_credentials::Error::SecretServiceUnavailable,
    )]);
}

#[test]
fn the_orchestrator_shows_an_azure_failure_as_its_own() {
    assert_localized(&[Error::Azure(azure::Error::RateLimit)]);
}
