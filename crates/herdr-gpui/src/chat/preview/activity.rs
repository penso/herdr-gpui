//! Synthetic live states; no provider, terminal, network or credentials.

use herdr_chat_protocol::{
    Entry, EntryId, EntryKind, Status, Tokens, Tool, ToolContent, ToolKind, ToolStatus, Transcript,
    Turn, TurnEnd, TurnSummary, Usage,
};
use herdr_client::protocol::AgentStatus;

pub(super) fn install(case: &str, transcript: &mut Transcript) -> AgentStatus {
    transcript.agent = "Custom ACP".into();
    transcript.status = Status::Working;
    transcript.turn = Some(Turn {
        started_ms: crate::chat::now_ms().saturating_sub(809_000),
    });
    transcript.usage = Some(Usage {
        cost: None,
        used: 42000,
        size: 200000,
    });
    transcript.entries = vec![
        Entry { id: EntryId(1), kind: EntryKind::Turn(TurnSummary {
            duration_ms: 23000, end: TurnEnd::Done,
            tokens: Some(Tokens { input: 3200, output: 1250, thought: 700, total: 4450, cached: 0 }),
        }) },
        Entry { id: EntryId(2), kind: EntryKind::User { text: "Check the training runs and explain the changes clearly.".into(), files: 0 } },
        Entry { id: EntryId(3), kind: EntryKind::Agent { text: "I’ll inspect the run status and check the updated code.\n\nThe activity strip below keeps the **reported work**, elapsed time, and clearly labeled token information visible.".into() } },
        Entry { id: EntryId(4), kind: EntryKind::Tool(Tool {
            parent: None,
            call_id: "synthetic-check".into(), kind: ToolKind::Execute, status: ToolStatus::InProgress,
            title: "Run training status checks".into(), locations: Vec::new(),
            content: vec![ToolContent::Text { text: "{\"command\":\"python scripts/check_runs.py --summary\\npython -m pytest tests/test_metrics.py\"}".into() }],
        }) },
    ];
    match case {
        "live-thinking" => transcript.entries.push(Entry {
            id: EntryId(5),
            kind: EntryKind::Thought {
                text: "Comparing the reported run metrics with the requested validation criteria."
                    .into(),
            },
        }),
        "live-waiting" => {
            transcript.read_only = true;
            return AgentStatus::Blocked;
        }
        "live-completed" | "live-failed" | "live-idle" => {
            transcript.turn = None;
            transcript.status = Status::Idle;
            if let EntryKind::Tool(tool) = &mut transcript.entries[3].kind {
                tool.status = if case == "live-failed" {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Completed
                };
            }
            if case != "live-idle" {
                transcript.entries.push(Entry {
                    id: EntryId(5),
                    kind: EntryKind::Turn(TurnSummary {
                        duration_ms: 809000,
                        end: if case == "live-failed" {
                            TurnEnd::Failed
                        } else {
                            TurnEnd::Done
                        },
                        tokens: Some(Tokens {
                            input: 3200,
                            output: 1250,
                            thought: 700,
                            total: 4450,
                            cached: 0,
                        }),
                    }),
                });
            }
            return AgentStatus::Idle;
        }
        _ => {}
    }
    AgentStatus::Working
}
