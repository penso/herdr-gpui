use herdr_chat_protocol::{ActivityKind, ActivityState};

use super::*;

#[test]
fn what_still_runs_when_the_agent_exits_is_marked_stopped() {
    let mut harness = ready();
    harness.update(json!({
        "sessionUpdate": "tool_call", "toolCallId": "bash", "title": "`npm run dev`",
        "kind": "execute", "status": "completed",
        "_meta": {"herdr": {"activity": {"kind": "shell", "state": "running"}}},
    }));
    let activity = harness.transcript().activity("bash").unwrap();
    assert_eq!(activity.kind, ActivityKind::Shell);
    assert!(activity.is_running());
    let receiver = harness.subscribe(1, false, None);
    harness.input(Input::AgentExited {
        code: Some(0),
        stderr: Vec::new(),
    });
    let activity = harness.transcript().activity("bash").unwrap();
    assert_eq!(activity.state, ActivityState::Killed);
    assert!(activity.finished_ms.is_some());
    assert!(
        drain(&receiver).iter().any(|message| matches!(
            message,
            ServerMessage::Change {
                change: Change::Activity { .. },
                ..
            }
        )),
        "clients hear of it"
    );
}
