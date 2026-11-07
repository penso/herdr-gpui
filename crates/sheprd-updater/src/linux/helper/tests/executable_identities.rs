use super::*;

#[test]
fn signed_requests_accept_both_installed_gui_identities() -> anyhow::Result<()> {
    for executable in ["/usr/bin/herdr-gpui", "/usr/bin/sheprd"] {
        let (prepared, public) = prepared_request(executable)?;
        write_request(&prepared)?;
        let (decoded, manifest) = request(prepared.stage.path(), &public, "20261010.1")?;
        assert_eq!(decoded.executable.as_os_str(), executable);
        assert_eq!(decoded.format, prepared.package.format);
        assert_eq!(manifest, prepared.offer.manifest);

        let command = authorization_command(prepared.stage.path());
        assert_eq!(command.get_program(), "/usr/bin/pkexec");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("/usr/bin/herdr-gpui"),
                ROOT_FLAG.as_ref(),
                prepared.stage.path().as_os_str(),
            ]
        );
    }
    Ok(())
}

#[test]
fn signed_requests_reject_other_paths_and_noncanonical_spellings() -> anyhow::Result<()> {
    for executable in [
        "/usr/local/bin/sheprd",
        "/tmp/sheprd",
        "/usr/bin/sheprd-other",
        "/usr/bin/Herdr",
        "/usr/bin/herdr-gpui-other",
        "sheprd",
        "usr/bin/sheprd",
        "/usr/bin/../bin/sheprd",
        "/usr/bin/./sheprd",
        "/usr//bin/sheprd",
        "/usr/bin/sheprd/",
        "/usr/bin/./herdr-gpui",
        "",
    ] {
        let (prepared, public) = prepared_request(executable)?;
        write_request(&prepared)?;
        assert!(
            matches!(
                request(prepared.stage.path(), &public, "20261010.1"),
                Err(Error::StaleRequest)
            ),
            "accepted executable {executable:?}"
        );
    }
    Ok(())
}

#[test]
fn canonical_gui_identity_still_requires_an_authentic_new_package() -> anyhow::Result<()> {
    let (mut prepared, public) = prepared_request("/usr/bin/sheprd")?;
    prepared.offer.signature[0] ^= 1;
    write_request(&prepared)?;
    assert!(matches!(
        request(prepared.stage.path(), &public, "20261010.1"),
        Err(Error::Signature(_))
    ));
    assert!(matches!(
        request(prepared.stage.path(), &public, "20261010.2"),
        Err(Error::StaleRequest)
    ));
    Ok(())
}
