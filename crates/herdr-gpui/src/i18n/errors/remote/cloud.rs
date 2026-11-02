//! What every cloud provider shares: the saved device list, sandbox
//! images, the exec stream under the relays, and the provider error that
//! hands each provider's failure to its own mapping.

use super::path;
use crate::i18n::{errors::Localize, fl};
use herdr_remote::cloud::{Error, Invalid, ProviderError, Worker, image, install};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Catalog { path: file, source } => fl!(
                "error-remote-cloud-catalog",
                path = path(file),
                error = source.to_string()
            ),
            Self::Json(_) => fl!("error-remote-cloud-json"),
            Self::Invalid(Invalid::Catalog) => fl!(
                "error-remote-cloud-invalid-catalog",
                path = herdr_remote::cloud::CATALOG_FILE
            ),
            Self::Invalid(Invalid::Device) => fl!("error-remote-cloud-invalid-device"),
            Self::Invalid(Invalid::Target) => fl!("error-remote-cloud-invalid-target"),
            Self::Missing => fl!("error-remote-cloud-missing"),
            Self::StateDirectory => fl!("error-remote-cloud-state-directory"),
            Self::Worker(worker) => fl!(
                "error-remote-cloud-worker",
                worker = worker_selector(*worker)
            ),
            Self::Process(_) => fl!("error-remote-cloud-process"),
            Self::Cancelled => fl!("error-remote-cloud-cancelled"),
            Self::Install(failure) => failure.localize(),
            Self::InstallTimeout => fl!("error-remote-cloud-install-timeout"),
            // Only `cloud::unavailable` gives a reason, and it has one.
            Self::Unavailable(_) => fl!("error-remote-cloud-unavailable"),
        }
    }
}

impl Localize for ProviderError {
    fn localize(&self) -> String {
        // `*self`: with no provider feature on, the enum has no variants.
        match *self {
            #[cfg(feature = "azure")]
            Self::Azure(ref error) => error.localize(),
            #[cfg(feature = "boat")]
            Self::Boat(ref error) => error.localize(),
            #[cfg(feature = "coder")]
            Self::Coder(ref error) => error.localize(),
            #[cfg(feature = "container")]
            Self::Container(ref error) => error.localize(),
            #[cfg(feature = "daytona")]
            Self::Daytona(ref error) => error.localize(),
            #[cfg(feature = "opensandbox")]
            Self::OpenSandbox(ref error) => error.localize(),
            #[cfg(feature = "railway")]
            Self::Railway(ref error) => error.localize(),
            #[cfg(feature = "sprites")]
            Self::Sprites(ref error) => error.localize(),
        }
    }
}

impl Localize for image::Error {
    fn localize(&self) -> String {
        match self {
            Self::Busy => fl!("error-remote-cloud-image-busy"),
            Self::Cancelled => fl!("error-remote-cloud-image-cancelled"),
            Self::Catalog { path: file, source } => fl!(
                "error-remote-cloud-image-catalog",
                path = path(file),
                error = source.to_string()
            ),
            Self::Context(provider) => {
                fl!("error-remote-cloud-image-context", provider = *provider)
            }
            Self::Devcontainer(_) => fl!("error-remote-cloud-image-devcontainer"),
            Self::Field(field) => match field {
                image::Field::Catalog => fl!(
                    "error-remote-cloud-image-field-catalog",
                    path = image::catalog::FILE
                ),
                image::Field::ContextPath => fl!("error-remote-cloud-image-field-context-path"),
                image::Field::DevcontainerImage => {
                    fl!("error-remote-cloud-image-field-devcontainer-image")
                }
                image::Field::FileName => fl!("error-remote-cloud-image-field-file-name"),
                image::Field::Projects => fl!("error-remote-cloud-image-field-projects"),
                image::Field::ProjectPath => fl!("error-remote-cloud-image-field-project-path"),
            },
            Self::Host(_) => fl!("error-remote-cloud-image-host"),
            Self::Json(_) => fl!("error-remote-cloud-image-json"),
            Self::Missing(file) => {
                fl!("error-remote-cloud-image-missing", path = file.as_str())
            }
            Self::StateDirectory => fl!("error-remote-cloud-image-state-directory"),
            Self::TooLarge => fl!("error-remote-cloud-image-too-large"),
            Self::Truncated => fl!("error-remote-cloud-image-truncated"),
            Self::Unknown => fl!("error-remote-cloud-image-unknown"),
            Self::Unsupported(image::Unsupported::Compose) => {
                fl!("error-remote-cloud-image-unsupported-compose")
            }
            Self::Unsupported(image::Unsupported::NoBuild) => {
                fl!("error-remote-cloud-image-unsupported-no-build")
            }
            Self::Unsupported(image::Unsupported::Provider) => {
                fl!("error-remote-cloud-image-unsupported-provider")
            }
        }
    }
}

impl Localize for install::Failure {
    fn localize(&self) -> String {
        use install::Retry;
        match self {
            Self::Missing => fl!("error-remote-cloud-install-missing"),
            Self::NoCurl => fl!("error-remote-cloud-install-no-curl"),
            Self::NoOutput => fl!("error-remote-cloud-install-no-output"),
            Self::NotInstalled { machine, retry } => {
                let retry = match retry {
                    Retry::Add => "add",
                    Retry::Allow => "allow",
                    Retry::Enable => "enable",
                    Retry::FinishSetup => "finish-setup",
                };
                match machine {
                    Some(machine) => fl!(
                        "error-remote-cloud-install-not-installed",
                        machine = machine.as_str(),
                        retry = retry
                    ),
                    None => fl!(
                        "error-remote-cloud-install-not-installed-unnamed",
                        retry = retry
                    ),
                }
            }
            // What the machine printed, in its own words.
            Self::Output(output) => fl!("error-remote-cloud-install", detail = output.as_str()),
            Self::Unreachable => fl!("error-remote-cloud-install-unreachable"),
        }
    }
}

#[cfg(any(feature = "opensandbox", feature = "sprites"))]
impl Localize for herdr_remote::cloud::socket::Error {
    fn localize(&self) -> String {
        match self {
            Self::Url => fl!("error-remote-cloud-socket-url"),
            Self::Connect(_) => fl!("error-remote-cloud-socket-connect"),
            Self::Tls(_) => fl!("error-remote-cloud-socket-tls"),
            Self::Stream(_) => fl!("error-remote-cloud-socket-stream"),
            Self::Header(_) => fl!("error-remote-cloud-socket-header"),
            Self::HeaderName(_) => fl!("error-remote-cloud-socket-header-name"),
            Self::Closed => fl!("error-remote-cloud-socket-closed"),
        }
    }
}

/// The Fluent selector for each worker that can stop.
pub(super) fn worker_selector(worker: Worker) -> &'static str {
    match worker {
        Worker::Destroy => "destroy",
        Worker::ImageBuild => "image-build",
        Worker::ImageReset => "image-reset",
        Worker::Job => "job",
        Worker::Settings => "settings",
        Worker::Setup => "setup",
        Worker::SignIn => "sign-in",
    }
}
