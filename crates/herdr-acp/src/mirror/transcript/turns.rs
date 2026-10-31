//! Reported turn boundaries, not inferred process activity. The GUI separately
//! gates the open clock on the daemon's live status. Context use is not output
//! token throughput, and an unfinished historical start never sets Working.

use super::{MAX_ENTRIES, string};
use herdr_chat_protocol::{Tokens, Turn, TurnEnd, TurnSummary};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Turns {
    open: Option<Open>,
    seen: HashSet<String>,
    session: Option<String>,
    total: Option<Tokens>,
}

struct Open {
    baseline: Option<Tokens>,
    id: String,
    latest: Option<Tokens>,
    started_ms: u64,
}

impl Turns {
    pub(super) fn current(&self) -> Option<Turn> {
        self.open.as_ref().map(|turn| Turn {
            started_ms: turn.started_ms,
        })
    }

    pub(super) fn invalidate(&mut self) {
        self.open = None;
        self.total = None;
    }

    pub(super) fn codex(&mut self, record: &Value) -> Option<TurnSummary> {
        let payload = &record["payload"];
        match string(record, "type") {
            "session_meta" => {
                let session = payload.get("session_id").or_else(|| payload.get("id"));
                self.session(session);
            }
            "compacted" => self.invalidate(),
            "turn_context" => {
                if self.open.as_ref().is_some_and(|open| {
                    payload
                        .get("turn_id")
                        .is_none_or(|id| id.as_str() != Some(open.id.as_str()))
                }) {
                    self.invalidate();
                }
            }
            "event_msg" => match string(payload, "type") {
                "task_started" | "turn_started" => {
                    let Some(id) = identifier(payload.get("turn_id")) else {
                        self.invalidate();
                        return None;
                    };
                    if self.seen.contains(id) {
                        return None;
                    }
                    let Some(started_ms) = timestamp(payload.get("started_at"), record) else {
                        self.invalidate();
                        return None;
                    };
                    if self.seen.len() >= MAX_ENTRIES {
                        self.invalidate();
                        return None;
                    }
                    self.seen.insert(id.to_owned());
                    self.open = Some(Open {
                        baseline: self.total,
                        id: id.to_owned(),
                        latest: None,
                        started_ms,
                    });
                }
                "token_count" => {
                    if let Some(value) = payload["info"].get("total_token_usage") {
                        let total = tokens(value);
                        if let Some(open) = &mut self.open {
                            if total.is_none()
                                || self
                                    .total
                                    .zip(total)
                                    .is_some_and(|(old, new)| difference(new, old).is_none())
                            {
                                open.baseline = None;
                            }
                            open.latest = total;
                        }
                        self.total = total;
                    }
                }
                "task_complete" | "turn_complete" | "turn_aborted" => {
                    let id = identifier(payload.get("turn_id"))?;
                    if self.open.as_ref().is_none_or(|open| open.id != id) {
                        return None;
                    }
                    let open = self.open.take()?;
                    let duration_ms = match payload.get("duration_ms") {
                        Some(value) if !value.is_null() => count(value)?,
                        _ => timestamp(payload.get("completed_at"), record)?
                            .checked_sub(open.started_ms)?,
                    };
                    let end = if payload.get("error").is_some_and(|error| !error.is_null()) {
                        TurnEnd::Failed
                    } else if string(payload, "type") == "turn_aborted" {
                        match string(payload, "reason") {
                            "budget_limited" => TurnEnd::Limit,
                            "interrupted" | "replaced" => TurnEnd::Cancelled,
                            _ => TurnEnd::Other,
                        }
                    } else {
                        TurnEnd::Done
                    };
                    let tokens = open
                        .latest
                        .zip(open.baseline)
                        .and_then(|(latest, baseline)| difference(latest, baseline))
                        .filter(|tokens| tokens.total > 0);
                    return Some(TurnSummary {
                        duration_ms,
                        end,
                        tokens,
                    });
                }
                "thread_rolled_back" | "context_compacted" => self.invalidate(),
                _ => {}
            },
            _ => {}
        }
        None
    }

    pub(super) fn claude(&mut self, record: &Value) -> Option<TurnSummary> {
        if record
            .get("isSidechain")
            .is_some_and(|value| value.as_bool() != Some(false))
        {
            return None;
        }
        if let Some(session) = record.get("sessionId") {
            self.session(Some(session));
        }
        if string(record, "type") == "system" && string(record, "subtype") == "turn_duration" {
            // Claude's interactive log reports elapsed time but not whether
            // this turn succeeded, was cancelled, or used a complete token set.
            return Some(TurnSummary {
                duration_ms: count(record.get("durationMs")?)?,
                end: TurnEnd::Other,
                tokens: None,
            });
        }
        None
    }

    fn session(&mut self, value: Option<&Value>) {
        let next = identifier(value).map(str::to_owned);
        if next.is_none() {
            self.invalidate();
            return;
        }
        if self.session != next {
            self.invalidate();
            self.seen.clear();
            self.session = next;
        }
    }
}

fn identifier(value: Option<&Value>) -> Option<&str> {
    value?.as_str().filter(|id| {
        !id.is_empty()
            && id.len() <= 512
            && !id.chars().any(|c| c.is_control() || c.is_whitespace())
    })
}

fn count(value: &Value) -> Option<u64> {
    value.as_u64().filter(|count| *count <= i64::MAX as u64)
}

fn timestamp(seconds: Option<&Value>, record: &Value) -> Option<u64> {
    if let Some(seconds) = seconds.filter(|value| !value.is_null()) {
        return count(seconds)?.checked_mul(1000);
    }
    let value = record
        .get("timestamp")?
        .as_str()
        .filter(|text| text.len() <= 64)?;
    u64::try_from(
        chrono::DateTime::parse_from_rfc3339(value)
            .ok()?
            .timestamp_millis(),
    )
    .ok()
}

fn tokens(value: &Value) -> Option<Tokens> {
    Some(Tokens {
        cached: count(value.get("cached_input_tokens")?)?,
        input: count(value.get("input_tokens")?)?,
        output: count(value.get("output_tokens")?)?,
        thought: count(value.get("reasoning_output_tokens")?)?,
        total: count(value.get("total_tokens")?)?,
    })
}

fn difference(latest: Tokens, baseline: Tokens) -> Option<Tokens> {
    Some(Tokens {
        cached: latest.cached.checked_sub(baseline.cached)?,
        input: latest.input.checked_sub(baseline.input)?,
        output: latest.output.checked_sub(baseline.output)?,
        thought: latest.thought.checked_sub(baseline.thought)?,
        total: latest.total.checked_sub(baseline.total)?,
    })
}
