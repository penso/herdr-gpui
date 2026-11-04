use super::*;

#[test]
fn serve_accepts_the_desktops_explicit_core_endpoint() {
    assert!(
        parse(args(&[
            "serve",
            "--state-dir",
            "/desktop/remote",
            "--core-socket",
            "/desktop/core.sock",
            "--core-session",
            "work"
        ]))
        .is_ok()
    );
}

#[test]
fn explicit_endpoint_is_preserved_and_requires_both_parts() {
    let options = serve(&[
        "serve",
        "--state-dir",
        "/s",
        "--core-socket",
        "/desktop/core.sock",
        "--core-session",
        "work",
    ]);
    assert_eq!(
        options.core_endpoint,
        crate::core_target::CoreTarget::Endpoint(sheprd_core_client::Endpoint {
            socket: "/desktop/core.sock".into(),
            session: "work".into()
        })
    );
    for flag in ["--core-socket", "--core-session"] {
        assert_eq!(
            parse(args(&["serve", "--state-dir", "/s", flag, "value"])),
            Err(ParseError::CorePair)
        );
        assert_eq!(
            parse(args(&["serve", "--state-dir", "/s", flag])),
            Err(ParseError::Missing(flag))
        );
    }
}

#[test]
fn a_failed_desktop_resolution_is_preserved_without_discovery() {
    use crate::core_target::CoreTarget;
    let options = serve(&[
        "serve",
        "--state-dir",
        "/s",
        "--core-invalid-session",
        "6261642f6e616d65",
    ]);
    assert_eq!(
        options.core_endpoint,
        CoreTarget::InvalidSession("bad/name".into())
    );
    assert!(
        matches!(options.core_endpoint.endpoint(), Err(sheprd_core_client::Error::Session(name)) if name == "bad/name")
    );
    let options = serve(&["serve", "--state-dir", "/s", "--core-no-home"]);
    assert_eq!(options.core_endpoint, CoreTarget::NoHome);
    assert!(matches!(
        options.core_endpoint.endpoint(),
        Err(sheprd_core_client::Error::NoHome)
    ));
    assert_eq!(
        parse(args(&[
            "serve",
            "--state-dir",
            "/s",
            "--core-no-home",
            "--core-socket",
            "/other",
            "--core-session",
            "default"
        ])),
        Err(ParseError::CorePair)
    );
}
