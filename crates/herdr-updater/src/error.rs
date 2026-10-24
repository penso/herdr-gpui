//! Typed updater failures. Remote diagnostics are sources, never display text.
use chrono::{DateTime, DurationRound as _, Local, TimeDelta, Utc};
use std::{io, path::PathBuf, process::ExitStatus};

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The first whole minute at or after `reset`, so retrying at the shown minute
/// is never still early, even by a fraction of a second.
pub(super) fn retry_minute(reset: DateTime<Utc>) -> DateTime<Utc> {
    // Rounding fails only for times chrono cannot represent a minute later.
    reset
        .duration_round_up(TimeDelta::minutes(1))
        .unwrap_or(reset)
}

/// When to try again, in local time, or `Retry later` without a reset time.
pub fn retry_hint(reset: Option<DateTime<Utc>>) -> String {
    match reset {
        Some(reset) => format!(
            "Retry after {}",
            retry_minute(reset).with_timezone(&Local).format("%H:%M")
        ),
        None => "Retry later".to_owned(),
    }
}

/// Whether a 403 is an exhausted quota (primary or secondary) rather than a
/// permission refusal: GitHub then sends `x-ratelimit-remaining: 0` or a
/// `retry-after` delay. Lives here so the updater-only test build, which
/// compiles no other app module, still has it.
pub fn quota_exhausted(headers: &ureq::http::HeaderMap) -> bool {
    header_number(headers, "x-ratelimit-remaining") == Some(0)
        || headers.contains_key("retry-after")
}

/// When GitHub's quota refills: a `retry-after` delay in seconds, else the
/// `x-ratelimit-reset` epoch. `None` without either, or once it has passed.
pub fn rate_limit_reset(
    headers: &ureq::http::HeaderMap,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    header_number(headers, "retry-after")
        .and_then(|seconds| now.checked_add_signed(TimeDelta::try_seconds(seconds)?))
        .or_else(|| {
            header_number(headers, "x-ratelimit-reset")
                .and_then(|epoch| DateTime::from_timestamp(epoch, 0))
        })
        .filter(|reset| *reset > now)
}

fn header_number(headers: &ureq::http::HeaderMap, name: &str) -> Option<i64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Update filesystem or pipe I/O failed")]
    Io(#[from] io::Error),
    #[error("Invalid update request JSON")]
    Json(#[from] serde_json::Error),
    #[error("Invalid update manifest")]
    ManifestJson(#[source] serde_json::Error),
    #[error("Invalid GitHub release metadata")]
    ReleaseJson(#[source] serde_json::Error),
    #[error("Invalid update public key")]
    PublicKey(#[source] ed25519_dalek::SignatureError),
    #[error("Invalid signature length")]
    SignatureLength(#[source] ed25519_dalek::SignatureError),
    #[error("Update manifest signature verification failed")]
    Signature(#[source] ed25519_dalek::SignatureError),
    #[error("Update HTTPS request failed")]
    Http(#[source] Box<ureq::Error>),
    #[error("Update request returned HTTP {0}")]
    HttpStatus(u16),
    /// GitHub refused the request because this address used up its API quota.
    #[error("GitHub rate limit reached. {}", retry_hint(*reset))]
    RateLimited { reset: Option<DateTime<Utc>> },
    #[error("Reading update failed")]
    ReadUpdate(#[source] io::Error),
    #[error("Reading archive failed")]
    ReadArchive(#[source] io::Error),
    #[error("Writing archive failed")]
    WriteArchive(#[source] io::Error),
    #[error("Creating update archive failed")]
    CreateArchive(#[source] io::Error),
    #[error("Syncing update archive failed")]
    SyncArchive(#[source] io::Error),
    #[error("Opening update archive failed")]
    OpenArchive(#[source] io::Error),
    #[error("Update cancelled")]
    Cancelled,
    #[error("Update validation timed out")]
    ValidationTimeout,
    #[error("Update helper timed out")]
    HelperTimeout,
    #[error("Update validation command failed ({0})")]
    ValidationFailed(ExitStatus),
    #[error("Non-UTF-8 validation output")]
    ValidationEncoding(#[source] std::string::FromUtf8Error),
    #[error("Cannot determine effective UID")]
    EffectiveUid(#[source] std::num::ParseIntError),
    #[error("Unsafe or non-user-owned installation path")]
    UnsafeInstallation(PathBuf),
    #[error("Another updater owns this installation")]
    LockContended,
    #[error("Missing code signature {0}")]
    MissingSignatureField(&'static str),
    #[error("Replacement failed; rollback failed; recovery was retained")]
    ReplacementRollback {
        #[source]
        source: io::Error,
        rollback: io::Error,
        backup: PathBuf,
    },
    #[error("Replacement failed; backup cleanup failed; recovery was retained")]
    ReplacementCleanup {
        #[source]
        source: io::Error,
        cleanup: Box<Error>,
        backup: PathBuf,
    },
    #[error("Restart failed; recovery was retained")]
    RestartRecovery {
        #[source]
        source: Box<Error>,
        recovery: io::Error,
        backup: PathBuf,
    },
    #[error("Update failed; could not record recovery outcome")]
    RecordOutcome {
        #[source]
        source: Box<Error>,
        record: Box<Error>,
    },
    #[error("Expected 64 lowercase hexadecimal characters")]
    InvalidHex,
    #[error("Unsupported update target")]
    UnsupportedTarget,
    #[error("Invalid update manifest schema or version")]
    ManifestSchema,
    #[error("Invalid update manifest asset count")]
    ManifestAssetCount,
    #[error("Invalid update asset name or duplicate target")]
    ManifestAsset,
    #[error("Update archive size is outside permitted bounds")]
    ArchiveSize,
    #[error("Invalid manifest size or expected version")]
    ManifestBounds,
    #[error("Update manifest version does not match release")]
    ManifestVersion,
    #[error("Update URL is outside the permitted HTTPS hosts")]
    UntrustedUrl,
    #[error("Missing or invalid update redirect")]
    InvalidRedirect,
    #[error("Too many update redirects")]
    RedirectLimit,
    #[error("Update response exceeds size limit")]
    ResponseLimit,
    #[error("Release metadata exceeds size limit")]
    MetadataLimit,
    #[error("GitHub release is not a stable vYYYYMMDD.COUNTER release")]
    UnstableRelease,
    #[error("GitHub lists no published vYYYYMMDD.COUNTER release")]
    NoPublishedRelease,
    #[error("GitHub release has too many assets")]
    ReleaseAssetCount,
    #[error("Invalid GitHub release asset metadata")]
    ReleaseAsset,
    #[error("Current build has no valid release version")]
    CurrentVersion,
    #[error("Invalid release version")]
    ReleaseVersion,
    #[error("Release is missing signed update metadata")]
    MissingSignedMetadata,
    #[error(
        "This release has no signed Linux package metadata. Install it from Manual Releases, or check for a newer release"
    )]
    MissingPackageMetadata,
    #[error(
        "No supported package manager owns this executable. Install a release .deb, .rpm, or Arch package, or use a standalone copy under HOME"
    )]
    UnknownPackage,
    #[error(
        "The installed package changed while the update was being prepared. Check for updates again"
    )]
    PackageChanged,
    #[error(
        "Installing packages requires /usr/bin/pkexec and a running desktop PolicyKit authentication agent"
    )]
    MissingPolicyKit,
    #[error("Could not start the package installer")]
    PackageCommand {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Package installation failed ({status}): {detail}")]
    PackageFailed { status: ExitStatus, detail: String },
    #[error(
        "Package installation was not authorized. Retry and approve the system authentication prompt"
    )]
    PackageAuthorization,
    #[error("The package helper must be authorized through PolicyKit for a non-root user")]
    PackageElevation,
    #[error(
        "Package updates require a trusted root-owned /usr/bin/herdr-gpui and root-owned installer storage"
    )]
    UnsafePackageHelper,
    #[error("The package manager finished, but the installed version does not match the update")]
    PackageVersion,
    #[error("Invalid signed metadata size")]
    SignedMetadataSize,
    #[error("Release has no update for this platform")]
    MissingPlatformAsset,
    #[error("Signed archive does not match GitHub release metadata")]
    ArchiveMetadata,
    #[error("Update archive exceeds signed size")]
    ArchiveExceedsSize,
    #[error("Update archive size or SHA-256 mismatch")]
    ArchiveDigest,
    #[error("Update offer asset does not match manifest or platform")]
    OfferAsset,
    #[error("Check for a release before downloading")]
    MissingOffer,
    #[error("Download and verify an update before installing")]
    MissingPrepared,
    #[error("Missing validation output pipes")]
    MissingValidationPipes,
    #[error("Update validation output exceeded limit")]
    ValidationOutputLimit,
    #[error("Installation path must be absolute")]
    RelativeInstallation,
    #[error("Noncanonical installation path")]
    NoncanonicalInstallation,
    #[error("Symlinked installation path")]
    SymlinkedInstallation,
    #[error("macOS installation parent must be user/root-owned and not world-writable")]
    UnsafeMacParent,
    #[error("Package-managed installations cannot self-update")]
    PackageManaged,
    #[error("Unsafe or non-user-owned Homebrew installation")]
    UnsafeBrew(PathBuf),
    #[error("Homebrew command failed ({status}): {detail}")]
    BrewFailed { status: ExitStatus, detail: String },
    #[error("Homebrew did not finish in time")]
    BrewTimeout,
    #[error("Homebrew did not report an installed cask version")]
    BrewVersion,
    #[error(
        "After refreshing Homebrew, the installed cask is {installed}; it must be newer than {current} and at least the expected release {expected}"
    )]
    BrewStale {
        installed: String,
        current: String,
        expected: String,
    },
    #[error("Homebrew is no longer managing this installation")]
    MissingCask,
    #[error("Could not arm the Dock relaunch ({0})")]
    RelaunchFailed(ExitStatus),
    #[error("Homebrew left no runnable app to restart")]
    RelaunchMissing(PathBuf),
    #[error("Standalone Linux updates require installation under HOME")]
    OutsideHome,
    #[error("No executable parent")]
    MissingExecutableParent,
    #[error("Installation must be an ordinary standalone executable")]
    NotStandalone,
    #[error("Updates are disabled for local builds")]
    LocalBuild,
    #[error("Updating as root is not supported")]
    RootUser,
    #[error("Unsupported installation platform")]
    UnsupportedPlatform,
    #[error("Not an installed Herdr.app")]
    NotHerdrBundle,
    #[error("HOME is not set")]
    MissingHome,
    #[error("No installation parent")]
    MissingInstallationParent,
    #[error("Installed file is not executable")]
    NotExecutable,
    #[error("No installation name")]
    MissingInstallationName,
    #[error("Unsafe update lock")]
    UnsafeLock,
    #[error("Bundle version differs from signed manifest")]
    BundleVersion,
    #[error("Installed or candidate bundle is not so.pen.herdr-gpui")]
    BundleIdentifier,
    #[error("Update request is stale or not a newer release")]
    StaleRequest,
    #[error("Too many archive entries")]
    ArchiveEntryLimit,
    #[error("Unsafe or duplicate archive path")]
    ArchivePath,
    #[error("Linux archive must contain exactly the release executable")]
    LinuxPayload,
    #[error("Archive is outside Herdr.app")]
    OutsideBundle,
    #[error("Expanded archive exceeds limit")]
    ExpandedArchiveLimit,
    #[error("Missing link target")]
    MissingLinkTarget,
    #[error("Unsafe archive symlink")]
    ArchiveSymlink,
    #[error("Directory contains data")]
    DirectoryData,
    #[error("Missing archive parent")]
    MissingArchiveParent,
    #[error("Release file is not executable")]
    PayloadNotExecutable,
    #[error("Unsupported archive entry type")]
    ArchiveEntryType,
    #[error("Trailing archive data")]
    TrailingArchiveData,
    #[error("Archive writes beneath a symlink")]
    ArchiveSymlinkParent,
    #[error("Missing symlink parent")]
    MissingSymlinkParent,
    #[error("Symlink chain escapes bundle")]
    SymlinkEscape,
    #[error("Empty update archive")]
    EmptyArchive,
    #[error("Bundle executable is not executable")]
    BundleNotExecutable,
    #[error("Update code signing identity changed")]
    SigningIdentityChanged,
    #[error("Offer differs from authenticated manifest")]
    UnauthenticatedOffer,
    #[error("Restart arguments exceed limit")]
    RestartArgumentsLimit,
    #[error("Oversized update request")]
    RequestLimit,
    #[error("Malformed update request")]
    MalformedRequest,
    #[error("Missing helper control pipe")]
    MissingControlPipe,
    #[error("Update helper owner stopped")]
    HelperOwnerStopped,
    #[error("Missing helper stdout")]
    MissingHelperOutput,
    #[error("Update helper did not become ready")]
    HelperNotReady,
    #[error("Recovery path already exists")]
    RecoveryExists,
    #[error("No destination parent")]
    MissingDestinationParent,
    #[error("Installation or backup changed; recovery was retained")]
    RecoveryChanged,
    #[error("Stage is not adjacent to installation")]
    StageLocation,
    #[error("Stage is not private")]
    StagePermissions,
    #[error("Update commit barrier timed out or disconnected")]
    CommitBarrier(#[source] std::sync::mpsc::RecvTimeoutError),
    #[error("Update was not committed")]
    NotCommitted,
    #[error("Installation changed during update")]
    InstallationChanged,
    #[error(
        "This release has no signed Windows installer metadata. Download it from GitHub, or check for a newer release"
    )]
    MissingInstallerMetadata,
    #[cfg(windows)]
    #[error(transparent)]
    Windows(#[from] super::windows::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context as _;
    use std::error::Error as _;

    #[test]
    fn stale_homebrew_diagnostic_describes_the_post_refresh_requirement() {
        let error = Error::BrewStale {
            installed: "20260921.2".into(),
            current: "20260921.1".into(),
            expected: "20260921.3".into(),
        };
        assert_eq!(
            error.to_string(),
            "After refreshing Homebrew, the installed cask is 20260921.2; it must be newer than 20260921.1 and at least the expected release 20260921.3"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn sources_remain_typed_without_displaying_remote_diagnostics() -> anyhow::Result<()> {
        let private =
            "https://release-assets.githubusercontent.com/a?signature=secret\n".repeat(4096);
        let http = Error::Http(Box::new(ureq::Error::BadUri(private.clone())));
        assert_eq!(http.to_string(), "Update HTTPS request failed");
        assert!(
            matches!(http.source().context("expected HTTP source")?.downcast_ref::<Box<ureq::Error>>().map(Box::as_ref), Some(ureq::Error::BadUri(uri)) if uri == &private)
        );

        let io = Error::ReadArchive(io::Error::new(io::ErrorKind::PermissionDenied, private));
        assert_eq!(io.to_string(), "Reading archive failed");
        assert_eq!(
            io.source()
                .context("expected archive read source")?
                .downcast_ref::<io::Error>()
                .context("expected I/O source")?
                .kind(),
            io::ErrorKind::PermissionDenied
        );

        let json = serde_json::from_str::<u32>("\"signature=secret\"")
            .err()
            .context("expected JSON type error")?;
        let error = Error::ManifestJson(json);
        assert_eq!(error.to_string(), "Invalid update manifest");
        assert!(
            error
                .source()
                .context("expected manifest JSON source")?
                .is::<serde_json::Error>()
        );
        Ok(())
    }

    #[test]
    fn recovery_retains_both_failures_and_path() -> anyhow::Result<()> {
        let error = Error::RestartRecovery {
            source: Box::new(Error::Io(io::Error::from(io::ErrorKind::NotFound))),
            recovery: io::Error::from(io::ErrorKind::PermissionDenied),
            backup: PathBuf::from("private/previous-installation"),
        };
        assert_eq!(
            error
                .source()
                .context("expected restart recovery source")?
                .source()
                .context("expected underlying restart source")?
                .downcast_ref::<io::Error>()
                .context("expected I/O source")?
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(
            matches!(error, Error::RestartRecovery { recovery, backup, .. }
            if recovery.kind() == io::ErrorKind::PermissionDenied && backup == std::path::Path::new("private/previous-installation"))
        );
        assert!(!matches!(Error::HelperTimeout, Error::Cancelled));
        assert!(!matches!(Error::ValidationTimeout, Error::Cancelled));
        assert!(!matches!(Error::NotCommitted, Error::Cancelled));
        Ok(())
    }
}
