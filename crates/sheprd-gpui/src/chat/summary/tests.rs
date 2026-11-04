use super::*;
use crate::release_notes::parse;

const COMPACTED: &str = "## Objective\n- No technical task in progress.\n\n## Important Details\n\
- Language: English.\n\n## Work State\n### Completed\n- Delivered joke.\n### Active\n- (none)\n\
### Blocked\n- (none)\n\n## Next Move\n1. (none) — awaiting a new task.\n2. (none)\n\n\
## Relevant Files\n- (none)\n";

#[test]
fn a_compaction_summary_keeps_only_what_it_says() {
    let summary = summary(&parse(COMPACTED)).unwrap();
    assert_eq!(summary.gist, "No technical task in progress.");
    let headings: Vec<_> = summary
        .lines
        .iter()
        .filter(|line| line.kind == Kind::Heading)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(
        headings,
        ["Objective", "Important Details", "Work State", "Completed"]
    );
    assert!(
        !summary
            .lines
            .iter()
            .any(|line| line.text.contains("(none)"))
    );
}

#[test]
fn an_ordinary_answer_is_not_a_summary() {
    assert!(summary(&parse("## Plan\n- one\n## Objective\n- two\n")).is_none());
    assert!(summary(&parse("No next steps on my end.")).is_none());
}
