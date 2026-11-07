use super::*;

struct Packages {
    metadata: serde_json::Value,
    manifest: Vec<u8>,
    signature: Vec<u8>,
    public: String,
}

impl Packages {
    fn new() -> anyhow::Result<Self> {
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
        let bytes = serde_json::to_vec(&manifest)?;
        let key = SigningKey::from_bytes(&[42; 32]);
        let metadata_asset = |name: &str, size: u64| {
            serde_json::json!({
                "name": name, "size": size, "browser_download_url": release_url(&manifest.version, name)
            })
        };
        let assets: Vec<_> = manifest
            .assets
            .iter()
            .map(|asset| metadata_asset(&asset.name, asset.size))
            .chain([
                metadata_asset("package-manifest.json", bytes.len() as u64),
                metadata_asset("package-manifest.sig", 64),
                // Legacy metadata is present too: it must never be a fallback.
                metadata_asset("update-manifest.json", 1),
                metadata_asset("update-manifest.sig", 64),
            ])
            .collect();
        Ok(Self {
            metadata: serde_json::json!({"tag_name": "v20260920.2", "draft": false, "prerelease": false, "assets": assets, "body": "Package release"}),
            signature: key.sign(&bytes).to_bytes().to_vec(),
            manifest: bytes,
            public: hex(key.verifying_key().as_bytes()),
        })
    }

    fn check(&self, format: Format, target: &str) -> Result<(Option<Offer>, Vec<String>)> {
        let mut requests = Vec::new();
        let offer = check_with(
            "20260920.1",
            &self.public,
            UpdateChannel::Stable,
            format,
            target,
            &AtomicBool::new(false),
            |url, limit| {
                requests.push(url.to_owned());
                let bytes = if url == LATEST_URL {
                    serde_json::to_vec(&self.metadata)?
                } else if url == release_url("20260920.2", "package-manifest.json") {
                    self.manifest.clone()
                } else if url == release_url("20260920.2", "package-manifest.sig") {
                    self.signature.clone()
                } else if url == HISTORY_URL {
                    // No grouped history in this fixture, so the offer keeps the
                    // target release's own notes.
                    b"[]".to_vec()
                } else {
                    panic!("unexpected fetch or archive fallback: {url}")
                };
                assert!(bytes.len() <= limit);
                Ok(bytes)
            },
        )?;
        Ok((offer, requests))
    }
}

#[test]
fn checks_fetch_package_metadata_and_select_exact_format_and_architecture() -> anyhow::Result<()> {
    let fixture = Packages::new()?;
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        for format in [Format::Deb, Format::Rpm, Format::Arch] {
            let (offer, requests) = fixture.check(format, target)?;
            let offer = offer.context("newer package release")?;
            assert_eq!(offer.asset.name, format.name("20260920.2", target)?);
            assert_eq!(offer.asset.target, target);
            assert_eq!(offer.notes, "Package release");
            assert_eq!(
                requests,
                [
                    LATEST_URL.into(),
                    release_url("20260920.2", "package-manifest.json"),
                    release_url("20260920.2", "package-manifest.sig"),
                    HISTORY_URL.into()
                ]
            );
        }
    }
    Ok(())
}

#[test]
fn missing_package_metadata_never_falls_back_to_legacy_archives() -> anyhow::Result<()> {
    for missing in ["package-manifest.json", "package-manifest.sig"] {
        let mut fixture = Packages::new()?;
        fixture.metadata["assets"]
            .as_array_mut()
            .context("assets")?
            .retain(|asset| asset["name"] != missing);
        for format in [Format::Deb, Format::Rpm, Format::Arch] {
            assert!(matches!(
                fixture.check(format, "x86_64-unknown-linux-gnu"),
                Err(Error::MissingPackageMetadata)
            ));
        }
    }
    Ok(())
}

#[test]
fn package_check_rejects_bad_signatures_and_mismatched_release_sizes() -> anyhow::Result<()> {
    let mut fixture = Packages::new()?;
    fixture.signature[0] ^= 1;
    assert!(matches!(
        fixture.check(Format::Deb, "x86_64-unknown-linux-gnu"),
        Err(Error::Signature(_))
    ));
    fixture.signature[0] ^= 1;
    fixture.metadata["assets"][0]["size"] = 4.into();
    assert!(matches!(
        fixture.check(Format::Deb, "x86_64-unknown-linux-gnu"),
        Err(Error::ArchiveMetadata)
    ));
    Ok(())
}
