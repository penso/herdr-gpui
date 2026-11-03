//! Host builds: `herdr-remote-control` for other machines, listed in their
//! own signed manifest (schema 4) and read only through it.

use super::*;

const VERSION: &str = "20260920.2";

fn host_manifest() -> Manifest {
    Manifest {
        schema: host::SCHEMA,
        version: VERSION.into(),
        assets: HOST_TARGETS
            .iter()
            .map(|target| Asset {
                target: (*target).into(),
                name: host::name(VERSION, target).unwrap_or_default(),
                size: 3,
                sha256: hex(&Sha256::digest(b"abc")),
            })
            .collect(),
    }
}

/// The asset `target` resolves to, served `manifest` signed by `signer`
/// and checked with key 42's public half.
fn resolve(manifest: &Manifest, signer: u8, target: &str) -> Result<Asset> {
    let bytes = serde_json::to_vec(manifest).map_err(Error::ManifestJson)?;
    let key = SigningKey::from_bytes(&[42; 32]);
    let signature = SigningKey::from_bytes(&[signer; 32])
        .sign(&bytes)
        .to_bytes();
    host::host_asset_with(
        VERSION,
        &hex(key.verifying_key().as_bytes()),
        target,
        |url, _| {
            if url == release_url(VERSION, host::MANIFEST.0) {
                Ok(bytes.clone())
            } else if url == release_url(VERSION, host::MANIFEST.1) {
                Ok(signature.to_vec())
            } else {
                Err(Error::UntrustedUrl)
            }
        },
    )
}

#[test]
fn a_host_build_resolves_through_its_signed_manifest() -> anyhow::Result<()> {
    for target in HOST_TARGETS {
        let asset = resolve(&host_manifest(), 42, target)?;
        assert_eq!(
            asset.name,
            format!("herdr-remote-control-{VERSION}-{target}")
        );
        assert_eq!(asset.size, 3);
    }
    Ok(())
}

#[test]
fn a_host_build_is_refused_without_the_right_signature_family_or_target() {
    assert!(matches!(
        resolve(&host_manifest(), 7, HOST_TARGETS[0]),
        Err(Error::Signature(_))
    ));
    assert!(matches!(
        resolve(&host_manifest(), 42, "x86_64-unknown-linux-gnu"),
        Err(Error::UnsupportedTarget)
    ));
    // Another family's manifest, signed by the same key, answers for no
    // host build.
    let mut archive = host_manifest();
    archive.schema = 1;
    assert!(resolve(&archive, 42, HOST_TARGETS[0]).is_err());
    // A name that does not follow the contract is refused when the manifest
    // is read.
    let mut renamed = host_manifest();
    renamed.assets[0].name = "herdr-gpui".into();
    assert!(matches!(
        resolve(&renamed, 42, HOST_TARGETS[0]),
        Err(Error::ManifestAsset)
    ));
    // A target the manifest leaves out has no build.
    let mut one = host_manifest();
    one.assets.truncate(1);
    assert!(matches!(
        resolve(&one, 42, HOST_TARGETS[1]),
        Err(Error::MissingPlatformAsset)
    ));
}

#[test]
fn updater_families_never_accept_the_host_manifest() -> anyhow::Result<()> {
    for format in [Format::Archive, Format::Deb, Format::Installer] {
        assert_ne!(format.schema(), host::SCHEMA);
        assert_ne!(format.manifest().0, host::MANIFEST.0);
    }
    Ok(())
}

#[test]
fn a_local_build_downloads_no_host_build() {
    let key = "00".repeat(32);
    assert!(matches!(
        host::release_identity("dev", Some(&key)),
        Err(Error::LocalBuild)
    ));
    assert!(matches!(
        host::release_identity(VERSION, None),
        Err(Error::LocalBuild)
    ));
    assert_eq!(
        host::release_identity(VERSION, Some(&key)).ok(),
        Some((VERSION, key.as_str()))
    );
}
