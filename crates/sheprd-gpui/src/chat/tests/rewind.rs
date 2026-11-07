//! "Edit from here": a prompt back in the composer, the choice about the
//! files, and the command that goes out.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{
    chat::rewind::{Checkpoint, Files, Stage, origin, running_origin},
    checkpoint::{Checkout, Checkpoint as Saved, Diff, Found, Request},
    window::chrome_focus::Region,
};
use gpui::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};
use sheprd_chat_protocol::{
    Attachment, Command, CommandId, PromptFiles, Rejection, SessionInfo, Turn,
};
use sheprd_client::{ConnectTarget, protocol::AgentStatus};
use sheprd_usage::Host;

const EDITED: EntryId = EntryId(3);

fn prompt(id: u64, text: &str, files: usize) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files,
        },
    }
}

fn transcript(rewind: bool) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        prompt(1, "first", 0),
        agent(2, "One."),
        prompt(3, "second\n\u{1f4ce} notes.md", 1),
        agent(4, "Two."),
    ];
    transcript.prompt_files = vec![PromptFiles {
        attachments: vec![Attachment {
            path: "/work/notes.md".into(),
        }],
        id: EDITED,
    }];
    transcript.session = Some(SessionInfo {
        cwd: Some("/work".into()),
        id: "s1".into(),
        rewind,
        title: None,
    });
    transcript
}

fn snapshot_of(transcript: Transcript) -> ServerMessage {
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn pane_id(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> String {
    view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().target.pane_id.clone()
    })
}

fn begin(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) {
    let pane = pane_id(view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_chat_rewind(&pane, EDITED, window, cx);
        });
    });
}

fn draft(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let pane = view.chats.panes().next().unwrap();
        pane.composer.as_ref().unwrap().read(cx).text().to_owned()
    })
}

fn with_rewind<T>(
    view: &Entity<crate::SheprdWindow>,
    cx: &mut VisualTestContext,
    read: impl FnOnce(&crate::chat::rewind::Rewind) -> T,
) -> T {
    view.read_with(cx, |view, _| {
        read(view.chats.panes().next().unwrap().rewind.as_ref().unwrap())
    })
}

fn rewinding(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().rewind.is_some()
    })
}

fn next_command(stream: &mut Peer) -> (Command, CommandId) {
    match read(stream) {
        Some(ClientMessage::Command { command, id }) => (command, id),
        other => panic!("a command, got {other:?}"),
    }
}

/// Polls chats and the checkpoint answers, as a tick does.
fn tick(view: &Entity<crate::SheprdWindow>, program: &Program, cx: &mut VisualTestContext) {
    poll(view, program, cx);
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_chat_rewinds(cx)));
    poll(view, program, cx);
}

/// A chat whose workspace shows a checkout checkpoints can reach.
fn open_in_checkout(
    cx: &mut TestAppContext,
    message: ServerMessage,
) -> (
    Entity<crate::SheprdWindow>,
    Program,
    Peer,
    &mut VisualTestContext,
) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let stream = attach_with(&view, &program, message, cx);
    let pane = pane_id(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.pane_id == pane)
                .unwrap();
            agent.agent_status = AgentStatus::Idle;
            let id = agent.workspace_id.clone();
            let workspace = snapshot
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.workspace_id == id)
                .unwrap();
            workspace.branch = Some("feature".into());
            workspace.worktree = Some(sheprd_client::protocol::ClientShellWorktree {
                key: "/repo/.git".into(),
                label: "repo".into(),
                is_linked_worktree: false,
            });
        });
    });
    (view, program, stream, cx)
}

fn checkout() -> Checkout {
    Checkout::new(Host::Local, "/repo/.git", Some("feature")).unwrap()
}

/// Answers the chat's checkpoint search with a checkpoint on `branch`.
fn found(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext, branch: &str) {
    let saved = Saved {
        id: "42".into(),
        created: 0,
        label: "Before Claude's turn".into(),
        branch: Some(branch.into()),
        diff: Diff::default(),
        origin: None,
    };
    let pane = pane_id(view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let transcript = view.chats.pane(&pane).unwrap().session.transcript();
            let origin = origin(&pane, transcript, EDITED).unwrap();
            view.chat_rewind_found(Found {
                checkout: checkout(),
                origin,
                result: Ok(Some(saved)),
            });
        });
    });
}

#[gpui::test]
fn the_prompt_and_its_files_come_back_and_keep_files_sends_the_rewind(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    cx.simulate_input("half a thought");
    poll(&view, &program, cx);

    begin(&view, cx);
    poll(&view, &program, cx);
    assert_eq!(draft(&view, cx), "second", "the file line is not text");
    assert!(cx.debug_bounds("chat-attachment-0").is_some());
    assert!(cx.debug_bounds("chat-rewind").is_some());
    assert_eq!(with_rewind(&view, cx, |rewind| rewind.files), Files::Keep);
    assert!(cx.debug_bounds("chat-rewind-files-lost").is_none());

    cx.simulate_input(" again");
    cx.simulate_keystrokes("enter");
    let (command, id) = next_command(&mut stream);
    assert_eq!(
        command,
        Command::Rewind {
            attachments: vec![Attachment {
                path: "/work/notes.md".into()
            }],
            before: EDITED,
            text: "second again".into(),
        }
    );
    tick(&view, &program, cx);
    assert_eq!(
        draft(&view, cx),
        "second again",
        "kept until it is answered"
    );

    send(&mut stream, &ServerMessage::Ack { id, rejected: None });
    wait_until(|| {
        tick(&view, &program, cx);
        !rewinding(&view, cx)
    });
    assert_eq!(draft(&view, cx), "");
    assert!(cx.debug_bounds("chat-attachment-0").is_none());
}

#[gpui::test]
fn a_refused_rewind_keeps_the_edit_and_says_why(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    begin(&view, cx);
    poll(&view, &program, cx);
    cx.simulate_keystrokes("enter");
    let (_, id) = next_command(&mut stream);
    send(
        &mut stream,
        &ServerMessage::Ack {
            id,
            rejected: Some(Rejection::RewindFailed),
        },
    );
    wait_until(|| {
        tick(&view, &program, cx);
        cx.debug_bounds("chat-problem").is_some()
    });
    tick(&view, &program, cx);
    assert_eq!(draft(&view, cx), "second");
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.stage.clone()),
        Stage::Editing
    );
}

#[gpui::test]
fn cancelling_puts_back_what_was_being_written(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    cx.simulate_input("half a thought");
    poll(&view, &program, cx);
    begin(&view, cx);
    poll(&view, &program, cx);
    let cancel = cx.debug_bounds("chat-rewind-cancel").unwrap();
    cx.simulate_click(cancel.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(!rewinding(&view, cx));
    assert_eq!(draft(&view, cx), "half a thought");
    assert!(cx.debug_bounds("chat-attachment-0").is_none());
}

#[gpui::test]
fn escape_in_the_composer_cancels(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    begin(&view, cx);
    poll(&view, &program, cx);
    cx.simulate_keystrokes("escape");
    poll(&view, &program, cx);
    assert!(!rewinding(&view, cx));
}

#[gpui::test]
fn an_agent_that_cannot_shows_it_as_unavailable(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(transcript(false)), cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chrome_region = Some(Region::Chat));
    });
    poll(&view, &program, cx);
    let shown = cx.debug_bounds("chat-rewind-3").unwrap();
    cx.simulate_click(shown.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(!rewinding(&view, cx));
    begin(&view, cx);
    assert!(!rewinding(&view, cx), "not by any other way either");
}

#[gpui::test]
fn each_prompt_offers_it_and_clicking_begins(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chrome_region = Some(Region::Chat));
    });
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rewind-1").is_some());
    assert!(
        cx.debug_bounds("chat-rewind-2").is_none(),
        "not the agent's"
    );
    let edit = cx.debug_bounds("chat-rewind-3").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(rewinding(&view, cx));
    assert_eq!(draft(&view, cx), "second");
}

#[gpui::test]
fn a_prompt_from_an_earlier_run_says_its_files_are_lost(cx: &mut TestAppContext) {
    let mut reopened = transcript(true);
    reopened.prompt_files.clear();
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(reopened), cx);
    begin(&view, cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rewind-files-lost").is_some());
    assert!(cx.debug_bounds("chat-attachment-0").is_none());
}

#[gpui::test]
fn restore_files_goes_first_then_the_rewind(cx: &mut TestAppContext) {
    let (view, program, mut stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    begin(&view, cx);
    let queued = view.read_with(cx, |view, _| view.checkpoints.queued());
    assert!(matches!(queued[0].1, Request::Find { .. }), "{queued:?}");
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.checkpoint.clone()),
        Checkpoint::Looking
    );
    found(&view, cx, "feature");
    poll(&view, &program, cx);
    let restore = cx.debug_bounds("chat-rewind-restore").unwrap();
    cx.simulate_click(restore.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.files),
        Files::Restore
    );

    cx.simulate_keystrokes("enter");
    let queued = view.read_with(cx, |view, _| view.checkpoints.queued());
    assert_eq!(queued[0].0, checkout());
    assert_eq!(queued[0].1, Request::Restore { id: "42".into() });
    assert!(matches!(
        with_rewind(&view, cx, |rewind| rewind.stage.clone()),
        Stage::Restoring { .. }
    ));

    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chat_rewind_restored(&checkout(), None));
    });
    let (command, _) = next_command(&mut stream);
    assert!(matches!(command, Command::Rewind { before: EDITED, .. }));
}

#[gpui::test]
fn a_failed_restore_leaves_the_conversation_alone(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    begin(&view, cx);
    found(&view, cx, "feature");
    poll(&view, &program, cx);
    let restore = cx.debug_bounds("chat-rewind-restore").unwrap();
    cx.simulate_click(restore.center(), Modifiers::default());
    poll(&view, &program, cx);
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.chat_rewind_restored(&checkout(), Some("the checkout is busy".into()));
        });
    });
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-problem").is_some());
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.stage.clone()),
        Stage::Editing
    );
    let pending = view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().session.since()
    });
    assert!(pending.is_some());
    assert_eq!(draft(&view, cx), "second");
}

#[gpui::test]
fn a_prompt_with_no_checkpoint_on_this_branch_cannot_restore(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    begin(&view, cx);
    found(&view, cx, "another-branch");
    poll(&view, &program, cx);
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.checkpoint.clone()),
        Checkpoint::Missing
    );
    let restore = cx.debug_bounds("chat-rewind-restore").unwrap();
    cx.simulate_click(restore.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert_eq!(with_rewind(&view, cx, |rewind| rewind.files), Files::Keep);
}

#[gpui::test]
fn a_chat_with_no_reachable_checkout_cannot_restore(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, snapshot_of(transcript(true)), cx);
    begin(&view, cx);
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.checkpoint.clone()),
        Checkpoint::Unreachable
    );
}

#[gpui::test]
fn a_working_agent_in_the_workspace_is_warned_of(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    begin(&view, cx);
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rewind-agent-working").is_none());
    let pane = pane_id(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.pane_id == pane)
                .unwrap();
            agent.agent_status = AgentStatus::Working;
        });
    });
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-rewind-agent-working").is_some());
}

#[gpui::test]
fn a_running_turn_must_stop_first(cx: &mut TestAppContext) {
    let mut working = transcript(true);
    working.status = Status::Working;
    working.turn = Some(Turn { started_ms: 1 });
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, snapshot_of(working), cx);
    begin(&view, cx);
    poll(&view, &program, cx);
    assert!(
        cx.debug_bounds("chat-stop").is_some(),
        "with an edit waiting, the button stops the turn"
    );
    cx.simulate_keystrokes("enter");
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-problem").is_some());
    let stop = cx.debug_bounds("chat-stop").unwrap();
    cx.simulate_click(stop.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream).0,
        Command::Cancel,
        "nothing else went out"
    );
}

#[gpui::test]
fn the_choice_works_from_the_keyboard(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    begin(&view, cx);
    found(&view, cx, "feature");
    poll(&view, &program, cx);
    cx.simulate_keystrokes("tab");
    poll(&view, &program, cx);
    let keep = with_rewind(&view, cx, |rewind| rewind.focus[0].clone());
    assert!(cx.update(|window, _| keep.is_focused(window)));
    assert_eq!(
        view.read_with(cx, |view, _| view.chrome_region),
        Some(Region::Chat)
    );
    cx.simulate_keystrokes("tab");
    poll(&view, &program, cx);
    let restore = with_rewind(&view, cx, |rewind| rewind.focus[1].clone());
    assert!(cx.update(|window, _| restore.is_focused(window)));
    // A focused control is used on the key's release.
    cx.simulate_keystrokes("space");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("space").unwrap(),
    });
    poll(&view, &program, cx);
    assert_eq!(
        with_rewind(&view, cx, |rewind| rewind.files),
        Files::Restore
    );
    let composer = view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().composer.clone().unwrap()
    });
    let focus = cx.update(|_, cx| composer.read(cx).focus_handle(cx));
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "typing goes back to the draft"
    );
}

#[test]
fn a_prompts_name_survives_a_reopened_chats_new_ids() {
    let mut live = transcript(true);
    let name = origin("p1", &live, EDITED).unwrap();
    assert_eq!(name.chat, "p1");
    assert!(name.prompt.starts_with("2-"), "{}", name.prompt);
    // Reopened from its log, the same prompt has another id and no file line.
    live.entries = vec![prompt(10, "first", 0), prompt(20, "second", 0)];
    assert_eq!(origin("p1", &live, EntryId(20)), Some(name.clone()));
    live.entries = vec![prompt(10, "first", 0), prompt(20, "edited", 0)];
    assert_ne!(origin("p1", &live, EntryId(20)), Some(name));
}

#[test]
fn only_a_running_turn_names_a_prompt() {
    let mut transcript = transcript(true);
    assert_eq!(running_origin("p1", &transcript), None);
    transcript.turn = Some(Turn { started_ms: 1 });
    assert_eq!(
        running_origin("p1", &transcript),
        origin("p1", &transcript, EDITED)
    );
}

#[test]
fn nothing_names_an_unknown_prompt() {
    assert_eq!(origin("p1", &transcript(true), EntryId(2)), None);
}
