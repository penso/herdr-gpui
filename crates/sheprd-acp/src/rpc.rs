//! JSON-RPC 2.0 over the agent's standard input and output, one message per line.
//!
//! The reader thread classifies each line and hands it on; the writer thread
//! writes queued lines in order. Correlating responses with requests is the
//! session's job, since it is the only one that knows what each request was for.

use std::{
    io::{BufRead, BufReader, Read, Write},
    thread,
};

use acp::v1 as wire;
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{Error, Result};

/// The longest line the agent may send.
pub(crate) const MAX_LINE: usize = 16 * 1024 * 1024;
/// Lines waiting to be written to the agent.
const OUTBOX: usize = 1024;

#[derive(Debug)]
pub(crate) enum Inbound {
    Request {
        id: wire::RequestId,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: wire::RequestId,
        result: std::result::Result<Value, wire::Error>,
    },
    /// The agent closed its output, or sent a line too long to read.
    Closed(Option<Error>),
}

/// Classifies one line; `None` for anything that is not a JSON-RPC message.
pub(crate) fn classify(line: &[u8]) -> Option<Inbound> {
    let Value::Object(mut message) = serde_json::from_slice::<Value>(line).ok()? else {
        return None;
    };
    let id = message
        .remove("id")
        .and_then(|id| serde_json::from_value::<wire::RequestId>(id).ok());
    let params = message.remove("params").unwrap_or(Value::Null);
    if let Some(Value::String(method)) = message.remove("method") {
        return Some(match id {
            Some(id) => Inbound::Request { id, method, params },
            None => Inbound::Notification { method, params },
        });
    }
    let id = id?;
    if let Some(error) = message.remove("error") {
        let error = serde_json::from_value(error).unwrap_or_else(|_| wire::Error::internal_error());
        return Some(Inbound::Response {
            id,
            result: Err(error),
        });
    }
    Some(Inbound::Response {
        id,
        result: Ok(message.remove("result").unwrap_or(Value::Null)),
    })
}

/// Reads the agent's output on its own thread until it ends or `deliver`
/// reports nobody is listening.
pub(crate) fn spawn_reader(
    output: impl Read + Send + 'static,
    mut deliver: impl FnMut(Inbound) -> bool + Send + 'static,
) -> std::io::Result<thread::JoinHandle<()>> {
    spawn_lines(output, "acp-reader", move |read| match read {
        Lines::Line(line) => classify(line).is_none_or(&mut deliver),
        Lines::End(error) => deliver(Inbound::Closed(error)),
    })
}

/// What [`spawn_lines`] hands on.
pub(crate) enum Lines<'a> {
    Line(&'a [u8]),
    /// The output ended, cleanly or with why it could not be read.
    End(Option<Error>),
}

/// Reads lines of at most [`MAX_LINE`] bytes on a thread named `name`,
/// handing each to `handle` until it returns `false`, then how the output
/// ended unless `handle` stopped first.
pub(crate) fn spawn_lines(
    output: impl Read + Send + 'static,
    name: &str,
    mut handle: impl FnMut(Lines<'_>) -> bool + Send + 'static,
) -> std::io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name(name.to_owned()).spawn(move || {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        let error = loop {
            line.clear();
            let read = (&mut reader)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut line);
            match read {
                Ok(0) => break None,
                Ok(_) if line.len() > MAX_LINE => {
                    break Some(Error::LineTooLong { limit: MAX_LINE });
                }
                Ok(_) => {
                    if !handle(Lines::Line(&line)) {
                        return;
                    }
                }
                Err(error) => break Some(error.into()),
            }
        };
        handle(Lines::End(error));
    })
}

/// Writes queued lines to the agent's input in order, until the queue closes.
pub(crate) fn spawn_writer(
    mut input: impl Write + Send + 'static,
) -> std::io::Result<(Outbox, thread::JoinHandle<()>)> {
    let (sender, receiver): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(OUTBOX);
    let handle = thread::Builder::new()
        .name("acp-writer".to_owned())
        .spawn(move || {
            for line in receiver {
                if input.write_all(&line).and_then(|()| input.flush()).is_err() {
                    return;
                }
            }
        })?;
    Ok((Outbox(sender), handle))
}

/// The queue of lines for the agent. Never blocks: an agent that stops
/// reading its input is reported rather than waited on.
#[derive(Clone, Debug)]
pub(crate) struct Outbox(pub(crate) Sender<Vec<u8>>);

impl Outbox {
    /// Queues one message as a line.
    pub(crate) fn send(&self, message: &Value) -> Result<()> {
        let mut line = serde_json::to_vec(message).map_err(Error::Encode)?;
        line.push(b'\n');
        match self.0.try_send(line) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => Err(Error::AgentStalled),
        }
    }

    pub(crate) fn request(&self, id: i64, method: &str, params: &impl Serialize) -> Result<()> {
        let params = serde_json::to_value(params).map_err(Error::Encode)?;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }

    pub(crate) fn notify(&self, method: &str, params: &impl Serialize) -> Result<()> {
        let params = serde_json::to_value(params).map_err(Error::Encode)?;
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub(crate) fn respond(
        &self,
        id: &wire::RequestId,
        result: std::result::Result<Value, wire::Error>,
    ) -> Result<()> {
        let message = match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        self.send(&message)
    }
}

#[cfg(test)]
mod tests;
