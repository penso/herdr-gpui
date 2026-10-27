use super::*;

fn snapshot() -> anyhow::Result<ClientShellSnapshot> {
    Ok(serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))?)
}

#[test]
fn the_dialog_names_the_agents_a_close_would_interrupt() -> anyhow::Result<()> {
    let mut snapshot = snapshot()?;
    let interrupts = |snapshot: &ClientShellSnapshot, pane: Option<&str>| {
        match pane {
            Some(pane) => CloseConfirmation::capture_pane(snapshot, pane),
            None => CloseConfirmation::capture_tab(snapshot, "w1:t1"),
        }
        .and_then(|close| close.interrupts)
    };
    // The fixture's Claude is blocked on a prompt.
    assert_eq!(
        interrupts(&snapshot, Some("w1:p1")).as_deref(),
        Some("Claude is waiting on you.")
    );
    snapshot.agents[0].agent_status = AgentStatus::Working;
    assert_eq!(
        interrupts(&snapshot, None).as_deref(),
        Some("Claude is working.")
    );
    let mut second = snapshot.agents[0].clone();
    second.pane_id = "w1:p2".into();
    second.agent_status = AgentStatus::Blocked;
    snapshot.agents.push(second);
    assert_eq!(
        interrupts(&snapshot, None).as_deref(),
        Some("2 agents are working or waiting on you.")
    );
    // A pane names only its own agent.
    assert_eq!(
        interrupts(&snapshot, Some("w1:p1")).as_deref(),
        Some("Claude is working.")
    );
    for agent in &mut snapshot.agents {
        agent.agent_status = AgentStatus::Done;
    }
    assert_eq!(interrupts(&snapshot, None), None);
    Ok(())
}
