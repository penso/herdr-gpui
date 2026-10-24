use super::*;
use herdr_client::protocol::ClientShellSnapshot;

mod api_socket;
mod chat_link;
mod projection;

/// A snapshot with one workspace holding a chat tab and a terminal tab.
pub(super) fn snapshot(agents: Value) -> ClientShellSnapshot {
    serde_json::from_value(json!({
        "boot_id": "boot-1",
        "revision": 1,
        "update_install_command": "",
        "latest_release_notes_available": false,
        "integration_updates_available": false,
        "worktree_directory": "",
        "tab_bar_right": [],
        "tab_bar_right_separator": "",
        "agent_order": [],
        "workspaces": [{
            "workspace_id": "w1", "active_tab_id": "t1", "new_workspace_cwd": "/code/app",
            "number": 1, "label": "app", "custom_label": false, "branch": "main",
            "tokens": [], "focused": true, "agent_status": "idle"
        }],
        "tabs": [
            {"tab_id": "t1", "workspace_id": "w1", "number": 1, "label": "Claude",
             "custom_label": false, "zoomed": false, "focused": true, "agent_status": "working"},
            {"tab_id": "t2", "workspace_id": "w1", "number": 2, "label": "zsh",
             "custom_label": false, "zoomed": false, "focused": false, "agent_status": "idle"}
        ],
        "panes": [
            {"pane_id": "p1", "workspace_id": "w1", "tab_id": "t1", "cwd": "/code/app",
             "focused": true, "right_click_passthrough": false},
            {"pane_id": "p2", "workspace_id": "w1", "tab_id": "t2", "cwd": "/code/app",
             "foreground_cwd": "/code/app/src", "focused": false, "right_click_passthrough": false}
        ],
        "agents": agents,
        "commands": []
    }))
    .unwrap()
}

pub(super) fn agent(pane: &str, tab: &str, tokens: Value) -> Value {
    json!({
        "pane_id": pane, "workspace_id": "w1", "tab_id": tab, "agent": "claude",
        "title": "Remote plan", "agent_status": "working", "state_change_seq": 1,
        "state_labels": [], "tokens": tokens, "focused": false
    })
}

#[test]
fn a_tab_created_answer_names_its_tab_and_pane() {
    let created = json!({"type":"tab_created","tab":{"tab_id":"t3"},"root_pane":{"pane_id":"p5"}});
    assert_eq!(created_tab(&created), Some(("t3".into(), "p5".into())));
    assert_eq!(created_tab(&json!({"type":"workspace_created"})), None);
    assert_eq!(
        created_tab(&json!({"type":"tab_created","tab":{"tab_id":""},"root_pane":{"pane_id":"p"}})),
        None
    );
}

#[test]
fn herdr_errors_keep_a_bounded_message() {
    let long = "x".repeat(500);
    let error = api::result(json!({"error": {"message": long}})).unwrap_err();
    assert!(matches!(error, Error::Daemon(message) if message.len() == 200));
    assert_eq!(
        api::result(json!({"result": {"type": "ok"}})).unwrap(),
        json!({"type": "ok"})
    );
}

#[cfg(unix)]
#[test]
fn chat_lines_run_the_program_without_trusting_its_path_as_shell() {
    assert_eq!(
        command_line(Agent::Claude),
        " sh -c 'exec \"$HERDR_ACP\" claude'"
    );
    assert_eq!(
        direct_command_line(Path::new("/opt/it's/herdr-acp"), Agent::Codex),
        " '/opt/it'\\''s/herdr-acp' codex"
    );
}

#[test]
fn tab_params_carry_the_program_in_the_environment() {
    let params = tab_params("w1", "/code", Agent::Claude, Path::new("/bin/herdr-acp"));
    assert_eq!(params["env"][PROGRAM_ENV], "/bin/herdr-acp");
    assert_eq!(params["focus"], false);
    assert_eq!(params["cwd"], "/code");
}
