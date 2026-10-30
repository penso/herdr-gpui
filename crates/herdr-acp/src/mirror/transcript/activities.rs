//! Claude Code's sub-agents and background shells as its log records them,
//! followed the way the native bridge follows them live and timed by each
//! record's `timestamp`. A sub-agent's own steps live in a log of their own,
//! which the mirror does not read, so they are not shown here.

use herdr_chat_protocol::EntryKind;
use serde_json::Value;

use super::{Projection, string};
use crate::{
    claude::background,
    project::{Report, activity},
};

/// The longest tool name kept to tell results apart.
const MAX_NAME: usize = 64;

impl Projection {
    pub(super) fn activity_started(
        &mut self,
        call: &str,
        name: &str,
        input: &Value,
        record: &Value,
    ) {
        let Some(&index) = self.tools.get(call) else {
            return;
        };
        if name.len() <= MAX_NAME {
            self.names.insert(call.to_owned(), name.to_owned());
        }
        let tag = self.background.started(call, name, input, None);
        if let Some(EntryKind::Tool(tool)) = self
            .transcript
            .entries
            .get_mut(index)
            .map(|entry| &mut entry.kind)
        {
            tag.mark(tool);
        }
        if let Some(report) = tag.activity {
            self.report(call, report, record);
        }
    }

    /// A result: the one in a record with `results` results carries the
    /// record's structured `toolUseResult` only when it is alone.
    pub(super) fn activity_result(
        &mut self,
        call: &str,
        failed: bool,
        output: &Value,
        results: usize,
        record: &Value,
    ) {
        let Some(name) = self.names.remove(call) else {
            return;
        };
        let structured = record.get("toolUseResult").filter(|_| results == 1);
        let (own, other) = self
            .background
            .result(call, &name, failed, output, structured);
        if let Some(report) = own {
            self.report(call, report, record);
        }
        if let Some((call, report)) = other {
            self.report(&call, report, record);
        }
    }

    /// A task notification Claude Code recorded as a prompt of its own.
    pub(super) fn activity_notified(&mut self, record: &Value, content: &str) {
        if string(&record["origin"], "kind") != "task-notification" {
            return;
        }
        let Some((task, call, status, summary)) = background::task_notification(content) else {
            return;
        };
        if let Some((call, report)) = self.background.notified(task, call, status, summary) {
            self.report(&call, report, record);
        }
    }

    fn report(&mut self, call: &str, report: Report, record: &Value) {
        let Some(now) = timestamp(record) else {
            return;
        };
        let title = self
            .tools
            .get(call)
            .and_then(|index| self.transcript.entries.get(*index))
            .and_then(|entry| match &entry.kind {
                EntryKind::Tool(tool) => Some(tool.title.clone()),
                _ => None,
            })
            .unwrap_or_default();
        if let Some(change) = activity::record(&self.transcript, call, &title, report, now) {
            // `record` only makes changes that fit this transcript.
            let _ = self.transcript.apply(&change);
        }
    }
}

/// A record's time, in Unix milliseconds.
fn timestamp(record: &Value) -> Option<u64> {
    let time = chrono::DateTime::parse_from_rfc3339(string(record, "timestamp")).ok()?;
    u64::try_from(time.timestamp_millis()).ok()
}
