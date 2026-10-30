use super::*;
use herdr_remote::cloud::{Error, ProviderError, image};
use std::path::PathBuf;

#[test]
fn every_cloud_variant_has_a_message() {
    assert_localized(&[
        Error::Catalog {
            path: PathBuf::from("/tmp/cloud.json"),
            source: io(),
        },
        Error::Json(json()),
        Error::Invalid("cloud device"),
        Error::Missing,
        Error::StateDirectory,
        Error::Worker("setup"),
        Error::Process(io()),
        Error::Cancelled,
        Error::Install("the installer finished but Herdr is not on the expected paths".into()),
        Error::InstallTimeout,
        Error::Unavailable("Cloud devices are unavailable on Windows."),
    ]);
}

#[test]
fn every_image_variant_has_a_message() {
    assert_localized(&[
        image::Error::Busy,
        image::Error::Cancelled,
        image::Error::Catalog {
            path: PathBuf::from("/tmp/images.json"),
            source: io(),
        },
        image::Error::Context("Daytona"),
        image::Error::Devcontainer(json()),
        image::Error::Field("the dev container's image"),
        image::Error::Host(client()),
        image::Error::Json(json()),
        image::Error::Missing(".devcontainer/Dockerfile.dev".into()),
        image::Error::StateDirectory,
        image::Error::TooLarge,
        image::Error::Truncated,
        image::Error::Unknown,
        image::Error::Unsupported("a dev container defined by Docker Compose"),
    ]);
}

#[test]
fn every_provider_delegates() {
    // `ProviderError` has a variant per provider feature turned on.
    let errors: Vec<ProviderError> = vec![
        #[cfg(feature = "azure")]
        herdr_remote::azure::Error::Size.into(),
        #[cfg(feature = "boat")]
        herdr_remote::boat::Error::Size.into(),
        #[cfg(feature = "coder")]
        herdr_remote::coder::Error::Size.into(),
        #[cfg(feature = "container")]
        herdr_remote::container::Error::NoEngine.into(),
        #[cfg(feature = "daytona")]
        herdr_remote::daytona::Error::Size.into(),
        #[cfg(feature = "opensandbox")]
        herdr_remote::opensandbox::Error::Size.into(),
        #[cfg(feature = "railway")]
        herdr_remote::railway::Error::KeyPath.into(),
        #[cfg(feature = "sprites")]
        herdr_remote::sprites::Error::Size.into(),
    ];
    assert_localized(&errors);
}

/// `Tls` and `Stream` hold `rustls` and `tungstenite` errors, which this
/// crate does not depend on, so they cannot be built here.
#[cfg(any(feature = "opensandbox", feature = "sprites"))]
#[test]
fn every_socket_variant_has_a_message() {
    use herdr_remote::cloud::socket::Error;
    assert_localized(&[
        Error::Url,
        Error::Connect(io()),
        Error::Header(header()),
        Error::HeaderName(ureq::http::HeaderName::from_bytes(b"\n").unwrap_err()),
        Error::Closed,
    ]);
}
