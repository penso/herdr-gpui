//! Home is projected from the layout, the chats' summaries, and the pull
//! requests of the same boot.

use super::*;
use crate::control::pull_requests::{PullRequests, Tone, WorkspacePullRequest};
use herdr_chat_protocol::Status;
use model::{Home, Run, Sources};

fn summary(questions: usize) -> Summary {
    Summary {
        connected: true,
        status: Status::Working,
        question: (questions > 0).then(|| permission(1)),
        questions,
        ..Summary::default()
    }
}

fn pull_requests(boot: &str, branch: &str) -> PullRequests {
    PullRequests {
        boot_id: boot.into(),
        entries: vec![WorkspacePullRequest {
            workspace_id: "w1".into(),
            cwd: "/work".into(),
            branch: branch.into(),
            number: 7,
            url: "https://github.com/o/r/pull/7".into(),
            tone: Tone::Ready,
        }],
    }
}

fn project(summaries: HashMap<String, Summary>, pull_requests: &PullRequests) -> Home {
    Home::project(Sources {
        computer: "Studio",
        fingerprint: "FP",
        layout: &layout(EndpointId::random()),
        load: None,
        summaries,
        pull_requests,
    })
}

#[test]
fn home_lists_chats_and_terminals_with_their_summaries() {
    let home = project(
        [("p1".to_owned(), summary(2))].into(),
        &PullRequests::default(),
    );
    assert_eq!(home.computer, "Studio");
    assert!(home.connected);
    assert_eq!(home.waiting, 2);
    let workspace = &home.workspaces[0];
    assert_eq!(workspace.branch.as_deref(), Some("main"));
    let chat = &workspace.chats[0];
    assert_eq!(
        (chat.pane.as_str(), chat.status),
        ("p1", AgentStatus::Working)
    );
    assert_eq!(chat.summary.as_ref().unwrap().questions, 2);
    let terminal = &workspace.terminals[0];
    assert_eq!(terminal.panes[0].pane, "p2");
    assert!(terminal.panes[0].focused);
    let agent = terminal.agent.as_ref().unwrap();
    assert_eq!(agent.status, AgentStatus::Done);
    assert_eq!(agent.pane.as_deref(), Some("p2"));
    assert_eq!(agent.title.as_deref(), Some("Fix the build"));
}

#[test]
fn a_chat_not_linked_yet_has_no_summary() {
    let home = project(HashMap::new(), &PullRequests::default());
    assert_eq!(home.waiting, 0);
    assert!(home.workspaces[0].chats[0].summary.is_none());
    let json = serde_json::to_value(&home).unwrap();
    assert!(json["workspaces"][0]["chats"][0].get("summary").is_none());
}

#[test]
fn a_pull_request_shows_only_for_the_same_boot_and_branch() {
    let shown = project(HashMap::new(), &pull_requests("b", "main"));
    let pr = shown.workspaces[0].pull_request.as_ref().unwrap();
    assert_eq!((pr.number, pr.tone), (7, Tone::Ready));
    for stale in [
        pull_requests("other boot", "main"),
        pull_requests("b", "dev"),
    ] {
        let home = project(HashMap::new(), &stale);
        assert!(home.workspaces[0].pull_request.is_none());
        let json = serde_json::to_value(&home).unwrap();
        assert!(json["workspaces"][0].get("pull_request").is_none());
    }
}

#[test]
fn runs_name_palette_colors_and_write_only_the_flags_set() {
    let (columns, rows) = model::screen("\x1b[31mred\x1b[0m \x1b[95;1mbb\x1b[0m\nplain");
    assert_eq!(columns, 6);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0].fg.as_deref(), Some("red"));
    assert_eq!(rows[0][2].fg.as_deref(), Some("bright_magenta"));
    assert!(rows[0][2].bold);
    let plain = serde_json::to_value(&rows[1][0]).unwrap();
    assert_eq!(plain, serde_json::json!({"text": "plain"}));
}

#[test]
fn rgb_and_256_colors_become_hex() {
    let (_, rows) = model::screen("\x1b[38;2;1;2;3ma\x1b[48;5;196mb");
    assert_eq!(rows[0][0].fg.as_deref(), Some("#010203"));
    assert_eq!(rows[0][1].bg.as_deref(), Some("#ff0000"));
}

#[test]
fn terminal_escapes_other_than_colors_never_reach_a_run() {
    let (_, rows) = model::screen("a\x1b]0;title\x07b\x1b[2Jc\x07d");
    let text: String = rows[0].iter().map(|run: &Run| run.text.as_str()).collect();
    assert_eq!(text, "abcd");
}
