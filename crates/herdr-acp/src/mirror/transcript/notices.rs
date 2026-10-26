//! Native system origins, never keywords in a person's message, identify activity.

use herdr_chat_protocol::{EntryKind, NoticeLevel, QuestionAnswer};
use serde_json::Value;

use super::{TEXT, limited, string};

const SUMMARY: usize = 160;
const CUT: &str = "\n… (source details truncated)";

/// A notice for a user record a harness wrote, or `None` for a person's own.
pub(super) fn project(record: &Value, content: &str) -> Option<EntryKind> {
    if content.is_empty() {
        return None;
    }
    let system = string(record, "promptSource") == "system" || record["isMeta"] == true;
    let (label, summary) = if string(&record["origin"], "kind") == "task-notification" {
        ("Task notification", task_summary(content))
    } else if system
        && string(record, "turnOrigin") == "scheduled"
        && !string(record, "scheduledTaskId").is_empty()
    {
        ("Scheduled reminder", first_line(content))
    } else if system {
        ("System activity", first_line(content))
    } else {
        return None;
    };
    let summary = summary.map(compact).unwrap_or_default();
    let text = if summary.is_empty() {
        label.to_owned()
    } else {
        format!("{label} · {summary}")
    };
    Some(source(text, content))
}

/// A notice whose details show the native `content` it stands for.
pub(super) fn source(text: String, content: &str) -> EntryKind {
    notice(text, Some(content), Vec::new())
}

/// A compacted context, with details only when Codex kept a summary.
pub(super) fn compacted(message: &str) -> EntryKind {
    let details = (!message.is_empty()).then_some(message);
    notice("Context compacted".into(), details, Vec::new())
}

/// Answered questions, with the native record that answered them.
pub(super) fn answered(answers: Vec<QuestionAnswer>, content: &str) -> EntryKind {
    let text = if answers.len() == 1 {
        "Question answered"
    } else {
        "Questions answered"
    };
    notice(text.into(), Some(content), answers)
}

fn notice(text: String, content: Option<&str>, answers: Vec<QuestionAnswer>) -> EntryKind {
    EntryKind::Notice {
        answers,
        details: content.map(details),
        level: NoticeLevel::Info,
        text,
    }
}

fn details(content: &str) -> String {
    if content.len() > TEXT {
        format!("{}{CUT}", limited(content, TEXT - CUT.len()))
    } else {
        content.to_owned()
    }
}

fn first_line(content: &str) -> Option<&str> {
    content.lines().find(|line| !line.trim().is_empty())
}

fn task_summary(content: &str) -> Option<&str> {
    // Only read a display label from the known envelope. No XML entities,
    // links, file paths, or instructions in it are interpreted or followed.
    let content = content
        .trim()
        .strip_prefix("<task-notification>")?
        .strip_suffix("</task-notification>")?;
    let summary = content
        .split_once("<summary>")?
        .1
        .split_once("</summary>")?
        .0;
    (!summary.contains(['<', '>'])).then_some(summary)
}

fn compact(text: &str) -> String {
    let mut summary = text.split_whitespace().collect::<Vec<_>>().join(" ");
    summary.retain(|ch| !ch.is_control());
    if summary.len() > SUMMARY {
        summary = limited(&summary, SUMMARY);
        summary.push('…');
    }
    summary
}
