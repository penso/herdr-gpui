//! The redesigned chat: entries on a timeline with avatars where the
//! speaker changes, the rail with the plan and changed files, and the
//! composer's foot.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{
    PlanEntry, PlanPriority, PlanStatus, SlashCommand, Tool, ToolContent, ToolKind, ToolStatus,
};
use herdr_client::ConnectTarget;

pub(super) fn conversation(plan: bool) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Fix it".into(),
                files: 0,
            },
        },
        agent(2, "Looking."),
        Entry {
            id: EntryId(3),
            kind: EntryKind::Tool(Tool {
                call_id: "edit".into(),
                content: vec![ToolContent::Diff {
                    new: "a\nb\n".into(),
                    old: Some("a\n".into()),
                    path: "crates/app/src/view.rs".into(),
                }],
                kind: ToolKind::Edit,
                locations: Vec::new(),
                status: ToolStatus::Completed,
                title: "Edit view.rs".into(),
            }),
        },
        agent(4, "Done."),
    ];
    transcript.commands = vec![SlashCommand {
        description: "Review the changes".into(),
        hint: None,
        name: "review".into(),
    }];
    if plan {
        transcript.plan = vec![
            PlanEntry {
                content: "Read".into(),
                priority: PlanPriority::Medium,
                status: PlanStatus::Completed,
            },
            PlanEntry {
                content: "Edit".into(),
                priority: PlanPriority::Medium,
                status: PlanStatus::InProgress,
            },
        ];
    }
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn avatars_mark_where_the_speaker_changes(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(false), cx);
    // The selector is borrowed for as long as the window, so it is leaked.
    let mut drawn = |selector: String| cx.debug_bounds(selector.leak()).is_some();
    for id in [1, 2] {
        assert!(drawn(format!("chat-avatar-{id}")), "{id}");
    }
    for id in [3, 4] {
        assert!(drawn(format!("chat-dot-{id}")), "{id}");
        assert!(!drawn(format!("chat-avatar-{id}")), "{id}");
    }
    let user = cx.debug_bounds("chat-avatar-1").unwrap();
    let tool = cx.debug_bounds("chat-dot-3").unwrap();
    assert_eq!(user.center().x, tool.center().x, "one gutter");
}

#[gpui::test]
fn the_rail_shows_the_plan_and_changed_files_and_hides(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, conversation(true), cx);
    let rail = cx.debug_bounds("chat-rail").unwrap();
    let chat = cx.debug_bounds("chat-view").unwrap();
    assert_eq!(rail.right(), chat.right(), "the rail is on the right");
    assert!(cx.debug_bounds("chat-plan").is_some());
    assert!(cx.debug_bounds("chat-changed").is_some());
    assert!(cx.debug_bounds("chat-plan-pill").is_none());
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!(
        composer.right() <= rail.left(),
        "the composer stays beside it"
    );

    let toggle = cx.debug_bounds("chat-rail-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rail").is_none());
    assert!(cx.debug_bounds("chat-plan-pill").is_some());
    let toggle = cx.debug_bounds("chat-rail-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rail").is_some());
}

#[gpui::test]
fn nothing_to_show_means_no_rail(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Hi.")];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    assert!(cx.debug_bounds("chat-rail").is_none());
    assert!(cx.debug_bounds("chat-rail-toggle").is_none());
}

#[gpui::test]
fn the_send_button_sends_the_prompt(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, conversation(false), cx);
    cx.simulate_input("ship it");
    poll(&view, &program, cx);
    let send = cx.debug_bounds("chat-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::default());
    match read(&mut stream) {
        Some(ClientMessage::Command {
            command: herdr_chat_protocol::Command::Prompt { text, .. },
            ..
        }) => assert_eq!(text, "ship it"),
        other => panic!("a prompt, got {other:?}"),
    }
}

#[gpui::test]
fn a_compaction_summary_folds_into_a_card(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("OpenCode");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(
        1,
        "## Objective\n- Waiting.\n## Work State\n### Active\n- (none)\n## Next Move\n1. (none)\n",
    )];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    let folded = cx.debug_bounds("chat-summary-1").unwrap();
    // Copy takes the whole summary, so a cut one can still be read in full.
    assert!(cx.debug_bounds("chat-copy-1").is_some());
    assert!(cx.debug_bounds("chat-summary-line-0").is_none());
    cx.simulate_click(folded.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-summary-line-0").is_some());
    assert!(cx.debug_bounds("chat-summary-1").unwrap().size.height > folded.size.height);
}
