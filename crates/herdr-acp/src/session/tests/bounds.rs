use super::*;
use herdr_chat_protocol::{Entry, EntryId, Tool, ToolContent, ToolKind, ToolStatus};

fn diff(index: u64, size: usize) -> Entry {
    Entry {
        id: EntryId(index),
        kind: EntryKind::Tool(Tool {
            call_id: format!("t{index}"),
            content: vec![ToolContent::Diff {
                new: "+".repeat(size),
                old: Some("-".repeat(size)),
                path: "/work/file".into(),
            }],
            kind: ToolKind::Edit,
            locations: vec!["/work/file".into()],
            status: ToolStatus::Completed,
            title: "Edit".into(),
        }),
    }
}

#[test]
fn diffs_count_toward_the_text_budget() {
    let entries: Vec<_> = (0..20).map(|index| diff(index, 128 * 1024)).collect();
    let keep = keep_from(&entries);
    assert!(keep > 0);
    let kept: usize = entries[keep..].iter().map(entry_size).sum();
    assert!(kept <= MAX_TRANSCRIPT_TEXT, "{kept}");
    assert!(kept + entry_size(&entries[keep - 1]) > MAX_TRANSCRIPT_TEXT);
}

#[test]
fn the_newest_entry_is_kept_even_over_budget() {
    let entries = vec![diff(0, 1), diff(1, MAX_TRANSCRIPT_TEXT)];
    assert_eq!(keep_from(&entries), 1);
    assert_eq!(keep_from(&entries[1..]), 0);
}
