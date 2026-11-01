//! Azure DevOps failures from `error-orchestrator-azure.ftl`. Network,
//! parser, and I/O text stays a placeable.

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
            Self::NoCheckout { repository } => fl!(
                "error-orchestrator-azure-no-checkout",
                repository = repository.as_str()
            ),
            Self::NotFound { repository } => fl!(
                "error-orchestrator-azure-not-found",
                repository = repository.as_str()
            ),
            Self::RateLimit => fl!("error-orchestrator-azure-rate-limit"),
            Self::Read(error) => fl!("error-orchestrator-azure-read", error = error.to_string()),
            Self::Reader => fl!("error-orchestrator-azure-reader"),
            Self::SignIn { repository } => fl!(
                "error-orchestrator-azure-sign-in",
                repository = repository.as_str()
            ),
            Self::Size => fl!("error-orchestrator-azure-size"),
            Self::Status(status) => fl!("error-orchestrator-azure-status", status = *status),
            Self::Token => fl!("error-orchestrator-azure-token"),
        }
    }
}
