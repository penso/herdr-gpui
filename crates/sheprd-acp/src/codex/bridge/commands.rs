//! Ordered command output projected through bounded ACP replacement updates.

use super::*;
use crate::{
    harness::streaming::Streams,
    project::{MAX_ENTRY_TEXT, bounded},
};

const MAX_COMPLETED: usize = 2000;
const MAX_ITEM_ID: usize = 1024;

#[derive(Debug, Default)]
pub(super) struct Commands {
    pub(super) active: Streams<Output>,
    completed: HashSet<String>,
}

impl Commands {
    pub(super) fn clear(&mut self) {
        self.active.clear();
        self.completed.clear();
    }

    pub(super) fn complete(&mut self, id: &str) {
        self.active.remove(id);
        if self.completed.len() < MAX_COMPLETED && !id.is_empty() && id.len() <= MAX_ITEM_ID {
            self.completed.insert(id.to_owned());
        }
    }

    pub(super) fn next_tick(&self) -> Option<Instant> {
        self.active.next_tick()
    }

    pub(super) fn flush(&mut self, session: &str) -> Vec<Out> {
        self.active.flush(|id, output| output.render(session, id))
    }
}

#[derive(Debug)]
pub(super) struct Output {
    command: Option<String>,
    text: String,
    truncated: bool,
}

impl Output {
    fn new(item: &Value) -> Self {
        Self {
            command: item
                .get("command")
                .and_then(Value::as_str)
                .filter(|command| command.lines().nth(1).is_some())
                .map(items::bounded_text),
            text: String::new(),
            truncated: false,
        }
    }

    fn append(&mut self, delta: &str) -> bool {
        if self.truncated || delta.is_empty() {
            return false;
        }
        let room = MAX_ENTRY_TEXT.saturating_sub(self.text.len());
        // Keep at most one extra character so the shared limiter can mark a cut.
        let end = delta.floor_char_boundary(room + 4);
        self.text.push_str(&delta[..end]);
        self.truncated = delta.len() > room;
        self.text = bounded(std::mem::take(&mut self.text));
        true
    }

    fn render(&self, session: &str, id: &str) -> Out {
        let mut content: Vec<_> = self.command.as_deref().map(out::text).into_iter().collect();
        content.push(out::text(&self.text));
        let fields = wire::ToolCallUpdateFields::new()
            .status(Some(wire::ToolCallStatus::InProgress))
            .content(Some(content));
        out::update(
            session,
            wire::SessionUpdate::ToolCallUpdate(wire::ToolCallUpdate::new(id.to_owned(), fields)),
        )
    }
}

impl Bridge {
    pub(super) fn item_started(&mut self, params: &Value) -> Vec<Out> {
        let Some(item) = params.get("item") else {
            return Vec::new();
        };
        if item.get("type").and_then(Value::as_str) == Some("commandExecution") {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                return Vec::new();
            };
            if !self.current_command(params)
                || id.is_empty()
                || id.len() > MAX_ITEM_ID
                || self.commands.completed.contains(id)
                || self.commands.completed.len() >= MAX_COMPLETED
                || self.commands.active.contains(id)
            {
                return Vec::new();
            }
            // Excess concurrent commands still get ordinary start/final updates.
            self.commands.active.insert(id, Output::new(item));
        }
        let tag = self.background.started(item);
        items::started(item)
            .map(|call| {
                let call = if tag == Tag::default() {
                    call
                } else {
                    call.meta(tag.meta())
                };
                out::update(self.session(), wire::SessionUpdate::ToolCall(call))
            })
            .into_iter()
            .collect()
    }

    pub(super) fn current_command(&self, params: &Value) -> bool {
        self.prompt.is_some()
            && self.thread.is_some()
            && self.turn.is_some()
            && params.get("threadId").and_then(Value::as_str) == self.thread.as_deref()
            && params.get("turnId").and_then(Value::as_str) == self.turn.as_deref()
    }

    pub(super) fn command_output(&mut self, params: &Value) -> Vec<Out> {
        if !self.current_command(params) {
            return Vec::new();
        }
        let (Some(id), Some(delta)) = (
            params.get("itemId").and_then(Value::as_str),
            params.get("delta").and_then(Value::as_str),
        ) else {
            return Vec::new();
        };
        let session = self.session().to_owned();
        self.commands
            .active
            .update(
                id,
                Instant::now(),
                |output| output.append(delta),
                |id, output| output.render(&session, id),
            )
            .into_iter()
            .collect()
    }
}
