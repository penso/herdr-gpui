use super::*;

fn offer(name: &str, target: &str) -> release::Offer {
    let asset = release::Asset {
        target: target.into(),
        name: name.into(),
        size: 1,
        sha256: "00".repeat(32),
    };
    release::Offer {
        manifest: release::Manifest {
            schema: 2,
            version: "20261103.2".into(),
            assets: vec![asset.clone()],
        },
        asset,
        manifest_bytes: Vec::new(),
        signature: Vec::new(),
        notes: String::new(),
    }
}

#[test]
fn package_preparation_accepts_both_publication_names_for_each_format() -> Result<()> {
    for (format, extension) in [
        (Format::Deb, "deb"),
        (Format::Rpm, "rpm"),
        (Format::Arch, "pkg.tar.zst"),
    ] {
        let package = Package {
            format,
            executable: SYSTEM_EXECUTABLE.into(),
        };
        for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
            for prefix in ["Herdr", "Sheprd"] {
                let offer = offer(&format!("{prefix}-20261103.2-{target}.{extension}"), target);
                // Preparation must accept the same asset the manifest selector chose.
                assert_eq!(format.select(&offer.manifest, target)?, &offer.asset);
                validate_offer(&package, &offer)?;
            }
        }
    }
    Ok(())
}

#[test]
fn renamed_package_offers_still_reject_wrong_schema_format_version_and_paths() {
    let target = "x86_64-unknown-linux-gnu";
    let package = Package {
        format: Format::Deb,
        executable: SYSTEM_EXECUTABLE.into(),
    };
    for name in [
        "Sheprd-20261103.2-x86_64-unknown-linux-gnu.rpm",
        "Sheprd-20261103.1-x86_64-unknown-linux-gnu.deb",
        "Sheprd-20261103.2-aarch64-unknown-linux-gnu.deb",
        "../Sheprd-20261103.2-x86_64-unknown-linux-gnu.deb",
        "/Sheprd-20261103.2-x86_64-unknown-linux-gnu.deb",
        "Sheprd-20261103.2-x86_64-unknown-linux-gnu.deb.bak",
    ] {
        assert!(matches!(
            validate_offer(&package, &offer(name, target)),
            Err(Error::OfferAsset)
        ));
    }
    let mut offer = offer("Sheprd-20261103.2-x86_64-unknown-linux-gnu.deb", target);
    for schema in [1, 3] {
        offer.manifest.schema = schema;
        assert!(matches!(
            validate_offer(&package, &offer),
            Err(Error::OfferAsset)
        ));
    }
}
