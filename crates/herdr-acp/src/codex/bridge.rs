//! The translation between the session's ACP and Codex's app server, free
//! of threads and pipes.

use std::collections::{HashMap, HashSet};

use acp::v1 as wire;
use serde_json::{Value, json};

use super::{approval, background, background::Background, items, settings::Settings};
use crate::{
    harness::{Out, Translator, out},
    project::Tag,
    rpc::Inbound,
};

/// What a request to Codex is waiting for.
#[derive(Debug)]
enum Pending {
    Initialize,
    Models,
    /// `thread/start` or `thread/resume`, answering `session/new` or `load`.
    Thread(wire::RequestId),
    /// `turn/start`, answering nothing itself: the turn ends the prompt.
    Turn,
    Interrupt,
}

/// The session's request to open a thread, waiting for the handshake.
#[derive(Debug)]
struct Opening {
    acp: wire::RequestId,
    cwd: String,
    resume: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct Bridge {
    asks: HashMap<i64, approval::Ask>,
    background: Background,
    /// Codex's `model/list`, kept until the thread opens.
    models: Option<Value>,
    next_id: i64,
    next_request: i64,
    opening: Option<Opening>,
    pending: HashMap<i64, Pending>,
    /// A thread opened before `model/list` answered, finished once it has.
    opened: Option<(wire::RequestId, Value)>,
    /// The running `session/prompt`, answered when its turn completes.
    prompt: Option<wire::RequestId>,
    ready: bool,
    settings: Settings,
    /// Items whose text came in pieces, so their whole text is not shown
    /// again.
    streamed: HashSet<String>,
    thread: Option<String>,
    /// The turn's tokens, from the last usage update.
    tokens: Option<Value>,
    turn: Option<String>,
    /// The session asked to cancel before the turn had an id.
    interrupting: bool,
}

impl Translator for Bridge {
    fn start(&mut self) -> Vec<Out> {
        vec![self.call(
            "initialize",
            json!({
                "clientInfo": {"name": "herdr-acp", "title": "Herdr", "version": env!("CARGO_PKG_VERSION")},
                "capabilities": {"experimentalApi": false, "requestAttestation": false},
            }),
            Pending::Initialize,
        )]
    }

    fn on_session(&mut self, inbound: Inbound) -> Vec<Out> {
        match inbound {
            Inbound::Request { id, method, params } => self.request(id, &method, params),
            Inbound::Notification { method, .. } if method == "session/cancel" => self.cancel(),
            Inbound::Response { id, result } => {
                let wire::RequestId::Number(id) = id else {
                    return Vec::new();
                };
                self.asks
                    .remove(&id)
                    .map(|ask| Out::Harness(ask.answer(&result)))
                    .into_iter()
                    .collect()
            }
            Inbound::Notification { .. } | Inbound::Closed(_) => Vec::new(),
        }
    }

    fn on_harness(&mut self, message: &Value) -> Vec<Out> {
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").filter(|id| !id.is_null());
        let params = message.get("params").unwrap_or(&Value::Null);
        match (method, id) {
            (Some(method), Some(id)) => self.server_request(method, id.clone(), params),
            (Some(method), None) => self.notification(method, params),
            (None, Some(id)) => self.response(id, message),
            (None, None) => Vec::new(),
        }
    }
}

impl Bridge {
    fn call(&mut self, method: &str, params: Value, pending: Pending) -> Out {
        self.next_id += 1;
        self.pending.insert(self.next_id, pending);
        Out::Harness(json!({"id": self.next_id, "method": method, "params": params}))
    }

    fn session(&self) -> &str {
        self.thread.as_deref().unwrap_or_default()
    }

    fn request(&mut self, id: wire::RequestId, method: &str, params: Value) -> Vec<Out> {
        let field = |key: &str| params.get(key).and_then(Value::as_str).map(str::to_owned);
        match method {
            "initialize" => vec![out::reply(id, out::initialized())],
            "session/new" | "session/load" => {
                self.opening = Some(Opening {
                    acp: id,
                    cwd: field("cwd").unwrap_or_default(),
                    resume: (method == "session/load")
                        .then(|| field("sessionId"))
                        .flatten(),
                });
                self.open()
            }
            "session/prompt" => self.prompt(id, &params),
            "session/set_mode" => {
                let mode = field("modeId").unwrap_or_default();
                let result = if self.settings.set_mode(&mode) {
                    Ok(json!({}))
                } else {
                    Err(wire::Error::invalid_params())
                };
                vec![out::reply(id, result)]
            }
            "session/set_config_option" => {
                let result = match out::chosen(params) {
                    Some((setting, value)) if self.settings.set(&setting, &value) => {
                        Ok(json!({"configOptions": self.settings.options()}))
                    }
                    _ => Err(wire::Error::invalid_params()),
                };
                vec![out::reply(id, result)]
            }
            _ => vec![out::reply(id, Err(wire::Error::method_not_found()))],
        }
    }

    /// Opens the thread once the handshake is done and a session asked.
    fn open(&mut self) -> Vec<Out> {
        if !self.ready {
            return Vec::new();
        }
        let Some(opening) = self.opening.take() else {
            return Vec::new();
        };
        let mut outs = Vec::new();
        if self.models.is_none() {
            outs.push(self.call("model/list", json!({}), Pending::Models));
        }
        let (method, params) = match &opening.resume {
            Some(thread) => (
                "thread/resume",
                json!({"threadId": thread, "cwd": opening.cwd}),
            ),
            None => ("thread/start", json!({"cwd": opening.cwd})),
        };
        outs.push(self.call(method, params, Pending::Thread(opening.acp)));
        outs
    }

    fn prompt(&mut self, id: wire::RequestId, params: &Value) -> Vec<Out> {
        let Some(thread) = self.thread.clone() else {
            return vec![out::reply(id, Err(wire::Error::invalid_params()))];
        };
        if self.prompt.is_some() {
            return vec![out::reply(
                id,
                Err(out::failure("A turn is already running.")),
            )];
        }
        let Ok(request) = serde_json::from_value::<wire::PromptRequest>(params.clone()) else {
            return vec![out::reply(id, Err(wire::Error::invalid_params()))];
        };
        self.prompt = Some(id);
        self.interrupting = false;
        self.tokens = None;
        let input: Vec<Value> = request.prompt.iter().filter_map(input).collect();
        let mut turn = self.settings.turn();
        turn.insert("threadId".to_owned(), json!(thread));
        turn.insert("input".to_owned(), Value::Array(input));
        vec![self.call("turn/start", Value::Object(turn), Pending::Turn)]
    }

    fn cancel(&mut self) -> Vec<Out> {
        if self.prompt.is_none() {
            return Vec::new();
        }
        match (self.thread.clone(), self.turn.clone()) {
            (Some(thread), Some(turn)) => vec![self.call(
                "turn/interrupt",
                json!({"threadId": thread, "turnId": turn}),
                Pending::Interrupt,
            )],
            // The turn's id comes with its start; it is interrupted then.
            _ => {
                self.interrupting = true;
                Vec::new()
            }
        }
    }

    fn response(&mut self, id: &Value, message: &Value) -> Vec<Out> {
        let Some(pending) = id.as_i64().and_then(|id| self.pending.remove(&id)) else {
            return Vec::new();
        };
        let result = match message.get("error") {
            Some(error) => Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Codex refused the request.")
                .to_owned()),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        match pending {
            Pending::Initialize => match result {
                Ok(_) => {
                    self.ready = true;
                    let mut outs = vec![Out::Harness(json!({"method": "initialized"}))];
                    outs.extend(self.open());
                    outs
                }
                Err(message) => self
                    .opening
                    .take()
                    .map(|opening| {
                        out::reply(
                            opening.acp,
                            Err(out::failure(format!("Codex's handshake failed: {message}"))),
                        )
                    })
                    .into_iter()
                    .collect(),
            },
            Pending::Models => {
                // Without a list the session still opens, with no model setting.
                self.models = Some(result.unwrap_or(Value::Null));
                self.opened
                    .take()
                    .map(|(acp, thread)| self.thread_opened(acp, &thread))
                    .into_iter()
                    .collect()
            }
            Pending::Thread(acp) => match result {
                Ok(thread) if self.models.is_none() => {
                    self.opened = Some((acp, thread));
                    Vec::new()
                }
                Ok(thread) => vec![self.thread_opened(acp, &thread)],
                Err(message) => vec![out::reply(acp, Err(out::failure(message)))],
            },
            Pending::Turn => match result {
                Ok(started) => self.turn_started(started.pointer("/turn/id")),
                Err(message) => self
                    .prompt
                    .take()
                    .map(|id| out::reply(id, Err(out::failure(message))))
                    .into_iter()
                    .collect(),
            },
            Pending::Interrupt => Vec::new(),
        }
    }

    fn thread_opened(&mut self, acp: wire::RequestId, thread: &Value) -> Out {
        let id = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.thread = Some(id.to_owned());
        self.settings = Settings::new(self.models.as_ref().unwrap_or(&Value::Null), thread);
        out::reply(
            acp,
            out::opened(id, Some(self.settings.modes()), &self.settings.options()),
        )
    }

    fn turn_started(&mut self, turn: Option<&Value>) -> Vec<Out> {
        let Some(turn) = turn.and_then(Value::as_str) else {
            return Vec::new();
        };
        self.turn = Some(turn.to_owned());
        if std::mem::take(&mut self.interrupting) {
            return self.cancel();
        }
        Vec::new()
    }

    fn server_request(&mut self, method: &str, id: Value, params: &Value) -> Vec<Out> {
        let Some((call, options, ask)) = approval::ask(method, id.clone(), params) else {
            return vec![Out::Harness(json!({
                "id": id,
                "error": {"code": -32601, "message": "Herdr does not support this request."},
            }))];
        };
        self.next_request += 1;
        self.asks.insert(self.next_request, ask);
        let request = wire::RequestPermissionRequest::new(self.session().to_owned(), call, options);
        vec![out::ask(
            self.next_request,
            "session/request_permission",
            &request,
        )]
    }

    fn notification(&mut self, method: &str, params: &Value) -> Vec<Out> {
        let thread = params.get("threadId").and_then(Value::as_str);
        if let Some(thread) = thread.filter(|thread| Some(*thread) != self.thread.as_deref()) {
            return self.child(method, params, thread);
        }
        let session = self.session().to_owned();
        let text = |key: &str| params.get(key).and_then(Value::as_str);
        match method {
            "item/agentMessage/delta"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta" => {
                let (Some(item), Some(delta)) = (
                    text("itemId"),
                    text("delta").filter(|delta| !delta.is_empty()),
                ) else {
                    return Vec::new();
                };
                self.streamed.insert(item.to_owned());
                let thought = method != "item/agentMessage/delta";
                vec![out::chunk(&session, thought, delta, Some(item))]
            }
            "item/started" => {
                let Some(item) = params.get("item") else {
                    return Vec::new();
                };
                let tag = self.background.started(item);
                items::started(item)
                    .map(|call| {
                        let call = if tag == Tag::default() {
                            call
                        } else {
                            call.meta(tag.meta())
                        };
                        out::update(&session, wire::SessionUpdate::ToolCall(call))
                    })
                    .into_iter()
                    .collect()
            }
            "item/completed" => self.item_completed(params.get("item")),
            "turn/started" => self.turn_started(params.pointer("/turn/id")),
            "turn/completed" => self.turn_completed(params.get("turn")),
            "turn/plan/updated" => vec![out::update(
                &session,
                wire::SessionUpdate::Plan(items::plan(params)),
            )],
            "thread/tokenUsage/updated" => self.usage(params.get("tokenUsage")),
            "thread/compacted" => vec![out::notice(
                &session,
                wire::NoticeSeverity::Info,
                "Codex compacted the conversation.",
            )],
            "warning" => text("message")
                .map(|message| out::notice(&session, wire::NoticeSeverity::Warning, message))
                .into_iter()
                .collect(),
            "error" if params.get("willRetry") != Some(&Value::Bool(true)) => params
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(|message| out::notice(&session, wire::NoticeSeverity::Error, message))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    }

    fn item_completed(&mut self, item: Option<&Value>) -> Vec<Out> {
        let Some(item) = item else {
            return Vec::new();
        };
        let session = self.session().to_owned();
        let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
        let streamed = self.streamed.remove(id);
        match item.get("type").and_then(Value::as_str) {
            Some("agentMessage") if !streamed => item
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| out::chunk(&session, false, text, Some(id)))
                .into_iter()
                .collect(),
            Some("reasoning") if !streamed => {
                let summary: Vec<&str> = item
                    .get("summary")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                (!summary.is_empty())
                    .then(|| out::chunk(&session, true, &summary.join("\n\n"), Some(id)))
                    .into_iter()
                    .collect()
            }
            _ => {
                let (own, others) = self.background.completed(item);
                let mut outs: Vec<Out> = items::completed(item)
                    .map(|update| {
                        let update = match own {
                            Some(report) => update.meta(Tag::activity(report).meta()),
                            None => update,
                        };
                        out::update(&session, wire::SessionUpdate::ToolCallUpdate(update))
                    })
                    .into_iter()
                    .collect();
                outs.extend(others.into_iter().map(|other| self.reported(other)));
                outs
            }
        }
    }

    /// An event of another thread: a sub-agent's calls show under the call
    /// that spawned it, and its turn's end ends it. Other threads are not
    /// this session's.
    fn child(&mut self, method: &str, params: &Value, thread: &str) -> Vec<Out> {
        let Some(parent) = self.background.parent(thread).map(str::to_owned) else {
            return Vec::new();
        };
        let session = self.session().to_owned();
        let item = params.get("item");
        match method {
            "item/started" => item
                .and_then(items::started)
                .map(|call| {
                    let tag = Tag {
                        parent: Some(parent),
                        ..Tag::default()
                    };
                    out::update(
                        &session,
                        wire::SessionUpdate::ToolCall(call.meta(tag.meta())),
                    )
                })
                .into_iter()
                .collect(),
            "item/completed" => item
                .and_then(items::completed)
                .map(|update| out::update(&session, wire::SessionUpdate::ToolCallUpdate(update)))
                .into_iter()
                .collect(),
            "turn/completed" => {
                let status = params.pointer("/turn/status").and_then(Value::as_str);
                self.background
                    .child_turn_completed(thread, status)
                    .map(|ended| self.reported(ended))
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// The update that says how another call's activity stands.
    fn reported(&self, (call, report): background::Elsewhere) -> Out {
        let update = wire::ToolCallUpdate::new(call, wire::ToolCallUpdateFields::new())
            .meta(Tag::activity(report).meta());
        out::update(self.session(), wire::SessionUpdate::ToolCallUpdate(update))
    }

    fn usage(&mut self, usage: Option<&Value>) -> Vec<Out> {
        let Some(usage) = usage else {
            return Vec::new();
        };
        let count = |path: &str| usage.pointer(path).and_then(Value::as_u64).unwrap_or(0);
        self.tokens = Some(json!({
            "inputTokens": count("/last/inputTokens").saturating_sub(count("/last/cachedInputTokens")),
            "cachedReadTokens": count("/last/cachedInputTokens"),
            "outputTokens": count("/last/outputTokens"),
            "thoughtTokens": count("/last/reasoningOutputTokens"),
            "totalTokens": count("/last/totalTokens"),
        }));
        let Some(size) = usage.get("modelContextWindow").and_then(Value::as_u64) else {
            return Vec::new();
        };
        let used = count("/last/inputTokens") + count("/last/outputTokens");
        vec![out::update(
            self.session(),
            wire::SessionUpdate::UsageUpdate(wire::UsageUpdate::new(used, size)),
        )]
    }

    fn turn_completed(&mut self, turn: Option<&Value>) -> Vec<Out> {
        let backgrounded = self.background.turn_ended();
        let mut outs: Vec<Out> = backgrounded
            .into_iter()
            .map(|started| self.reported(started))
            .collect();
        let Some(id) = self.prompt.take() else {
            return outs;
        };
        self.turn = None;
        self.interrupting = false;
        self.streamed.clear();
        let status = turn
            .and_then(|turn| turn.get("status"))
            .and_then(Value::as_str);
        let result = match status {
            Some("interrupted") => Ok(wire::StopReason::Cancelled),
            Some("failed") => Err(turn
                .and_then(|turn| turn.pointer("/error/message"))
                .and_then(Value::as_str)
                .unwrap_or("Codex's turn failed.")
                .to_owned()),
            _ => Ok(wire::StopReason::EndTurn),
        };
        let result = match result {
            Ok(stop) => out::to_value(&wire::PromptResponse::new(stop)).map(|mut response| {
                if let Some(tokens) = self.tokens.take() {
                    response["usage"] = tokens;
                }
                response
            }),
            Err(message) => Err(out::failure(message)),
        };
        outs.push(out::reply(id, result));
        outs
    }
}

/// One block of an ACP prompt as Codex's input. A file sent as a link is a
/// mention, which Codex reads itself.
fn input(block: &wire::ContentBlock) -> Option<Value> {
    let text = |text: String| Some(json!({"type": "text", "text": text, "text_elements": []}));
    match block {
        wire::ContentBlock::Text(content) => text(content.text.clone()),
        wire::ContentBlock::Image(image) => Some(json!({
            "type": "image",
            "url": format!("data:{};base64,{}", image.mime_type, image.data),
        })),
        wire::ContentBlock::Resource(resource) => match &resource.resource {
            wire::EmbeddedResourceResource::TextResourceContents(file) => {
                text(out::embedded_file(&file.uri, &file.text))
            }
            _ => None,
        },
        wire::ContentBlock::ResourceLink(link) => Some(json!({
            "type": "mention",
            "name": link.name,
            "path": out::file_path(&link.uri),
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
