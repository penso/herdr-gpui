use super::*;
use herdr_remote::Error;
use std::io;

#[cfg(feature = "azure")]
mod azure;
#[cfg(feature = "boat")]
mod boat;
#[cfg(feature = "cloud")]
mod cloud;
#[cfg(feature = "coder")]
mod coder;
#[cfg(feature = "container")]
mod container;
#[cfg(feature = "daytona")]
mod daytona;
#[cfg(feature = "opensandbox")]
mod opensandbox;
#[cfg(feature = "railway")]
mod railway;
#[cfg(feature = "sprites")]
mod sprites;
mod teleport;

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

fn client() -> herdr_client::Error {
    herdr_client::Error::ScriptTimeout
}

/// A boxed `herdr-remote` error, as providers keep their saved device's.
#[cfg(feature = "cloud")]
fn remote() -> Box<Error> {
    Box::new(Error::NoConfig)
}

#[cfg(feature = "cloud")]
fn network() -> ureq::Error {
    ureq::Error::StatusCode(502)
}

#[cfg(feature = "cloud")]
fn header() -> ureq::http::header::InvalidHeaderValue {
    ureq::http::HeaderValue::from_str("\n").unwrap_err()
}

#[test]
fn wrapped_errors_delegate() {
    assert_localized(&[
        Error::Client(client()),
        Error::Credentials(herdr_credentials::Error::Io(io())),
    ]);
}

#[test]
fn every_variant_has_a_message() {
    let errors = vec![
        Error::Config(Box::new(crate::Error::ConnectionBusy)),
        Error::NoConfig,
        Error::PreferencesPath,
        Error::Teleport(herdr_remote::teleport::Error::Cancelled),
        #[cfg(feature = "cloud")]
        Error::Cloud(herdr_remote::cloud::Error::Missing),
        #[cfg(feature = "cloud")]
        Error::SandboxImage(herdr_remote::cloud::image::Error::Busy),
        #[cfg(feature = "azure")]
        Error::Provider(herdr_remote::cloud::ProviderError::Azure(
            herdr_remote::azure::Error::Size,
        )),
    ];
    assert_localized(&errors);
}

#[test]
fn system_text_is_shown_as_it_is() {
    assert_verbatim(&[
        Error::Config("an unknown loader failure".into()),
        Error::Io(io()),
        Error::Json(json()),
    ]);
    assert_localized(&[Error::Path(
        herdr_client::local_path::PathError::MissingHome,
    )]);
}
