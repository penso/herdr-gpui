//! Incremental tool updates use the same card while a command is still running.
//! These go through the attach connection, reducer, preparation and real view.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Tool, ToolContent, ToolKind, ToolStatus};
use herdr_client::ConnectTarget;

fn command(output: &str, status: ToolStatus) -> Entry {
    Entry {
        id: EntryId(1),
        kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "command-1".into(),
            content: vec![ToolContent::Text {
                text: output.into(),
            }],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            status,
            title: "cargo test --lib".into(),
        }),
    }
}

#[gpui::test]
fn running_output_updates_an_open_card_before_completion(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(
        &view,
        &program,
        snapshot(
            cursor(1, 0),
            vec![command("Compiling", ToolStatus::InProgress)],
        ),
        cx,
    );
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    let before = cx.debug_bounds("chat-tool-content-1-0").unwrap();

    let running = command(
        "Compiling\nRunning unit tests\ntest one ... ok\ntest two ... ok",
        ToolStatus::InProgress,
    );
    send(
        &mut stream,
        &change(
            Change::Replace {
                entry: running.clone(),
            },
            cursor(1, 1),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-tool-content-1-0")
            .is_some_and(|bounds| {
                bounds.size.height
                    >= before.size.height
                        + gpui::px(
                            3. * view.read_with(cx, |view, _| view.config.terminal.size * 1.5),
                        )
            })
    });
    view.read_with(cx, |view, _| {
        let pane = view.chats.pane("w1:p1").unwrap();
        assert_eq!(pane.session.transcript().entries, [running]);
        assert!(pane.expanded.contains(&EntryId(1)));
    });

    // The completed aggregate replaces provisional output, not a second card
    // or a repeated copy of the already displayed output.
    let completed = command("test result: ok. 2 passed", ToolStatus::Completed);
    send(
        &mut stream,
        &change(
            Change::Replace {
                entry: completed.clone(),
            },
            cursor(1, 2),
        ),
    );
    wait_until(|| {
        poll(&view, &program, cx);
        view.read_with(cx, |view, _| {
            view.chats
                .pane("w1:p1")
                .unwrap()
                .session
                .transcript()
                .entries
                == [completed.clone()]
        })
    });
    assert!(cx.debug_bounds("chat-tool-details-1").is_some());
    assert!(cx.debug_bounds("chat-tool-content-1-1").is_none());
}

#[gpui::test]
fn streamed_output_respects_a_folded_card(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![command("", ToolStatus::InProgress)]),
        cx,
    );
    for (seq, status) in [(1, ToolStatus::InProgress), (2, ToolStatus::Failed)] {
        let entry = command("a retained diagnostic\nexit code: 1", status);
        send(
            &mut stream,
            &change(
                Change::Replace {
                    entry: entry.clone(),
                },
                cursor(1, seq),
            ),
        );
        wait_until(|| {
            poll(&view, &program, cx);
            view.read_with(cx, |view, _| {
                view.chats
                    .pane("w1:p1")
                    .unwrap()
                    .session
                    .transcript()
                    .entries
                    == [entry.clone()]
            })
        });
        assert!(cx.debug_bounds("chat-tool-details-1").is_none());
        assert!(cx.debug_bounds("chat-tool-content-1-0").is_none());
    }
    let header = cx.debug_bounds("chat-tool-1").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-tool-content-1-0").is_some());
}
