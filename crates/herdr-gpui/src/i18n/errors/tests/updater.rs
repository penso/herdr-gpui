use super::*;
use chrono::{TimeDelta, Utc};
use herdr_updater::Error;
use std::{io, path::PathBuf, process::ExitStatus};

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

#[cfg(unix)]
fn status() -> ExitStatus {
    std::os::unix::process::ExitStatusExt::from_raw(256)
}

#[cfg(windows)]
fn status() -> ExitStatus {
    std::os::windows::process::ExitStatusExt::from_raw(1)
}

#[test]
fn every_variant_without_fields_has_a_message() {
    assert_localized(&[
        Error::Cancelled,
        Error::ValidationTimeout,
        Error::HelperTimeout,
        Error::LockContended,
        Error::InvalidHex,
        Error::UnsupportedTarget,
        Error::ManifestSchema,
        Error::ManifestAssetCount,
        Error::ManifestAsset,
        Error::ArchiveSize,
        Error::ManifestBounds,
        Error::ManifestVersion,
        Error::UntrustedUrl,
        Error::InvalidRedirect,
        Error::RedirectLimit,
        Error::ResponseLimit,
        Error::MetadataLimit,
        Error::UnstableRelease,
        Error::NoPublishedRelease,
        Error::ReleaseAssetCount,
        Error::ReleaseAsset,
        Error::CurrentVersion,
        Error::ReleaseVersion,
        Error::MissingSignedMetadata,
        Error::MissingPackageMetadata,
        Error::UnknownPackage,
        Error::PackageChanged,
        Error::MissingPolicyKit,
        Error::PackageAuthorization,
        Error::PackageElevation,
        Error::UnsafePackageHelper,
        Error::PackageVersion,
        Error::SignedMetadataSize,
        Error::MissingPlatformAsset,
        Error::ArchiveMetadata,
        Error::ArchiveExceedsSize,
        Error::ArchiveDigest,
        Error::OfferAsset,
        Error::MissingOffer,
        Error::MissingPrepared,
        Error::MissingValidationPipes,
        Error::ValidationOutputLimit,
        Error::RelativeInstallation,
        Error::NoncanonicalInstallation,
        Error::SymlinkedInstallation,
        Error::UnsafeMacParent,
        Error::PackageManaged,
        Error::BrewTimeout,
        Error::BrewVersion,
        Error::MissingCask,
        Error::OutsideHome,
        Error::MissingExecutableParent,
        Error::NotStandalone,
        Error::LocalBuild,
        Error::RootUser,
        Error::UnsupportedPlatform,
        Error::NotHerdrBundle,
        Error::MissingHome,
        Error::MissingInstallationParent,
        Error::NotExecutable,
        Error::MissingInstallationName,
        Error::UnsafeLock,
        Error::BundleVersion,
        Error::BundleIdentifier,
        Error::StaleRequest,
        Error::ArchiveEntryLimit,
        Error::ArchivePath,
        Error::LinuxPayload,
        Error::OutsideBundle,
        Error::ExpandedArchiveLimit,
        Error::MissingLinkTarget,
        Error::ArchiveSymlink,
        Error::DirectoryData,
        Error::MissingArchiveParent,
        Error::PayloadNotExecutable,
        Error::ArchiveEntryType,
        Error::TrailingArchiveData,
        Error::ArchiveSymlinkParent,
        Error::MissingSymlinkParent,
        Error::SymlinkEscape,
        Error::EmptyArchive,
        Error::BundleNotExecutable,
        Error::SigningIdentityChanged,
        Error::UnauthenticatedOffer,
        Error::RestartArgumentsLimit,
        Error::RequestLimit,
        Error::MalformedRequest,
        Error::MissingControlPipe,
        Error::HelperOwnerStopped,
        Error::MissingHelperOutput,
        Error::HelperNotReady,
        Error::RecoveryExists,
        Error::MissingDestinationParent,
        Error::RecoveryChanged,
        Error::StageLocation,
        Error::StagePermissions,
        Error::NotCommitted,
        Error::InstallationChanged,
        Error::MissingInstallerMetadata,
        Error::WorkerStopped,
        Error::CancelUnacknowledged,
    ]);
}

/// `PublicKey`, `SignatureLength`, and `Signature` hold an
/// `ed25519_dalek::SignatureError`, which this crate cannot name, and the
/// Windows installer's `Windows` variant wraps an enum private to the
/// updater, so neither is built here.
#[test]
fn every_variant_with_fields_has_a_message() {
    let backup = PathBuf::from("/tmp/Herdr.app.previous");
    assert_localized(&[
        Error::Io(io()),
        Error::Json(json()),
        Error::ManifestJson(json()),
        Error::ReleaseJson(json()),
        Error::Http(Box::new(ureq::Error::BadUri("x".into()))),
        Error::HttpStatus(404),
        Error::RateLimited { reset: None },
        Error::RateLimited {
            reset: Some(Utc::now() + TimeDelta::hours(1)),
        },
        Error::ReadUpdate(io()),
        Error::ReadArchive(io()),
        Error::WriteArchive(io()),
        Error::CreateArchive(io()),
        Error::SyncArchive(io()),
        Error::OpenArchive(io()),
        Error::ValidationFailed(status()),
        Error::ValidationEncoding(String::from_utf8(vec![0xff]).unwrap_err()),
        Error::EffectiveUid("x".parse::<u32>().unwrap_err()),
        Error::UnsafeInstallation(backup.clone()),
        Error::MissingSignatureField("TeamIdentifier="),
        Error::ReplacementRollback {
            source: io(),
            rollback: io(),
            backup: backup.clone(),
        },
        Error::ReplacementCleanup {
            source: io(),
            cleanup: Box::new(Error::Cancelled),
            backup: backup.clone(),
        },
        Error::RestartRecovery {
            source: Box::new(Error::Cancelled),
            recovery: io(),
            backup: backup.clone(),
        },
        Error::RecordOutcome {
            source: Box::new(Error::Cancelled),
            record: Box::new(Error::NotCommitted),
        },
        Error::PackageCommand {
            program: "/usr/bin/pkexec".into(),
            source: io(),
        },
        Error::PackageFailed {
            status: status(),
            detail: "dpkg: error processing archive".into(),
        },
        Error::UnsafeBrew(backup.clone()),
        Error::BrewFailed {
            status: status(),
            detail: "Error: Cask 'herdr-gpui' is unavailable".into(),
        },
        Error::BrewStale {
            installed: "20260921.2".into(),
            current: "20260921.1".into(),
            expected: "20260921.3".into(),
        },
        Error::RelaunchFailed(status()),
        Error::RelaunchMissing(backup),
        Error::CommitBarrier(std::sync::mpsc::RecvTimeoutError::Timeout),
        Error::Start(io()),
    ]);
}

#[test]
fn every_reason_app_updates_never_checks_has_a_message() {
    use herdr_updater::Disabled;
    assert_localized(&[
        Disabled::LocalBuild,
        Disabled::Platform,
        Disabled::Secondary,
        Disabled::Unsupported(Error::WorkerStopped.into()),
    ]);
}

#[cfg(windows)]
#[test]
fn every_windows_installer_variant_has_a_message() {
    use herdr_updater::windows::Error;
    assert_localized(&[
        Error::InstallerPath,
        Error::Launch(io()),
        Error::LocalAppData,
        Error::Location,
        Error::MissingPublisher,
        Error::MissingSigner,
        Error::Signer {
            found: "CN=Someone Else".into(),
        },
        Error::Untrusted(-2_146_762_496),
    ]);
}
