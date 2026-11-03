use super::*;
use ed25519_dalek::{Signer, SigningKey};

#[test]
fn future_package_names_survive_signed_helper_handoff_and_protected_copy() -> anyhow::Result<()> {
    let key = SigningKey::from_bytes(&[42; 32]);
    let public: String = key
        .verifying_key()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        for (format, extension) in [
            (Format::Deb, "deb"),
            (Format::Rpm, "rpm"),
            (Format::Arch, "pkg.tar.zst"),
        ] {
            for prefix in ["Herdr", "Sheprd"] {
                for executable in ["/usr/bin/herdr-gpui", "/usr/bin/sheprd"] {
                    let mut asset = asset();
                    asset.target = target.into();
                    asset.name = format!("{prefix}-20261010.2-{target}.{extension}");
                    let manifest = release::Manifest {
                        schema: 2,
                        version: "20261010.2".into(),
                        assets: vec![asset.clone()],
                    };
                    let bytes = serde_json::to_vec(&manifest)?;
                    let mut request = Request {
                        executable: executable.into(),
                        format,
                        version: manifest.version.clone(),
                        signature: key.sign(&bytes).to_bytes().to_vec(),
                        manifest: bytes,
                    };
                    let (decoded, manifest) = decode_request(
                        serde_json::to_vec(&request)?.as_slice(),
                        &public,
                        "20261010.1",
                    )?;
                    let selected = decoded.format.select(&manifest, target)?;
                    assert_eq!(selected, &asset);
                    let storage = tempfile::tempdir()?;
                    with_protected_package(PAYLOAD, selected, storage.path(), |path| {
                        assert_eq!(path.file_name(), Some(asset.name.as_ref()));
                        assert_eq!(fs::read(path)?, PAYLOAD);
                        Ok(())
                    })?;
                    assert_eq!(fs::read_dir(storage.path())?.count(), 0);
                    assert!(matches!(
                        with_protected_package(&b"tampered"[..], selected, storage.path(), |_| {
                            panic!("tampered future package must not run")
                        }),
                        Err(Error::ArchiveDigest)
                    ));
                    request.executable = "/usr/local/bin/sheprd".into();
                    assert!(matches!(
                        decode_request(
                            serde_json::to_vec(&request)?.as_slice(),
                            &public,
                            "20261010.1"
                        ),
                        Err(Error::StaleRequest)
                    ));
                    request.executable = executable.into();
                    request.signature[0] ^= 1;
                    assert!(matches!(
                        decode_request(
                            serde_json::to_vec(&request)?.as_slice(),
                            &public,
                            "20261010.1"
                        ),
                        Err(Error::Signature(_))
                    ));
                }
            }
        }
    }
    Ok(())
}
