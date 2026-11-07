//! The choice of the files, where checkpoints can reach the chat's checkout:
//! this machine's own is out of their reach on Windows (`host_for`).

use super::*;
use crate::checkpoint::{Checkout, Checkpoint as Saved, Diff, Found, Request};
use gpui::Focusable;
use sheprd_client::protocol::AgentStatus;
use sheprd_usage::Host;

/// The restore the edit is waiting for.
fn ticket(view: &Entity<crate::SheprdWindow>, cx: &mut VisualTestContext) -> u64 {
    with_rewind(view, cx, |rewind| match rewind.stage {
        Stage::Restoring { ticket, .. } => ticket,
        _ => panic!("no restore is running"),
    })
}

/// An edit that chose Restore files and was sent, waiting for the restore.
fn restoring(view: &Entity<crate::SheprdWindow>, program: &Program, cx: &mut VisualTestContext) {
    begin(view, cx);
    found(view, cx, "feature");
    poll(view, program, cx);
    let restore = cx.debug_bounds("chat-rewind-restore").unwrap();
    cx.simulate_click(restore.center(), Modifiers::default());
    poll(view, program, cx);
    cx.simulate_keystrokes("enter");
}

#[gpui::test]
fn another_restore_finishing_leaves_the_edit_waiting(cx: &mut TestAppContext) {
    let (view, program, mut stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    restoring(&view, &program, cx);
    let ticket = ticket(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chat_rewind_restored(ticket + 1, None));
    });
    assert!(matches!(
        with_rewind(&view, cx, |rewind| rewind.stage.clone()),
        Stage::Restoring { .. }
    ));
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chat_rewind_restored(ticket, None));
    });
    assert!(matches!(
        next_command(&mut stream).0,
        Command::Rewind { .. }
    ));
}

#[gpui::test]
fn one_restore_of_a_checkout_runs_at_a_time(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    restoring(&view, &program, cx);
    let pane = pane_id(&view, cx);
    view.read_with(cx, |view, _| {
        assert!(view.chat_restoring("another chat", &checkout()));
        assert!(!view.chat_restoring(&pane, &checkout()), "not its own");
        let elsewhere = Checkout::new(Host::Local, "/other/.git", Some("feature")).unwrap();
        assert!(!view.chat_restoring("another chat", &elsewhere));
    });
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

    let ticket = ticket(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.chat_rewind_restored(ticket, None));
    });
    let (command, _) = next_command(&mut stream);
    assert!(matches!(command, Command::Rewind { before: EDITED, .. }));
}

#[gpui::test]
fn a_failed_restore_leaves_the_conversation_alone(cx: &mut TestAppContext) {
    let (view, program, _stream, cx) = open_in_checkout(cx, snapshot_of(transcript(true)));
    restoring(&view, &program, cx);
    let ticket = ticket(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.chat_rewind_restored(ticket, Some("the checkout is busy".into()));
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
