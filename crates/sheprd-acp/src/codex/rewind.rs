//! Going back to before an earlier prompt: Codex forks the thread through
//! the turn before it, and the chat goes on in the fork. `thread/revert`
//! would keep the thread, but the rollout log a reopened chat is read from
//! keeps the reverted turns; a fork's log holds only what it kept.

use acp::v1 as wire;
use serde_json::{Value, json};

use super::bridge::{Bridge, Pending};
use crate::harness::{Out, out};

/// Turns read to find the prompt: a transcript shows at most this many
/// prompts anyway.
const TURNS: usize = 2000;

impl Bridge {
    /// Asks for the thread's turns, to find the one to go back before.
    pub(super) fn rewind(&mut self, acp: wire::RequestId, params: Value) -> Vec<Out> {
        let (Ok(rewind), Some(thread)) = (
            serde_json::from_value::<out::Rewind>(params),
            self.thread.clone(),
        ) else {
            return vec![out::reply(acp, Err(wire::Error::invalid_params()))];
        };
        vec![self.call(
            "thread/turns/list",
            json!({"threadId": thread, "limit": TURNS}),
            Pending::RewindTurns { acp, rewind },
        )]
    }

    /// Forks the thread through the turn before the prompt. The thread that
    /// is left behind keeps its log; the first prompt leaves nothing to
    /// fork, so a new thread starts.
    pub(super) fn rewind_turns(
        &mut self,
        acp: wire::RequestId,
        rewind: &out::Rewind,
        result: Result<Value, String>,
    ) -> Vec<Out> {
        let turns = match result {
            Ok(turns) => turns,
            Err(message) => return vec![out::reply(acp, Err(out::failure(message)))],
        };
        // Newest first, as `thread/turns/list` gives them by default.
        let mut turns: Vec<&Value> = turns
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .collect();
        turns.reverse();
        let texts: Vec<&str> = turns.iter().map(|turn| prompt(turn)).collect();
        let Some(found) = rewind.find(texts.into_iter()) else {
            let message = "Codex's thread does not have that prompt.";
            return vec![out::reply(acp, Err(out::failure(message)))];
        };
        let cwd = self.cwd.clone();
        let call = match found.checked_sub(1).and_then(|last| turns.get(last)) {
            Some(last) => {
                let last = last.get("id").and_then(Value::as_str).unwrap_or_default();
                let thread = self.thread.clone().unwrap_or_default();
                let params = json!({
                    "cwd": cwd,
                    "excludeTurns": true,
                    "lastTurnId": last,
                    "threadId": thread,
                });
                ("thread/fork", params)
            }
            None => ("thread/start", json!({"cwd": cwd})),
        };
        vec![self.call(call.0, call.1, Pending::Rewound(acp))]
    }

    /// The fork is open: what Codex says now is about it.
    pub(super) fn rewound(&mut self, acp: wire::RequestId, result: Result<Value, String>) -> Out {
        let id = match &result {
            Ok(thread) => thread.pointer("/thread/id").and_then(Value::as_str),
            Err(_) => None,
        };
        match (id, &result) {
            (Some(id), _) => {
                self.thread = Some(id.to_owned());
                out::reply(acp, Ok(json!({"sessionId": id})))
            }
            (None, Err(message)) => out::reply(acp, Err(out::failure(message.clone()))),
            (None, Ok(_)) => out::reply(acp, Err(out::failure("Codex opened no thread."))),
        }
    }
}

/// The text the user typed in a turn, as the first text of its user message.
fn prompt(turn: &Value) -> &str {
    turn.get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("type").and_then(Value::as_str) == Some("userMessage"))
        .and_then(|item| item.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .and_then(|block| block.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
}
