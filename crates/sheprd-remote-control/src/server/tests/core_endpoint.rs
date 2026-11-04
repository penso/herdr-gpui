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
        Some(sheprd_core_client::Endpoint {
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
