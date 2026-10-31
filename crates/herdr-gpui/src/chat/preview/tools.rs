//! Synthetic tool payloads for native preview checks; none are executed.

use herdr_chat_protocol::{Entry, EntryId, EntryKind, Tool, ToolContent, ToolKind, ToolStatus};
use serde_json::json;

pub(super) fn entries() -> Vec<Entry> {
    vec![
        tool(
            901,
            ToolKind::Execute,
            ToolStatus::Completed,
            "Tool exec",
            vec![
                text(json!({
                    "command": "cargo test --lib sample::tests",
                    "description": "Check the sample calculation",
                }).to_string()),
                text("running 3 tests\ntest sample::tests::empty ... ok\ntest sample::tests::unicode ... ok\ntest sample::tests::sum ... ok\n\ntest result: ok. 3 passed; 0 failed"),
            ],
        ),
        tool(
            902,
            ToolKind::Execute,
            ToolStatus::InProgress,
            "Tool script",
            vec![text(json!({
                "code": "const samples = [2, 3, 5];\nconst total = samples.reduce((sum, value) => {\n  return sum + value;\n}, 0);\nconst mean = total / samples.length;\nconst report = {\n  count: samples.length,\n  total,\n  mean,\n};\n// Synthetic preview: this code is never executed.\nconsole.log(JSON.stringify(report));",
                "language": "javascript",
                "description": "Summarize the synthetic samples",
            }).to_string())],
        ),
        tool(
            903,
            ToolKind::Edit,
            ToolStatus::Completed,
            "Update the sample greeting",
            vec![ToolContent::Diff {
                path: "src/example.rs".into(),
                old: Some("fn greeting() -> &'static str {\n    \"Hello\"\n}\n".into()),
                new: "fn greeting() -> &'static str {\n    \"Hello, café 中文\"\n}\n".into(),
            }],
        ),
        tool(
            904,
            ToolKind::Think,
            ToolStatus::InProgress,
            "Compare the reported results",
            vec![text("Checking whether the sample count matches the displayed total.")],
        ),
        tool(
            905,
            ToolKind::Execute,
            ToolStatus::Pending,
            "Queued sample check",
            vec![text("The tool reports pending status. No live approval is requested by this fixture.")],
        ),
        tool(
            906,
            ToolKind::Execute,
            ToolStatus::Failed,
            "Sample validation failed",
            vec![
                text(json!({"command": "sample-check --expected 12"}).to_string()),
                text("expected total: 12\nreported total: 10\nvalidation exited with status 1"),
            ],
        ),
    ]
}

fn text(text: impl Into<String>) -> ToolContent {
    ToolContent::Text { text: text.into() }
}

fn tool(
    id: u64,
    kind: ToolKind,
    status: ToolStatus,
    title: &str,
    content: Vec<ToolContent>,
) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("synthetic-tool-{id}"),
            content,
            kind,
            locations: Vec::new(),
            parent: None,
            status,
            title: title.into(),
        }),
    }
}
