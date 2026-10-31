use super::*;

#[test]
fn a_workspace_names_its_status_and_a_remote_device() {
    assert_eq!(
        workspace_name("herdr", false, AgentStatus::Blocked, false, None),
        "herdr, blocked"
    );
    assert_eq!(
        workspace_name("herdr", false, AgentStatus::Unknown, false, Some("devbox")),
        "herdr, devbox"
    );
    assert_eq!(
        workspace_name("fix/rounding", true, AgentStatus::Working, false, None),
        "fix/rounding, linked worktree, working"
    );
    assert_eq!(
        workspace_name("herdr", false, AgentStatus::Done, true, Some("devbox")),
        "herdr, done, notifications muted, devbox"
    );
}

#[test]
fn an_agent_names_its_state_and_place() {
    assert_eq!(
        agent_name(
            "claude",
            None,
            "working",
            Some(("herdr", Some("tests"))),
            None
        ),
        "claude, working, herdr, tests"
    );
    assert_eq!(
        agent_name("codex", None, "needs input", None, Some("devbox")),
        "codex, needs input, devbox"
    );
}

#[test]
fn a_pane_name_tells_same_named_agents_apart() {
    assert_eq!(
        agent_name("grok", Some("api"), "working", Some(("herdr", None)), None),
        "grok, api, working, herdr"
    );
}

#[test]
fn a_device_names_its_connection_and_working_agents() {
    assert_eq!(device_name("Local", true, None), "Local, connected");
    assert_eq!(
        device_name("devbox", false, Some("2/5")),
        "devbox, disconnected, \u{2068}2/5\u{2069} working"
    );
}

#[test]
fn blank_parts_leave_no_empty_commas() {
    assert_eq!(
        agent_name("claude", Some(" "), " ", Some(("", None)), None),
        "claude"
    );
}
