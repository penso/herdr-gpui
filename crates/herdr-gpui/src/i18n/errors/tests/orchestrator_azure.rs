use super::*;
use herdr_orchestrator::{Error, azure};

#[test]
fn every_azure_variant_has_a_message() {
    let repository = || "contoso/project/repo".to_owned();
    assert_localized(&[
        azure::Error::Address("contoso/repo".into()),
        azure::Error::Authentication,
        azure::Error::Forbidden {
            repository: repository(),
        },
        azure::Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        azure::Error::Network {
            origin: "https://dev.azure.com".into(),
            source: Box::new(ureq::Error::HostNotFound),
        },
        azure::Error::NoCheckout {
            repository: repository(),
        },
        azure::Error::NotFound {
            repository: repository(),
        },
        azure::Error::RateLimit,
        azure::Error::Read(std::io::Error::other("pipe closed")),
        azure::Error::Reader,
        azure::Error::SignIn {
            repository: repository(),
        },
        azure::Error::Size,
        azure::Error::Status(500),
        azure::Error::Token,
    ]);
}

#[test]
fn the_orchestrator_shows_an_azure_failure_as_its_own() {
    assert_localized(&[Error::Azure(azure::Error::RateLimit)]);
}
