//! The in-app updater's failures, from `error-updater.ftl`. Versions,
//! command output, and exit statuses are placeables; the sources the
//! updater keeps for the log (remote diagnostics, signatures, I/O) are not
//! shown, as its `Display` does not show them.

use super::Localize;
use crate::i18n::fl;
use chrono::{DateTime, DurationRound as _, Local, TimeDelta, Utc};
use herdr_updater::Error;

/// The local minute to retry at, as `herdr_updater::retry_hint` words it:
/// the first whole minute at or after `reset`.
fn retry_time(reset: DateTime<Utc>) -> String {
    reset
        .duration_round_up(TimeDelta::minutes(1))
        .unwrap_or(reset)
        .with_timezone(&Local)
        .format("%H:%M")
        .to_string()
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Io(_) => fl!("error-updater-io"),
            Self::Json(_) => fl!("error-updater-json"),
            Self::ManifestJson(_) => fl!("error-updater-manifest-json"),
            Self::ReleaseJson(_) => fl!("error-updater-release-json"),
            Self::PublicKey(_) => fl!("error-updater-public-key"),
            Self::SignatureLength(_) => fl!("error-updater-signature-length"),
            Self::Signature(_) => fl!("error-updater-signature"),
            Self::Http(_) => fl!("error-updater-http"),
            Self::ReadUpdate(_) => fl!("error-updater-read-update"),
            Self::ReadArchive(_) => fl!("error-updater-read-archive"),
            Self::WriteArchive(_) => fl!("error-updater-write-archive"),
            Self::CreateArchive(_) => fl!("error-updater-create-archive"),
            Self::SyncArchive(_) => fl!("error-updater-sync-archive"),
            Self::OpenArchive(_) => fl!("error-updater-open-archive"),
            Self::Cancelled => fl!("error-updater-cancelled"),
            Self::ValidationTimeout => fl!("error-updater-validation-timeout"),
            Self::HelperTimeout => fl!("error-updater-helper-timeout"),
            Self::ValidationEncoding(_) => fl!("error-updater-validation-encoding"),
            Self::EffectiveUid(_) => fl!("error-updater-effective-uid"),
            Self::UnsafeInstallation(_) => fl!("error-updater-unsafe-installation"),
            Self::LockContended => fl!("error-updater-lock-contended"),
            Self::ReplacementRollback { .. } => fl!("error-updater-replacement-rollback"),
            Self::ReplacementCleanup { .. } => fl!("error-updater-replacement-cleanup"),
            Self::RestartRecovery { .. } => fl!("error-updater-restart-recovery"),
            Self::RecordOutcome { .. } => fl!("error-updater-record-outcome"),
            Self::InvalidHex => fl!("error-updater-invalid-hex"),
            Self::UnsupportedTarget => fl!("error-updater-unsupported-target"),
            Self::ManifestSchema => fl!("error-updater-manifest-schema"),
            Self::ManifestAssetCount => fl!("error-updater-manifest-asset-count"),
            Self::ManifestAsset => fl!("error-updater-manifest-asset"),
            Self::ArchiveSize => fl!("error-updater-archive-size"),
            Self::ManifestBounds => fl!("error-updater-manifest-bounds"),
            Self::ManifestVersion => fl!("error-updater-manifest-version"),
            Self::UntrustedUrl => fl!("error-updater-untrusted-url"),
            Self::InvalidRedirect => fl!("error-updater-invalid-redirect"),
            Self::RedirectLimit => fl!("error-updater-redirect-limit"),
            Self::ResponseLimit => fl!("error-updater-response-limit"),
            Self::MetadataLimit => fl!("error-updater-metadata-limit"),
            Self::UnstableRelease => fl!("error-updater-unstable-release"),
            Self::NoPublishedRelease => fl!("error-updater-no-published-release"),
            Self::ReleaseAssetCount => fl!("error-updater-release-asset-count"),
            Self::ReleaseAsset => fl!("error-updater-release-asset"),
            Self::CurrentVersion => fl!("error-updater-current-version"),
            Self::ReleaseVersion => fl!("error-updater-release-version"),
            Self::MissingSignedMetadata => fl!("error-updater-missing-signed-metadata"),
            Self::MissingPackageMetadata => fl!("error-updater-missing-package-metadata"),
            Self::UnknownPackage => fl!("error-updater-unknown-package"),
            Self::PackageChanged => fl!("error-updater-package-changed"),
            Self::MissingPolicyKit => fl!("error-updater-missing-policy-kit"),
            Self::PackageCommand { .. } => fl!("error-updater-package-command"),
            Self::PackageAuthorization => fl!("error-updater-package-authorization"),
            Self::PackageElevation => fl!("error-updater-package-elevation"),
            Self::UnsafePackageHelper => fl!("error-updater-unsafe-package-helper"),
            Self::PackageVersion => fl!("error-updater-package-version"),
            Self::SignedMetadataSize => fl!("error-updater-signed-metadata-size"),
            Self::MissingPlatformAsset => fl!("error-updater-missing-platform-asset"),
            Self::ArchiveMetadata => fl!("error-updater-archive-metadata"),
            Self::ArchiveExceedsSize => fl!("error-updater-archive-exceeds-size"),
            Self::ArchiveDigest => fl!("error-updater-archive-digest"),
            Self::OfferAsset => fl!("error-updater-offer-asset"),
            Self::MissingOffer => fl!("error-updater-missing-offer"),
            Self::MissingPrepared => fl!("error-updater-missing-prepared"),
            Self::MissingValidationPipes => fl!("error-updater-missing-validation-pipes"),
            Self::ValidationOutputLimit => fl!("error-updater-validation-output-limit"),
            Self::RelativeInstallation => fl!("error-updater-relative-installation"),
            Self::NoncanonicalInstallation => fl!("error-updater-noncanonical-installation"),
            Self::SymlinkedInstallation => fl!("error-updater-symlinked-installation"),
            Self::UnsafeMacParent => fl!("error-updater-unsafe-mac-parent"),
            Self::PackageManaged => fl!("error-updater-package-managed"),
            Self::UnsafeBrew(_) => fl!("error-updater-unsafe-brew"),
            Self::BrewTimeout => fl!("error-updater-brew-timeout"),
            Self::BrewVersion => fl!("error-updater-brew-version"),
            Self::MissingCask => fl!("error-updater-missing-cask"),
            Self::RelaunchMissing(_) => fl!("error-updater-relaunch-missing"),
            Self::OutsideHome => fl!("error-updater-outside-home"),
            Self::MissingExecutableParent => fl!("error-updater-missing-executable-parent"),
            Self::NotStandalone => fl!("error-updater-not-standalone"),
            Self::LocalBuild => fl!("error-updater-local-build"),
            Self::RootUser => fl!("error-updater-root-user"),
            Self::UnsupportedPlatform => fl!("error-updater-unsupported-platform"),
            Self::NotHerdrBundle => fl!("error-updater-not-herdr-bundle"),
            Self::MissingHome => fl!("error-updater-missing-home"),
            Self::MissingInstallationParent => fl!("error-updater-missing-installation-parent"),
            Self::NotExecutable => fl!("error-updater-not-executable"),
            Self::MissingInstallationName => fl!("error-updater-missing-installation-name"),
            Self::UnsafeLock => fl!("error-updater-unsafe-lock"),
            Self::BundleVersion => fl!("error-updater-bundle-version"),
            Self::BundleIdentifier => fl!("error-updater-bundle-identifier"),
            Self::StaleRequest => fl!("error-updater-stale-request"),
            Self::ArchiveEntryLimit => fl!("error-updater-archive-entry-limit"),
            Self::ArchivePath => fl!("error-updater-archive-path"),
            Self::LinuxPayload => fl!("error-updater-linux-payload"),
            Self::OutsideBundle => fl!("error-updater-outside-bundle"),
            Self::ExpandedArchiveLimit => fl!("error-updater-expanded-archive-limit"),
            Self::MissingLinkTarget => fl!("error-updater-missing-link-target"),
            Self::ArchiveSymlink => fl!("error-updater-archive-symlink"),
            Self::DirectoryData => fl!("error-updater-directory-data"),
            Self::MissingArchiveParent => fl!("error-updater-missing-archive-parent"),
            Self::PayloadNotExecutable => fl!("error-updater-payload-not-executable"),
            Self::ArchiveEntryType => fl!("error-updater-archive-entry-type"),
            Self::TrailingArchiveData => fl!("error-updater-trailing-archive-data"),
            Self::ArchiveSymlinkParent => fl!("error-updater-archive-symlink-parent"),
            Self::MissingSymlinkParent => fl!("error-updater-missing-symlink-parent"),
            Self::SymlinkEscape => fl!("error-updater-symlink-escape"),
            Self::EmptyArchive => fl!("error-updater-empty-archive"),
            Self::BundleNotExecutable => fl!("error-updater-bundle-not-executable"),
            Self::SigningIdentityChanged => fl!("error-updater-signing-identity-changed"),
            Self::UnauthenticatedOffer => fl!("error-updater-unauthenticated-offer"),
            Self::RestartArgumentsLimit => fl!("error-updater-restart-arguments-limit"),
            Self::RequestLimit => fl!("error-updater-request-limit"),
            Self::MalformedRequest => fl!("error-updater-malformed-request"),
            Self::MissingControlPipe => fl!("error-updater-missing-control-pipe"),
            Self::HelperOwnerStopped => fl!("error-updater-helper-owner-stopped"),
            Self::MissingHelperOutput => fl!("error-updater-missing-helper-output"),
            Self::HelperNotReady => fl!("error-updater-helper-not-ready"),
            Self::RecoveryExists => fl!("error-updater-recovery-exists"),
            Self::MissingDestinationParent => fl!("error-updater-missing-destination-parent"),
            Self::RecoveryChanged => fl!("error-updater-recovery-changed"),
            Self::StageLocation => fl!("error-updater-stage-location"),
            Self::StagePermissions => fl!("error-updater-stage-permissions"),
            Self::CommitBarrier(_) => fl!("error-updater-commit-barrier"),
            Self::NotCommitted => fl!("error-updater-not-committed"),
            Self::InstallationChanged => fl!("error-updater-installation-changed"),
            Self::MissingInstallerMetadata => fl!("error-updater-missing-installer-metadata"),
            Self::HttpStatus(status) => {
                fl!("error-updater-http-status", status = status.to_string())
            }
            Self::RateLimited { reset: Some(reset) } => fl!(
                "error-updater-rate-limited-until",
                time = retry_time(*reset)
            ),
            Self::RateLimited { reset: None } => fl!("error-updater-rate-limited"),
            Self::ValidationFailed(status) => {
                fl!(
                    "error-updater-validation-failed",
                    status = status.to_string()
                )
            }
            Self::MissingSignatureField(field) => {
                fl!("error-updater-missing-signature-field", field = *field)
            }
            Self::PackageFailed { status, detail } => fl!(
                "error-updater-package-failed",
                status = status.to_string(),
                detail = detail.as_str()
            ),
            Self::BrewFailed { status, detail } => fl!(
                "error-updater-brew-failed",
                status = status.to_string(),
                detail = detail.as_str()
            ),
            Self::BrewStale {
                installed,
                current,
                expected,
            } => fl!(
                "error-updater-brew-stale",
                installed = installed.as_str(),
                current = current.as_str(),
                expected = expected.as_str()
            ),
            Self::RelaunchFailed(status) => {
                fl!("error-updater-relaunch-failed", status = status.to_string())
            }
            // The Windows installer's enum is private to the updater, so its
            // English `Display` shows until the crate exports it.
            #[cfg(windows)]
            Self::Windows(error) => error.to_string(),
        }
    }
}
