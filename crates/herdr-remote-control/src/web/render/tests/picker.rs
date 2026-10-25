use super::*;

fn folders() -> Folders {
    Folders {
        recent: vec!["/srv/code/agent-launcher".into()],
        roots: vec![
            (
                "/srv/worktrees".into(),
                vec!["/srv/worktrees/moltis/agent-1255-bug-agentend-messagesending".into()],
            ),
            ("/srv/empty".into(), vec![]),
        ],
    }
}

#[test]
fn the_project_is_a_searchable_list_not_a_select() {
    let html = new_workspace(&folders()).into_string();
    assert!(!html.contains("<select"), "{html}");
    assert!(
        html.contains(r#"<input type="hidden" name="cwd" value="/srv/code/agent-launcher""#),
        "{html}"
    );
    assert!(html.contains(r#"type="search""#));
    // One line per folder: its name, then where it sits.
    assert!(
        html.contains(">agent-1255-bug-agentend-messagesending</span>"),
        "{html}"
    );
    assert!(html.contains(">/srv/worktrees/moltis</small>"), "{html}");
    assert!(
        html.contains(
            r#"data-find="/srv/worktrees/moltis/agent-1255-bug-agentend-messagesending""#
        )
    );
    // A root with nothing in it gets no heading.
    assert!(!html.contains(">/srv/empty<"), "{html}");
    assert_eq!(html.matches(r#"aria-pressed="true""#).count(), 1);
}

#[test]
fn no_folders_asks_for_a_project() {
    let html = new_workspace(&Folders::default()).into_string();
    assert!(html.contains("Choose a project"));
    assert!(
        html.contains(r#"<input type="hidden" name="cwd" x-ref="value">"#),
        "{html}"
    );
}
