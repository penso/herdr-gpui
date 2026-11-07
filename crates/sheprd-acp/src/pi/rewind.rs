//! Going back to before an earlier prompt: pi forks a new session that ends
//! just before a user message (`get_fork_messages`, `fork`, and `get_state`
//! in its `docs/rpc.md`), and the chat goes on in it.

use acp::v1 as wire;
use serde_json::{Value, json};

use super::bridge::{Bridge, Pending};
use crate::harness::{Out, out};

impl Bridge {
    /// Asks for the user messages pi can fork from.
    pub(super) fn rewind(&mut self, acp: wire::RequestId, params: Value) -> Vec<Out> {
        let Ok(rewind) = serde_json::from_value::<out::Rewind>(params) else {
            return vec![out::reply(acp, Err(wire::Error::invalid_params()))];
        };
        vec![self.command(
            json!({"type": "get_fork_messages"}),
            Pending::RewindMessages { acp, rewind },
        )]
    }

    /// Forks from the prompt's message.
    pub(super) fn rewind_messages(
        &mut self,
        acp: wire::RequestId,
        rewind: &out::Rewind,
        data: &Value,
        failed: Option<String>,
    ) -> Vec<Out> {
        if let Some(error) = failed {
            return vec![out::reply(acp, Err(out::failure(error)))];
        }
        let messages: Vec<&Value> = data
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .collect();
        let text = |message: &&Value| {
            message
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let texts: Vec<String> = messages.iter().map(text).collect();
        let entry = rewind
            .find(texts.iter().map(String::as_str))
            .and_then(|found| messages.get(found))
            .and_then(|message| message.get("entryId"))
            .and_then(Value::as_str);
        let Some(entry) = entry else {
            let message = "pi's session does not have that prompt.";
            return vec![out::reply(acp, Err(out::failure(message)))];
        };
        let command = json!({"type": "fork", "entryId": entry});
        vec![self.command(command, Pending::RewindFork(acp))]
    }

    /// Reads the new session's id, unless an extension refused the fork.
    pub(super) fn rewind_forked(
        &mut self,
        acp: wire::RequestId,
        data: &Value,
        failed: Option<String>,
    ) -> Vec<Out> {
        let refused = (data.get("cancelled") == Some(&Value::Bool(true)))
            .then(|| "A pi extension refused to fork the session.".to_owned());
        if let Some(error) = failed.or(refused) {
            return vec![out::reply(acp, Err(out::failure(error)))];
        }
        vec![self.command(json!({"type": "get_state"}), Pending::RewindState(acp))]
    }

    /// The fork is open: what pi says now is about it.
    pub(super) fn rewound(
        &mut self,
        acp: wire::RequestId,
        data: &Value,
        failed: Option<String>,
    ) -> Vec<Out> {
        if let Some(error) = failed {
            return vec![out::reply(acp, Err(out::failure(error)))];
        }
        let Some(session) = data.get("sessionId").and_then(Value::as_str) else {
            return vec![out::reply(acp, Err(out::failure("pi named no session.")))];
        };
        session.clone_into(&mut self.session);
        vec![out::reply(acp, Ok(json!({"sessionId": session})))]
    }
}
