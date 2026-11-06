use super::*;

#[test]
fn package_manifest_authenticates_all_formats_and_architectures() -> anyhow::Result<()> {
    let mut manifest = fixture();
    manifest.schema = 2;
    manifest.assets.clear();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        for format in [Format::Deb, Format::Rpm, Format::Arch] {
            manifest.assets.push(Asset {
                target: target.into(),
                name: format.name(&manifest.version, target)?,
                size: 3,
                sha256: hex(&Sha256::digest(b"abc")),
            });
        }
    }
    assert_eq!(signed(&serde_json::to_vec(&manifest)?)?, manifest);
    let mut duplicate = manifest.clone();
    duplicate.assets[1] = duplicate.assets[0].clone();
    assert!(matches!(
        signed(&serde_json::to_vec(&duplicate)?),
        Err(Error::ManifestAsset)
    ));
    for name in [
        "../../payload.deb",
        "Herdr-20260920.1-x86_64-unknown-linux-gnu.deb",
        "Herdr-20260920.2-x86_64-unknown-linux-gnu.zip",
    ] {
        let mut invalid = manifest.clone();
        invalid.assets[0].name = name.into();
        assert!(matches!(
            signed(&serde_json::to_vec(&invalid)?),
            Err(Error::ManifestAsset)
        ));
    }
    manifest.assets[0].target = "universal-apple-darwin".into();
    assert!(matches!(
        signed(&serde_json::to_vec(&manifest)?),
        Err(Error::ManifestAsset)
    ));
    Ok(())
}

#[test]
fn package_and_standalone_manifests_remain_separate() -> anyhow::Result<()> {
    let mut manifest = fixture();
    manifest.schema = 2;
    assert!(matches!(
        signed(&serde_json::to_vec(&manifest)?),
        Err(Error::ManifestAsset)
    ));
    manifest.schema = 1;
    manifest.assets[0].target = "x86_64-unknown-linux-gnu".into();
    manifest.assets[0].name = Format::Deb.name(&manifest.version, &manifest.assets[0].target)?;
    assert!(matches!(
        signed(&serde_json::to_vec(&manifest)?),
        Err(Error::ManifestAsset)
    ));
    assert_eq!(
        Format::Archive.manifest(),
        ("update-manifest.json", "update-manifest.sig")
    );
    assert_eq!(
        Format::Deb.manifest(),
        ("package-manifest.json", "package-manifest.sig")
    );
    Ok(())
}
