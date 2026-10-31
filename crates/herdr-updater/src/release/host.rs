//! This release's helper programs built for other machines. Herdr UI copies
//! `herdr-remote-control` onto an SSH or cloud machine of another platform
//! to pair a phone with it. The builds are static (musl), so they run on
//! any glibc. They are listed in their own signed manifest (schema 4), so
//! the updater's families never meet them, and checked against it before
//! they leave this computer.

use super::*;

/// The platforms a release publishes host builds for.
pub const HOST_TARGETS: [&str; 2] = ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl"];
pub(super) const SCHEMA: u32 = 4;
pub(super) const MANIFEST: (&str, &str) = ("host-manifest.json", "host-manifest.sig");

/// The asset name of `herdr-remote-control` built for `target`: the bare
/// executable.
pub(super) fn name(version: &str, target: &str) -> Result<String> {
    if !HOST_TARGETS.contains(&target) {
        return Err(Error::UnsupportedTarget);
    }
    Ok(format!("herdr-remote-control-{version}-{target}"))
}

/// The signed listing of `target`'s build in release `version`. Only
/// transport is injected, so tests run the production checks.
pub(super) fn host_asset_with(
    version: &str,
    key_hex: &str,
    target: &str,
    mut fetch: impl FnMut(&str, usize) -> Result<Vec<u8>>,
) -> Result<Asset> {
    let name = name(version, target)?;
    let bytes = fetch(&release_url(version, MANIFEST.0), MANIFEST_LIMIT)?;
    let signature = fetch(&release_url(version, MANIFEST.1), 64)?;
    let manifest = verify_manifest(&bytes, &signature, key_hex, version)?;
    // A manifest signed for another family must not answer for this one.
    if manifest.schema != SCHEMA {
        return Err(Error::ManifestSchema);
    }
    manifest
        .assets
        .into_iter()
        .find(|asset| asset.target == target && asset.name == name)
        .ok_or(Error::MissingPlatformAsset)
}

/// The release this build is and the key its manifests are signed with;
/// a local build has neither and downloads nothing.
pub(super) fn release_identity<'a>(
    version: &'a str,
    key: Option<&'a str>,
) -> Result<(&'a str, &'a str)> {
    let key = key.ok_or(Error::LocalBuild)?;
    parse_version(version).ok_or(Error::LocalBuild)?;
    Ok((version, key))
}

/// Downloads this release's `herdr-remote-control` built for `target` into
/// `destination`, which must not exist yet and is created private. The
/// bytes are checked against the release's signed host manifest, by the
/// key this build embeds; a local build has neither and refuses. Blocking:
/// run it on a worker thread.
pub fn download_host_build(target: &str, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    let (version, key) =
        release_identity(crate::APP_VERSION, option_env!("HERDR_UPDATE_PUBLIC_KEY"))?;
    let asset = host_asset_with(version, key, target, |url, limit| {
        read_bounded(
            request(url, RequestProfile::Metadata, cancel)?
                .into_body()
                .into_reader(),
            limit,
            cancel,
        )
    })?;
    let response = request(
        &release_url(version, &asset.name),
        RequestProfile::Archive,
        cancel,
    )?;
    let mut file = create_archive(destination)?;
    copy_archive(
        response.into_body().into_reader(),
        &mut file,
        &asset,
        cancel,
        |_, _| {},
    )?;
    file.sync_all().map_err(Error::SyncArchive)?;
    cancelled(cancel)
}
