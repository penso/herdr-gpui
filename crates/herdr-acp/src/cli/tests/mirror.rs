use super::*;

#[test]
fn mirror_requires_an_explicit_native_transcript_and_parses_its_route() {
    let id = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        parse(args(&[
            "mirror",
            "codex",
            "/tmp/session.jsonl",
            "--pane",
            "p1",
            "--endpoint",
            id
        ])),
        Ok(Parsed::Mirror(MirrorOptions {
            agent: TranscriptAgent::Codex,
            path: "/tmp/session.jsonl".into(),
            pane: Some("p1".into()),
            endpoint: Some(id.parse().unwrap()),
        }))
    );
    assert!(matches!(
        parse(args(&["mirror", "claude", "/tmp/session.jsonl"])),
        Ok(Parsed::Mirror(_))
    ));
    assert_eq!(
        parse(args(&["mirror", "gemini", "/tmp/session.jsonl"])),
        Err(Error::TranscriptAgent)
    );
    assert_eq!(
        parse(args(&["mirror", "codex"])),
        Err(Error::MissingValue("mirror FILE"))
    );
    assert!(matches!(
        parse(args(&["mirror", "codex", "file", "--endpoint", "../other"])),
        Err(Error::InvalidEndpoint(_))
    ));
    assert_eq!(parse(args(&["mirror", "--help"])), Ok(Parsed::Help));
    assert_eq!(
        parse(args(&["mirror", "codex", "--help"])),
        Ok(Parsed::Help)
    );
}

#[cfg(unix)]
#[test]
fn mirror_preserves_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt as _;
    let path = OsString::from_vec(vec![b'/', b't', 0xff]);
    let Parsed::Mirror(options) =
        parse(vec!["mirror".into(), "claude".into(), path.clone()]).unwrap()
    else {
        panic!("mirror")
    };
    assert_eq!(options.path.as_os_str(), path);
}
