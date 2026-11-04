use super::*;

#[test]
fn desktop_core_endpoint_reaches_every_service_definition() {
    let mut plan = plan();
    plan.core_endpoint = CoreTarget::Endpoint(sheprd_core_client::Endpoint {
        socket: "/desktop/core socket.sock".into(),
        session: "work".into(),
    });
    let args = plan.args();
    assert!(args.windows(4).any(|args| words(args)
        == [
            "--core-socket",
            "/desktop/core socket.sock",
            "--core-session",
            "work"
        ]));
    let plist = files::launch_agent(&plan).unwrap();
    let doc = roxmltree::Document::parse_with_options(
        &plist,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let strings = doc
        .descendants()
        .filter(|node| node.has_tag_name("string"))
        .filter_map(|node| node.text())
        .collect::<Vec<_>>();
    assert!(strings.windows(4).any(|words| words
        == [
            "--core-socket",
            "/desktop/core socket.sock",
            "--core-session",
            "work"
        ]));
    let unit = files::systemd_unit(&plan).unwrap();
    assert!(
        unit.contains(
            "\"--core-socket\" \"/desktop/core socket.sock\" \"--core-session\" \"work\""
        )
    );
    let task = files::scheduled_task(&plan, "user").unwrap();
    let doc = roxmltree::Document::parse(&task).unwrap();
    assert!(
        doc.descendants()
            .find(|node| node.tag_name().name() == "Arguments")
            .unwrap()
            .text()
            .unwrap()
            .contains("--core-socket \"/desktop/core socket.sock\" --core-session work")
    );
}

#[test]
fn invalid_session_diagnostics_survive_service_serialization() {
    use crate::core_target::CoreTarget;
    for session in ["bad/name", "bad\nname", "bad\tname", "bad\u{0}name"] {
        let mut plan = plan();
        plan.core_endpoint = CoreTarget::InvalidSession(session.into());
        assert!(files::launch_agent(&plan).is_ok());
        assert!(files::systemd_unit(&plan).is_ok());
        assert!(files::scheduled_task(&plan, "user").is_ok());
        #[cfg(feature = "server")]
        {
            let args = std::iter::once("remote".into()).chain(plan.args());
            let crate::server::Invocation::Serve(options) = crate::server::parse(args).unwrap()
            else {
                panic!("not serve")
            };
            assert_eq!(options.core_endpoint, plan.core_endpoint);
        }
    }
}
