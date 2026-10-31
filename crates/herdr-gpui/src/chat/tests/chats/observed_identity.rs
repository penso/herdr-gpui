use super::*;

fn observed(pane: &str, pid: u32) -> Target {
    let mut target = target(pane, A);
    target.existing = Some(existing::Binding {
        pane_id: pane.into(),
        agent: herdr_chat_protocol::Agent::Codex,
        session_id: "01234567-89ab-4cde-8123-456789abcdef".into(),
        herdr_session: Some("test-session".into()),
        pid,
        shell_pid: pid - 1,
        started_ticks: 123,
    });
    target
}

#[test]
fn panes_observing_one_session_hide_independently() {
    let mut harness = Harness::new();
    let targets = vec![observed("p1", 21), observed("p2", 31)];
    harness.reconcile(targets.clone());
    let mut streams = Vec::new();
    for _ in &targets {
        let (mut stream, _) = harness.program.accept();
        send(&mut stream, &welcome());
        send(
            &mut stream,
            &snapshot(cursor(1, 0), vec![agent(1, "Shared history")]),
        );
        streams.push(stream);
    }
    harness.poll_until(|chats| chats.shown("p1") && chats.shown("p2"));
    assert!(harness.chats.set_raw("p1", true));
    assert!(!harness.chats.shown("p1"));
    assert!(harness.chats.shown("p2"));
    harness.chats.set_raw("p2", true);
    harness.chats.set_raw("p2", false);
    harness.reconcile(targets);
    assert!(
        !harness.chats.shown("p1"),
        "another pane cannot clear this choice"
    );
    assert!(harness.chats.shown("p2"));
}

#[test]
fn observer_replacement_does_not_inherit_another_targets_hide_choice() {
    for change in ["pid", "start", "shell", "session", "host", "herdr_session"] {
        let mut harness = Harness::new();
        let original = observed("p1", 21);
        harness.reconcile(vec![original.clone()]);
        let (_old_stream, _) = harness.program.accept();
        harness.chats.set_raw("p1", true);
        let mut replacement = original;
        let binding = replacement.existing.as_mut().unwrap();
        match change {
            "pid" => binding.pid += 1,
            "start" => binding.started_ticks += 1,
            "shell" => binding.shell_pid += 1,
            "session" => binding.session_id = "87654321-4321-4321-8321-cba987654321".into(),
            "host" => replacement.route = Route::Ssh("another-host".into()),
            "herdr_session" => binding.herdr_session = Some("another-session".into()),
            _ => unreachable!(),
        }
        harness.reconcile(vec![replacement]);
        assert!(!harness.chats.raw("p1"), "{change}");
        let (_new_stream, greeting) = harness.program.accept();
        assert_eq!(greeting, hello(None));
    }
}
