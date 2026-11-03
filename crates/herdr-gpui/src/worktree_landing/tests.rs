#![allow(clippy::unwrap_used)]
use super::*;

fn picker(chosen: Landing) -> Picker {
    Picker::new(
        crate::teleport::host_for(&herdr_client::ConnectTarget::Local).unwrap(),
        chosen,
    )
}

#[test]
fn the_remembered_agent_is_offered_while_the_host_is_looked_at() {
    let picker = picker(Landing::Agent(AgentKind::Claude));
    assert!(picker.looking());
    assert_eq!(
        picker.offered(),
        [Landing::Terminal, Landing::Agent(AgentKind::Claude)]
    );
    assert_eq!(picker.chosen, Landing::Agent(AgentKind::Claude));
}

#[test]
fn installed_agents_follow_the_terminal_and_a_missing_choice_falls_back() {
    let mut picker = picker(Landing::Agent(AgentKind::Claude));
    picker.offer(Ok(vec![AgentKind::Codex, AgentKind::Pi]));
    assert!(!picker.looking());
    assert_eq!(picker.chosen, Landing::Terminal);
    assert_eq!(
        picker.offered(),
        [
            Landing::Terminal,
            Landing::Agent(AgentKind::Codex),
            Landing::Agent(AgentKind::Pi),
        ]
    );
    assert!(picker.choose(Landing::Agent(AgentKind::Pi)));
    assert_eq!(picker.chosen, Landing::Agent(AgentKind::Pi));
    assert!(!picker.choose(Landing::Agent(AgentKind::Claude)));
    assert_eq!(picker.chosen, Landing::Agent(AgentKind::Pi));
}

#[test]
fn an_installed_choice_is_kept() {
    let mut picker = picker(Landing::Agent(AgentKind::Claude));
    picker.offer(Ok(vec![AgentKind::Claude]));
    assert_eq!(picker.chosen, Landing::Agent(AgentKind::Claude));
}

#[test]
fn a_failed_lookup_offers_every_agent() {
    let mut picker = picker(Landing::Agent(AgentKind::Codex));
    picker.offer(Err(herdr_client::Error::Disconnected));
    assert_eq!(picker.chosen, Landing::Agent(AgentKind::Codex));
    assert_eq!(picker.offered().len(), AgentKind::ALL.len() + 1);
}

#[test]
fn the_choice_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let mut remembered = Remembered::at(Some(path.clone()));
    assert_eq!(remembered.landing, Landing::Terminal);
    remembered.landing = Landing::Agent(AgentKind::Claude);
    if let Some(writer) = &remembered.writer {
        writer.save(remembered.landing.into());
    }
    remembered.finish();
    assert_eq!(
        Remembered::at(Some(path.clone())).landing,
        Landing::Agent(AgentKind::Claude)
    );
    std::fs::write(&path, br#"{}"#).unwrap();
    assert_eq!(Remembered::at(Some(path)).landing, Landing::Terminal);
}

#[test]
fn unknown_or_damaged_files_land_in_the_terminal() {
    assert_eq!(
        parse(br#"{"agent":"someday-agent"}"#).unwrap(),
        Landing::Terminal
    );
    assert_eq!(
        parse(br#"{"agent":"codex"}"#).unwrap(),
        Landing::Agent(AgentKind::Codex)
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    std::fs::write(&path, b"not json").unwrap();
    assert_eq!(Remembered::at(Some(path)).landing, Landing::Terminal);
}

#[gpui::test]
fn recording_replaces_the_remembered_landing(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        assert_eq!(Remembered::landing(cx), Landing::Terminal);
        Remembered::record(Landing::Agent(AgentKind::Claude), cx);
        assert_eq!(Remembered::landing(cx), Landing::Agent(AgentKind::Claude));
        Remembered::record(Landing::Terminal, cx);
        assert_eq!(Remembered::landing(cx), Landing::Terminal);
    });
}

#[test]
fn agent_names_are_ones_herdr_accepts() {
    for kind in AgentKind::ALL {
        let name = agent_name(kind);
        assert!(name.len() <= 32, "{name}");
        assert!(
            name.starts_with(|ch: char| ch.is_ascii_lowercase()),
            "{name}"
        );
        assert!(
            name.chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'),
            "{name}"
        );
    }
}

#[test]
fn only_a_created_root_pane_is_landed_in() {
    let created = serde_json::json!({"result":{"type":"worktree_created",
        "root_pane":{"pane_id":"p9","terminal_id":"t_42"}}});
    assert_eq!(root_pane(&created), Some(("p9", Some("t_42"))));
    // Without the terminal's identity a busy shell is not retried.
    let older = serde_json::json!({"result":{"type":"worktree_created",
        "root_pane":{"pane_id":"p9"}}});
    assert_eq!(root_pane(&older), Some(("p9", None)));
    let opened = serde_json::json!({"result":{"type":"worktree_opened",
        "root_pane":{"pane_id":"p9"}}});
    assert_eq!(root_pane(&opened), None);
    let refused = serde_json::json!({"error":{"code":"worktree_create_failed"}});
    assert_eq!(root_pane(&refused), None);
    let blank = serde_json::json!({"result":{"type":"worktree_created",
        "root_pane":{"pane_id":""}}});
    assert_eq!(root_pane(&blank), None);
}

#[test]
fn a_launch_waits_on_its_request_on_its_connection_and_boot() {
    let launch = Launch::new(
        "create",
        (3, 7),
        "boot-a",
        &picker(Landing::Agent(AgentKind::Claude)),
    );
    assert_eq!(launch.request(), "create");
    assert!(launch.current((3, 7), Some("boot-a")));
    assert!(!launch.current((3, 8), Some("boot-a")));
    assert!(!launch.current((4, 7), Some("boot-a")));
    assert!(!launch.current((3, 7), Some("boot-b")));
    assert!(!launch.current((3, 7), None));
}
