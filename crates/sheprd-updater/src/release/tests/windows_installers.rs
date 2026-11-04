use super::*;

/// A release whose `windows-manifest.json` (schema 3) lists both signed
/// installers, beside the other families' manifests.
struct Installers {
    metadata: serde_json::Value,
    manifest: Vec<u8>,
    signature: Vec<u8>,
    public: String,
}

fn installers_manifest() -> anyhow::Result<Manifest> {
    let mut manifest = fixture();
    manifest.schema = 3;
    manifest.assets.clear();
    for target in format::WINDOWS_TARGETS {
        manifest.assets.push(Asset {
            target: target.into(),
            name: Format::Installer.name(&manifest.version, target)?,
            size: 3,
            sha256: hex(&Sha256::digest(b"abc")),
        });
    }
    Ok(manifest)
}

impl Installers {
    fn new(manifest: &Manifest) -> anyhow::Result<Self> {
        let bytes = serde_json::to_vec(manifest)?;
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
                metadata_asset("windows-manifest.json", bytes.len() as u64),
                metadata_asset("windows-manifest.sig", 64),
                // The other families' metadata must never be a fallback.
                metadata_asset("update-manifest.json", 1),
                metadata_asset("update-manifest.sig", 64),
                metadata_asset("package-manifest.json", 1),
                metadata_asset("package-manifest.sig", 64),
            ])
            .collect();
        Ok(Self {
            metadata: serde_json::json!({"tag_name": "v20260920.2", "draft": false, "prerelease": false, "assets": assets, "body": "Windows release"}),
            signature: key.sign(&bytes).to_bytes().to_vec(),
            manifest: bytes,
            public: hex(key.verifying_key().as_bytes()),
        })
    }

    fn check(&self, format: Format, target: &str) -> Result<(Option<Offer>, Vec<String>)> {
        let mut requests = Vec::new();
        let (json, sig) = format.manifest();
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
                } else if url == release_url("20260920.2", json) {
                    self.manifest.clone()
                } else if url == release_url("20260920.2", sig) {
                    self.signature.clone()
                } else if url == HISTORY_URL {
                    // No grouped history in this fixture, so the offer keeps the
                    // target release's own notes.
                    b"[]".to_vec()
                } else {
                    panic!("unexpected fetch or fallback: {url}")
                };
                assert!(bytes.len() <= limit);
                Ok(bytes)
            },
        )?;
        Ok((offer, requests))
    }
}

#[test]
fn windows_checks_select_the_signed_installer_for_their_architecture() -> anyhow::Result<()> {
    let fixture = Installers::new(&installers_manifest()?)?;
    for target in format::WINDOWS_TARGETS {
        let (offer, requests) = fixture.check(Format::Installer, target)?;
        let offer = offer.context("newer Windows release")?;
        assert_eq!(
            offer.asset.name,
            format!("Herdr-20260920.2-{target}-setup.exe")
        );
        assert_eq!(offer.asset.target, target);
        assert_eq!(
            requests,
            [
                LATEST_URL.into(),
                release_url("20260920.2", "windows-manifest.json"),
                release_url("20260920.2", "windows-manifest.sig"),
                HISTORY_URL.into()
            ]
        );
    }
    Ok(())
}

#[test]
fn releases_without_windows_metadata_offer_nothing_to_install() -> anyhow::Result<()> {
    for missing in ["windows-manifest.json", "windows-manifest.sig"] {
        let mut fixture = Installers::new(&installers_manifest()?)?;
        fixture.metadata["assets"]
            .as_array_mut()
            .context("assets")?
            .retain(|asset| asset["name"] != missing);
        assert!(matches!(
            fixture.check(Format::Installer, "x86_64-pc-windows-msvc"),
            Err(Error::MissingInstallerMetadata)
        ));
    }
    Ok(())
}

#[test]
fn a_manifest_signed_for_another_family_never_answers_for_windows() -> anyhow::Result<()> {
    // A validly signed schema-1 manifest served as windows-manifest.json.
    let fixture = Installers::new(&fixture())?;
    assert!(matches!(
        fixture.check(Format::Installer, "x86_64-pc-windows-msvc"),
        Err(Error::ManifestSchema)
    ));
    Ok(())
}

#[test]
fn windows_manifest_policy_is_strict() -> anyhow::Result<()> {
    let manifest = installers_manifest()?;
    signed(&serde_json::to_vec(&manifest)?)?;
    // A Linux or macOS name in the Windows family is refused.
    let mut wrong = manifest.clone();
    wrong.assets[0].name = "Herdr-20260920.2-x86_64-unknown-linux-gnu.deb".into();
    assert!(signed(&serde_json::to_vec(&wrong)?).is_err());
    // One installer per target, and no more targets than Windows has.
    let mut duplicate = manifest.clone();
    duplicate.assets[1] = duplicate.assets[0].clone();
    assert!(signed(&serde_json::to_vec(&duplicate)?).is_err());
    let mut extra = manifest.clone();
    extra.assets.push(manifest.assets[0].clone());
    assert!(matches!(
        signed(&serde_json::to_vec(&extra)?),
        Err(Error::ManifestAssetCount)
    ));
    // Installers stay out of the schema-1 manifest old clients read.
    let mut legacy = manifest;
    legacy.schema = 1;
    assert!(signed(&serde_json::to_vec(&legacy)?).is_err());
    Ok(())
}

#[test]
fn installer_names_exist_only_for_windows_targets() {
    assert!(
        Format::Installer
            .name("20260920.2", "x86_64-unknown-linux-gnu")
            .is_err()
    );
    assert!(
        Format::Installer
            .name("20260920.2", "universal-apple-darwin")
            .is_err()
    );
    assert_eq!(
        Format::Installer.manifest(),
        ("windows-manifest.json", "windows-manifest.sig")
    );
}
