//! A report queue longer than one page says so instead of passing for all.
use super::*;
use crate::security::reports::PAGE;
use reports::{advisory_json, parse_json};

fn queue(length: usize) -> Reports {
    parse_json(Value::Array(
        (0..length)
            .map(|index| {
                advisory_json(
                    &format!("GHSA-{index:04}-aaaa-aaaa"),
                    "triage",
                    json!("low"),
                    "2026-10-05T00:00:00Z",
                )
            })
            .collect(),
    ))
}

#[test]
fn one_report_past_the_page_marks_the_list_partial() {
    let Reports::Open { reports, more } = queue(PAGE + 1) else {
        panic!("expected open reports");
    };
    assert_eq!(reports.len(), PAGE);
    assert!(more);
    let Reports::Open { reports, more } = queue(PAGE) else {
        panic!("expected open reports");
    };
    assert_eq!(reports.len(), PAGE);
    assert!(!more);
}
