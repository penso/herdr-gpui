//! Synthetic tool payloads for native preview checks; none are executed.

use serde_json::json;
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Tool, ToolContent, ToolKind, ToolStatus};

pub(super) fn entries() -> Vec<Entry> {
    vec![
        tool(
            901,
            ToolKind::Execute,
            ToolStatus::Completed,
            // i18n: data
            "Tool exec",
            vec![
                text(json!({
                    // i18n: data
                    "command": "cargo test --lib sample::tests",
                    // i18n: data
                    "description": "Check the sample calculation",
                }).to_string()),
                // i18n: data
                text("running 3 tests\ntest sample::tests::empty ... ok\ntest sample::tests::unicode ... ok\ntest sample::tests::sum ... ok\n\ntest result: ok. 3 passed; 0 failed"),
            ],
        ),
        tool(
            902,
            ToolKind::Execute,
            ToolStatus::InProgress,
            // i18n: data
            "Tool script",
            vec![text(json!({
                // i18n: data
                "code": "const samples = [2, 3, 5];\nconst total = samples.reduce((sum, value) => {\n  return sum + value;\n}, 0);\nconst mean = total / samples.length;\nconst report = {\n  count: samples.length,\n  total,\n  mean,\n};\n// Synthetic preview: this code is never executed.\nconsole.log(JSON.stringify(report));",
                "language": "javascript",
                // i18n: data
                "description": "Summarize the synthetic samples",
            }).to_string())],
        ),
        tool(
            903,
            ToolKind::Edit,
            ToolStatus::Completed,
            // i18n: data
            "Update the sample greeting",
            vec![ToolContent::Diff {
                path: "src/example.rs".into(),
                // i18n: data
                old: Some("fn greeting() -> &'static str {\n    \"Hello\"\n}\n".into()),
                // i18n: data
                new: "fn greeting() -> &'static str {\n    \"Hello, café 中文\"\n}\n".into(),
            }],
        ),
        tool(
            904,
            ToolKind::Think,
            ToolStatus::InProgress,
            // i18n: data
            "Compare the reported results",
            // i18n: data
            vec![text("Checking whether the sample count matches the displayed total.")],
        ),
        tool(
            905,
            ToolKind::Execute,
            ToolStatus::Pending,
            // i18n: data
            "Queued sample check",
            // i18n: data
            vec![text("The tool reports pending status. No live approval is requested by this fixture.")],
        ),
        tool(
            906,
            ToolKind::Execute,
            ToolStatus::Failed,
            // i18n: data
            "Sample validation failed",
            vec![
                text(json!({"command": "sample-check --expected 12"}).to_string()),
                // i18n: data
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
            started_ms: None,
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
