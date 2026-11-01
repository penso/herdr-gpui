//! Azure DevOps failures from `error-orchestrator-azure.ftl`. Network,
//! parser, and I/O text stays a placeable; a path or store failure is
//! localized on its own.

use super::Localize;
use crate::i18n::fl;
use herdr_orchestrator::azure::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Address(repository) => fl!(
                "error-orchestrator-azure-address",
                repository = format!("{repository:?}")
            ),
            Self::Authentication => fl!("error-orchestrator-azure-authentication"),
            Self::Directory(error) => fl!(
                "error-orchestrator-azure-directory",
                error = error.localize()
            ),
            Self::EnvironmentRefused => fl!("error-orchestrator-azure-environment-refused"),
            Self::Forbidden { repository } => fl!(
                "error-orchestrator-azure-forbidden",
                repository = repository.as_str()
            ),
            Self::Json(error) => fl!("error-orchestrator-azure-json", error = error.to_string()),
            Self::Network { origin, source } => fl!(
                "error-orchestrator-azure-network",
                origin = origin.as_str(),
                error = source.to_string()
            ),
            #[cfg(target_os = "linux")]
            Self::NoKeyring(_) => fl!("error-orchestrator-azure-no-keyring"),
            Self::NoOrganization(organization) => fl!(
                "error-orchestrator-azure-no-organization",
                organization = organization.as_str()
            ),
            Self::NoStore(_) => fl!("error-orchestrator-azure-no-store"),
            Self::NotFound { repository } => fl!(
                "error-orchestrator-azure-not-found",
                repository = repository.as_str()
            ),
            Self::Organization(organization) => fl!(
                "error-orchestrator-azure-organization",
                organization = format!("{organization:?}")
            ),
            Self::RateLimit => fl!("error-orchestrator-azure-rate-limit"),
            Self::Read(error) => fl!("error-orchestrator-azure-read", error = error.to_string()),
            Self::Record => fl!("error-orchestrator-azure-record"),
            Self::SignIn { repository } => fl!(
                "error-orchestrator-azure-sign-in",
                repository = repository.as_str()
            ),
            Self::Size => fl!("error-orchestrator-azure-size"),
            Self::Status(status) => fl!("error-orchestrator-azure-status", status = *status),
            Self::Store(error) => fl!("error-orchestrator-azure-store", error = error.localize()),
            Self::Token => fl!("error-orchestrator-azure-token"),
            Self::TooManyOrganizations => fl!(
                "error-orchestrator-azure-too-many-organizations",
                limit = herdr_orchestrator::azure::saved::MAX_ORGANIZATIONS
            ),
        }
    }
}
