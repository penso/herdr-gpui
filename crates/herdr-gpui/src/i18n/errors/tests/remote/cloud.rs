use super::*;
use herdr_remote::cloud::{Error, Invalid, ProviderError, image};
use std::path::PathBuf;

#[test]
fn every_cloud_variant_has_a_message() {
    assert_localized(&[
        Error::Catalog {
            path: PathBuf::from("/tmp/cloud.json"),
            source: io(),
        },
        Error::Json(json()),
        Error::Invalid(Invalid::Catalog),
        Error::Invalid(Invalid::Device),
        Error::Invalid(Invalid::Target),
        Error::Missing,
        Error::StateDirectory,
        Error::Worker(crate::cloud::Worker::Setup),
        Error::Process(io()),
        Error::Cancelled,
        Error::InstallTimeout,
        Error::Unavailable("Cloud devices are unavailable on Windows."),
    ]);
}

#[test]
fn every_worker_has_a_message() {
    use herdr_remote::cloud::Worker;
    let workers = [
        Worker::Destroy,
        Worker::ImageBuild,
        Worker::ImageReset,
        Worker::Job,
        Worker::Settings,
        Worker::Setup,
        Worker::SignIn,
    ];
    assert_localized(&workers.map(Error::Worker));
    #[cfg(feature = "coder")]
    assert_localized(&workers.map(herdr_remote::coder::Error::Worker));
}

#[test]
fn every_install_failure_has_a_message() {
    use herdr_remote::cloud::install::{Failure, Retry};
    let mut failures = vec![
        Failure::Missing,
        Failure::NoCurl,
        Failure::NoOutput,
        Failure::Unreachable,
    ];
    for retry in [Retry::Add, Retry::Allow, Retry::Enable, Retry::FinishSetup] {
        failures.push(Failure::NotInstalled {
            machine: Some("build-box".into()),
            retry,
        });
        failures.push(Failure::NotInstalled {
            machine: None,
            retry,
        });
    }
    assert_localized(&failures.into_iter().map(Error::Install).collect::<Vec<_>>());
    assert_localized(&[Error::Install(Failure::Output("exit 9: boom".into()))]);
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
        image::Error::Field(image::Field::Catalog),
        image::Error::Field(image::Field::ContextPath),
        image::Error::Field(image::Field::DevcontainerImage),
        image::Error::Field(image::Field::FileName),
        image::Error::Field(image::Field::Projects),
        image::Error::Field(image::Field::ProjectPath),
        image::Error::Host(client()),
        image::Error::Json(json()),
        image::Error::Missing(".devcontainer/Dockerfile.dev".into()),
        image::Error::StateDirectory,
        image::Error::TooLarge,
        image::Error::Truncated,
        image::Error::Unknown,
        image::Error::Unsupported(image::Unsupported::Compose),
        image::Error::Unsupported(image::Unsupported::NoBuild),
        image::Error::Unsupported(image::Unsupported::Provider),
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
