//! The `en-XA` leak test (`crate::i18n::leak_tests`) for a chat pane: a
//! working turn with a user prompt, an answer, a tool call and its output,
//! the agent's plan, and a shell left running, in every layout, with the
//! running table open.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::{
    config::ChatLayoutMode,
    i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa},
};
use gpui::Modifiers;
use herdr_chat_protocol::{
    Activity, ActivityKind, ActivityState, PlanEntry, PlanPriority, PlanStatus, Tool, ToolContent,
    ToolKind, ToolStatus,
};
use herdr_client::ConnectTarget;

const PROMPT: &str = "Fix it";
const ANSWER: &str = "Done.";
const TITLE: &str = "cargo test";
const OUTPUT: &str = "ok";
const SHELL: &str = "npm run dev";
const STEPS: [&str; 2] = ["Read", "Write"];

fn working() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: PROMPT.into(),
                files: 0,
            },
        },
        agent(2, ANSWER),
        Entry {
            id: EntryId(3),
            kind: EntryKind::Tool(Tool {
                call_id: "run".into(),
                content: vec![ToolContent::Text {
                    text: OUTPUT.into(),
                }],
                kind: ToolKind::Execute,
                locations: Vec::new(),
                parent: None,
                status: ToolStatus::Completed,
                title: TITLE.into(),
            }),
        },
    ];
    transcript.plan = STEPS
        .iter()
        .zip([PlanStatus::Completed, PlanStatus::InProgress])
        .map(|(content, status)| PlanEntry {
            content: (*content).into(),
            status,
            priority: PlanPriority::default(),
        })
        .collect();
    transcript.activities = vec![Activity {
        call_id: "shell".into(),
        detail: None,
        finished_ms: None,
        kind: ActivityKind::Shell,
        label: SHELL.into(),
        started_ms: now_ms().saturating_sub(65_000),
        state: ActivityState::Running,
    }];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[test]
fn a_chat_shows_only_messages_and_its_conversation_under_en_xa() {
    under_en_xa("chat en-XA leak", |cx, recorder| {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        let program = Program::new();
        draw(cx);
        recorder.take();
        let _stream = attach_with(&view, &program, working(), cx);
        let data: Vec<String> = ["Claude", PROMPT, ANSWER, TITLE, OUTPUT, SHELL]
            .into_iter()
            .chain(STEPS)
            .map(str::to_owned)
            .collect();
        // GPUI shapes a line once, so a layout that cuts a line an earlier
        // one drew whole shows only the cut; it is checked against them all.
        let mut seen = Vec::new();
        let mut shown = |lines: Vec<String>| {
            let fresh = lines.len();
            seen.extend(lines);
            let uncut = uncut(seen.clone());
            let start = seen.len() - fresh;
            seen[start..]
                .iter()
                .filter(|line| uncut.contains(line))
                .cloned()
                .collect::<Vec<_>>()
        };
        for &layout in ChatLayoutMode::ALL {
            view.update(cx, |view, cx| {
                view.set_chat_layout_with(layout, |_| Ok(()), cx)
            });
            poll(&view, &program, cx);
            assert_translated(&format!("{layout:?} chat"), &shown(recorder.take()), &data);
        }
        let row = cx.debug_bounds("chat-running").unwrap();
        cx.simulate_click(row.center(), Modifiers::default());
        wait_until(|| {
            poll(&view, &program, cx);
            cx.debug_bounds("chat-running-row-0").is_some()
        });
        assert_translated("running table", &shown(recorder.take()), &data);
    });
}
