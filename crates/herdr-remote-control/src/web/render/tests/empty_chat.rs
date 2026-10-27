use super::*;
use herdr_chat_protocol::Status;

#[test]
fn empty_chat_keeps_navigation_and_explains_how_to_start_with_an_agent_icon() {
    let row = row("p1");
    let workspace = workspace(vec![row.clone()]);
    let view = View {
        transcript: Transcript {
            status: Status::Idle,
            ..Transcript::default()
        },
        connected: true,
    };
    let page = chat(&workspace, &row, &view, 0).into_string();
    assert!(page.contains("aria-label=\"Back\""));
    assert!(page.contains("id=\"chat-head\""));
    assert!(!page.contains("backdrop-blur"));
    assert!(page.contains("id=\"chat-empty\""));
    assert!(page.contains("Start a conversation"));
    assert!(page.contains("Ask a question or describe a task"));
    assert!(page.contains("agent-mark"));
    assert!(page.contains("id=\"chat-composer\""));
}

#[test]
fn starting_and_stopped_agents_do_not_promise_they_are_ready() {
    let row = row("p1");
    for (status, title) in [
        (Status::Starting, "Starting your agent"),
        (
            Status::Failed {
                message: "Connection refused".into(),
            },
            "No conversation yet",
        ),
    ] {
        let view = View {
            transcript: Transcript {
                status,
                ..Transcript::default()
            },
            connected: true,
        };
        let html = Turn::new(&row, &view).whole();
        assert!(html.contains(title));
        assert!(!html.contains("Start a conversation"));
    }
}
