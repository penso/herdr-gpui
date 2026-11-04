use super::*;

const VERSION: &str = "20260920.2";
const NEW_LATEST: &str = "https://api.github.com/repos/sheprd-labs/sheprd/releases/latest";

struct Fixture {
    manifest: Manifest,
    bytes: Vec<u8>,
    signature: Vec<u8>,
    metadata: serde_json::Value,
    public: String,
}

impl Fixture {
    fn new(format: Format, target: &str, name: &str, repo: &str) -> anyhow::Result<Self> {
        let manifest = Manifest {
            schema: format.schema(),
            version: VERSION.into(),
            assets: vec![Asset {
                target: target.into(),
                name: name.into(),
                size: 3,
                sha256: hex(&Sha256::digest(b"abc")),
            }],
        };
        let bytes = serde_json::to_vec(&manifest)?;
        let key = SigningKey::from_bytes(&[42; 32]);
        let (json, sig) = format.manifest();
        let assets: Vec<_> = [(json, bytes.len() as u64), (sig, 64), (name, 3)].into_iter()
            .map(|(name, size)| serde_json::json!({"name":name, "size":size,
                "browser_download_url":format!("https://github.com/{repo}/releases/download/v{VERSION}/{name}")}))
            .collect();
        Ok(Self {
            manifest,
            signature: key.sign(&bytes).to_bytes().to_vec(),
            bytes,
            public: hex(key.verifying_key().as_bytes()),
            metadata: serde_json::json!({"tag_name":format!("v{VERSION}"), "draft":false,
                "prerelease":false, "assets":assets, "body":"New notes"}),
        })
    }

    fn check(
        &self,
        format: Format,
        channel: UpdateChannel,
        current: &str,
        absent: bool,
    ) -> Result<(Option<Offer>, Vec<String>)> {
        let mut requests = Vec::new();
        let offer = check_with(
            current,
            &self.public,
            channel,
            format,
            &self.manifest.assets[0].target,
            &AtomicBool::new(false),
            |url, _| {
                requests.push(url.to_owned());
                if absent && url.contains("/penso/herdr-gpui/") {
                    return Err(Error::HttpStatus(404));
                }
                if url.ends_with("/latest") {
                    return Ok(serde_json::to_vec(&self.metadata)?);
                }
                if url.ends_with("?per_page=10") {
                    return Ok(serde_json::to_vec(&[&self.metadata])?);
                }
                if url.ends_with("?per_page=20") {
                    return Ok(serde_json::to_vec(&[
                        serde_json::json!({"tag_name":"v20260920.1",
                        "draft":false, "prerelease":false, "assets":[], "body":"Earlier notes"}),
                    ])?);
                }
                if url.ends_with(format.manifest().0) {
                    return Ok(self.bytes.clone());
                }
                if url.ends_with(format.manifest().1) {
                    return Ok(self.signature.clone());
                }
                panic!("unexpected request {url}");
            },
        )?;
        Ok((offer, requests))
    }
}

/// Literal names are the publication contract, not an echo of name generation.
fn variants() -> Vec<(Format, &'static str, String)> {
    let mut result = Vec::new();
    for prefix in ["herdr-gpui", "sheprd"] {
        result.push((
            Format::Archive,
            "universal-apple-darwin",
            format!("{prefix}-{VERSION}-macos-universal.app.tar.gz"),
        ));
        for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
            result.push((
                Format::Archive,
                target,
                format!("{prefix}-{VERSION}-{target}-update.tar.gz"),
            ));
        }
    }
    for prefix in ["Herdr", "Sheprd"] {
        for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
            for (format, extension) in [
                (Format::Deb, "deb"),
                (Format::Rpm, "rpm"),
                (Format::Arch, "pkg.tar.zst"),
            ] {
                result.push((
                    format,
                    target,
                    format!("{prefix}-{VERSION}-{target}.{extension}"),
                ));
            }
        }
        for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
            result.push((
                Format::Installer,
                target,
                format!("{prefix}-{VERSION}-{target}-setup.exe"),
            ));
        }
    }
    result
}

#[test]
fn both_naming_contracts_authenticate_and_select_through_both_repository_families()
-> anyhow::Result<()> {
    for (format, target, name) in variants() {
        for repo in ["penso/herdr-gpui", "sheprd-labs/sheprd"] {
            let fixture = Fixture::new(format, target, &name, repo)?;
            for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
                for absent in [false, true] {
                    let (offer, requests) = fixture.check(format, channel, "20260919.1", absent)?;
                    let offer = offer.context("new offer")?;
                    assert_eq!(offer.asset, fixture.manifest.assets[0]);
                    assert!(offer.notes.contains("Earlier notes"));
                    assert!(requests.contains(&format!(
                        "https://github.com/{repo}/releases/download/v{VERSION}/{}",
                        format.manifest().0
                    )));
                    if channel == UpdateChannel::Stable {
                        assert_eq!(requests[0], NEW_LATEST);
                    }
                    if absent && repo == "penso/herdr-gpui" {
                        assert!(requests.contains(&format!(
                            "https://github.com/sheprd-labs/sheprd/releases/download/v{VERSION}/{}",
                            format.manifest().0
                        )));
                    }
                    // The authenticated digest continues to cover the payload.
                    copy_archive(
                        &b"abc"[..],
                        std::io::sink(),
                        &offer.asset,
                        &AtomicBool::new(false),
                        |_, _| {},
                    )?;
                    assert!(matches!(
                        copy_archive(
                            &b"abd"[..],
                            std::io::sink(),
                            &offer.asset,
                            &AtomicBool::new(false),
                            |_, _| {}
                        ),
                        Err(Error::ArchiveDigest)
                    ));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn canonical_metadata_does_not_change_channel_version_or_signature_policy() -> anyhow::Result<()> {
    let mut fixture = Fixture::new(
        Format::Archive,
        "universal-apple-darwin",
        "sheprd-20260920.2-macos-universal.app.tar.gz",
        "sheprd-labs/sheprd",
    )?;
    for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
        for current in [VERSION, "20260920.3"] {
            assert!(
                fixture
                    .check(Format::Archive, channel, current, true)?
                    .0
                    .is_none()
            );
        }
    }
    fixture.metadata["prerelease"] = true.into();
    assert!(matches!(
        fixture.check(Format::Archive, UpdateChannel::Stable, "20260920.1", false),
        Err(Error::UnstableRelease)
    ));
    assert!(
        fixture
            .check(Format::Archive, UpdateChannel::Beta, "20260920.1", false)?
            .0
            .is_some()
    );
    fixture.signature[0] ^= 1;
    assert!(matches!(
        fixture.check(Format::Archive, UpdateChannel::Beta, "20260920.1", true),
        Err(Error::Signature(_))
    ));
    fixture.signature[0] ^= 1;
    fixture.bytes.push(b' ');
    assert!(matches!(
        fixture.check(Format::Archive, UpdateChannel::Beta, "20260920.1", true),
        Err(Error::Signature(_))
    ));
    Ok(())
}

#[test]
fn names_do_not_expand_schema_slots_or_accept_near_matches() -> anyhow::Result<()> {
    for (format, target, name) in variants() {
        let fixture = Fixture::new(format, target, &name, "sheprd-labs/sheprd")?;
        for bad in [
            format!("{name}.extra"),
            format!("../{name}"),
            name.replace(VERSION, "20260920.3"),
            name.replace("20260920.2", "v20260920.2"),
        ] {
            let mut manifest = fixture.manifest.clone();
            manifest.assets[0].name = bad;
            assert!(matches!(
                signed(&serde_json::to_vec(&manifest)?),
                Err(Error::ManifestAsset)
            ));
        }
        let mut duplicate = fixture.manifest;
        let mut other = duplicate.assets[0].clone();
        other.name = if name.starts_with("Sheprd-") || name.starts_with("sheprd-") {
            format.name(VERSION, target)?
        } else {
            format::Naming::Sheprd.name(name)
        };
        duplicate.assets.push(other);
        assert!(matches!(
            signed(&serde_json::to_vec(&duplicate)?),
            Err(Error::ManifestAsset)
        ));
    }
    Ok(())
}

#[test]
fn metadata_still_binds_exact_repository_tag_and_file() -> anyhow::Result<()> {
    let fixture = Fixture::new(
        Format::Archive,
        "universal-apple-darwin",
        "sheprd-20260920.2-macos-universal.app.tar.gz",
        "sheprd-labs/sheprd",
    )?;
    let valid = fixture.metadata["assets"][0]["browser_download_url"]
        .as_str()
        .context("url")?;
    for bad in [
        valid.replace("sheprd-labs/sheprd", "attacker/sheprd"),
        valid.replace("sheprd-labs/sheprd", "sheprd-labs/sheprd-evil"),
        valid.replace("v20260920.2", "v20260920.1"),
        valid.replace("update-manifest.json", "windows-manifest.json"),
        valid.replace("https://", "http://"),
        format!("{valid}?download=1"),
        format!("{valid}#fragment"),
        valid.replace("/releases/", "/extra/../releases/"),
    ] {
        let mut metadata = fixture.metadata.clone();
        metadata["assets"][0]["browser_download_url"] = bad.into();
        assert!(matches!(
            parse_release(&serde_json::to_vec(&metadata)?),
            Err(Error::ReleaseAsset)
        ));
    }
    Ok(())
}

#[test]
fn remote_helpers_use_the_same_transfer_and_signed_naming_contract() -> anyhow::Result<()> {
    for target in HOST_TARGETS {
        for prefix in ["herdr-remote-control", "sheprd-remote-control"] {
            let mut manifest = fixture();
            manifest.schema = 4;
            manifest.assets[0].target = target.into();
            manifest.assets[0].name = format!("{prefix}-{VERSION}-{target}");
            let bytes = serde_json::to_vec(&manifest)?;
            let key = SigningKey::from_bytes(&[42; 32]);
            let public = hex(key.verifying_key().as_bytes());
            for tampered in [false, true] {
                let result = host::host_asset_with(VERSION, &public, target, |url, _| {
                    if url.contains("/penso/herdr-gpui/") {
                        return Err(Error::HttpStatus(404));
                    }
                    assert!(url.starts_with(
                        "https://github.com/sheprd-labs/sheprd/releases/download/v20260920.2/"
                    ));
                    if url.ends_with("host-manifest.json") {
                        return Ok(bytes.clone());
                    }
                    let mut sig = key.sign(&bytes).to_bytes();
                    if tampered {
                        sig[0] ^= 1;
                    }
                    Ok(sig.to_vec())
                });
                if tampered {
                    assert!(matches!(result, Err(Error::Signature(_))));
                } else {
                    assert_eq!(result?, manifest.assets[0]);
                }
            }
        }
    }
    Ok(())
}
