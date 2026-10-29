//! The translation itself, kept free of threads and pipes: each message from
//! the session or from Claude Code yields the messages to send on.

use std::collections::{HashMap, HashSet};

use acp::v1 as wire;
use serde_json::{Value, json};

use super::{background, background::Background, permission, settings, settings::Settings, tools};
use crate::{
    harness::{
        Out, Translator, out,
        out::{failure, reply, to_value},
    },
    project::Tag,
    rpc::Inbound,
};

/// What a control request this side sent is waiting for.
#[derive(Debug)]
enum Control {
    Effort { acp: wire::RequestId, level: String },
    Initialize,
    Interrupt,
    Mode { acp: wire::RequestId, mode: String },
    Model { acp: wire::RequestId, value: String },
}

/// A tool call of the running turn.
#[derive(Debug)]
struct Call {
    /// Shown otherwise than as a call (the to-do list is the plan).
    hidden: bool,
    name: String,
}

/// Where the handshake stands.
#[derive(Debug)]
enum Handshake {
    Waiting,
    Ready(Settings),
    Failed(String),
}

#[derive(Debug)]
pub(super) struct Bridge {
    asks: HashMap<i64, permission::Ask>,
    background: Background,
    calls: HashMap<String, Call>,
    controls: HashMap<String, Control>,
    handshake: Handshake,
    interrupted: bool,
    /// The message Claude is streaming now.
    message: Option<String>,
    next_control: u64,
    next_request: i64,
    /// `session/new` or `session/load`, answered once Claude's handshake is.
    opening: Option<wire::RequestId>,
    /// The running `session/prompt`, answered by Claude's `result`.
    prompt: Option<wire::RequestId>,
    session: String,
    /// Messages whose text came in pieces, so the whole message that follows
    /// is not shown again.
    streamed: HashSet<String>,
}

impl Bridge {
    pub(super) fn new(session: String) -> Self {
        Self {
            asks: HashMap::new(),
            background: Background::default(),
            calls: HashMap::new(),
            controls: HashMap::new(),
            handshake: Handshake::Waiting,
            interrupted: false,
            message: None,
            next_control: 0,
            next_request: 0,
            opening: None,
            prompt: None,
            session,
            streamed: HashSet::new(),
        }
    }
}

impl Translator for Bridge {
    /// Claude's handshake.
    fn start(&mut self) -> Vec<Out> {
        vec![self.control(
            json!({"subtype": "initialize", "hooks": null}),
            Control::Initialize,
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
        let field = |key: &str| message.get(key).and_then(Value::as_str);
        // A sub-agent's own steps show under its call; its prose stays
        // inside it.
        match message.get("parent_tool_use_id") {
            Some(Value::String(parent)) => return self.subagent(message, parent),
            None | Some(Value::Null) => {}
            Some(_) => return Vec::new(),
        }
        match field("type") {
            Some("control_response") => self.control_response(message.get("response")),
            Some("control_request") => self.control_request(message),
            Some("system") => self.system(message),
            Some("stream_event") => message
                .get("event")
                .map(|event| self.stream(event))
                .unwrap_or_default(),
            Some("assistant") => self.assistant(message.get("message")),
            Some("user") => self.user(message),
            Some("result") => self.result(message),
            _ => Vec::new(),
        }
    }
}

impl Bridge {
    fn request(&mut self, id: wire::RequestId, method: &str, params: Value) -> Vec<Out> {
        match method {
            "initialize" => vec![reply(id, out::initialized())],
            "session/new" | "session/load" => match &self.handshake {
                Handshake::Waiting => {
                    self.opening = Some(id);
                    Vec::new()
                }
                _ => self.opened(id),
            },
            "session/prompt" => self.prompt(id, params),
            "session/set_mode" => {
                let Ok(request) = serde_json::from_value::<wire::SetSessionModeRequest>(params)
                else {
                    return vec![reply(id, Err(wire::Error::invalid_params()))];
                };
                let mode = request.mode_id.0.to_string();
                vec![self.control(
                    json!({"subtype": "set_permission_mode", "mode": mode}),
                    Control::Mode { acp: id, mode },
                )]
            }
            "session/set_config_option" => self.set_option(id, params),
            _ => vec![reply(id, Err(wire::Error::method_not_found()))],
        }
    }

    /// Answers `session/new` or `session/load` from Claude's handshake.
    fn opened(&mut self, id: wire::RequestId) -> Vec<Out> {
        let settings = match &self.handshake {
            Handshake::Ready(settings) => settings,
            Handshake::Failed(message) => {
                return vec![reply(id, Err(failure(message.clone())))];
            }
            Handshake::Waiting => return Vec::new(),
        };
        let response = out::opened(&self.session, Some(settings.modes()), &settings.options());
        vec![reply(id, response)]
    }

    fn prompt(&mut self, id: wire::RequestId, params: Value) -> Vec<Out> {
        let Ok(request) = serde_json::from_value::<wire::PromptRequest>(params) else {
            return vec![reply(id, Err(wire::Error::invalid_params()))];
        };
        if self.prompt.is_some() {
            return vec![reply(id, Err(failure("A turn is already running.")))];
        }
        self.prompt = Some(id);
        self.interrupted = false;
        let content: Vec<Value> = request.prompt.iter().filter_map(super::content).collect();
        vec![Out::Harness(json!({
            "type": "user",
            "message": {"role": "user", "content": content},
            "parent_tool_use_id": null,
            "session_id": self.session,
        }))]
    }

    fn cancel(&mut self) -> Vec<Out> {
        if self.prompt.is_none() || self.interrupted {
            return Vec::new();
        }
        self.interrupted = true;
        vec![self.control(json!({"subtype": "interrupt"}), Control::Interrupt)]
    }

    fn set_option(&mut self, id: wire::RequestId, params: Value) -> Vec<Out> {
        let request = serde_json::from_value::<wire::SetSessionConfigOptionRequest>(params);
        let (Ok(request), Handshake::Ready(settings)) = (request, &self.handshake) else {
            return vec![reply(id, Err(wire::Error::invalid_params()))];
        };
        let wire::SessionConfigOptionValue::ValueId { value } = request.value else {
            return vec![reply(id, Err(wire::Error::invalid_params()))];
        };
        let value = value.0.to_string();
        match request.config_id.0.as_ref() {
            settings::MODEL if settings.has_model(&value) => vec![self.control(
                json!({"subtype": "set_model", "model": value}),
                Control::Model { acp: id, value },
            )],
            settings::EFFORT if settings.takes_effort(&value) => vec![self.control(
                json!({
                    "subtype": "apply_flag_settings",
                    "settings": {"effortLevel": settings::effort_setting(&value)},
                }),
                Control::Effort {
                    acp: id,
                    level: value,
                },
            )],
            _ => vec![reply(id, Err(wire::Error::invalid_params()))],
        }
    }

    fn control(&mut self, request: Value, purpose: Control) -> Out {
        self.next_control += 1;
        let id = format!("herdr_{}", self.next_control);
        self.controls.insert(id.clone(), purpose);
        Out::Harness(json!({"type": "control_request", "request_id": id, "request": request}))
    }

    fn control_response(&mut self, response: Option<&Value>) -> Vec<Out> {
        let Some(response) = response else {
            return Vec::new();
        };
        let Some(control) = response
            .get("request_id")
            .and_then(Value::as_str)
            .and_then(|id| self.controls.remove(id))
        else {
            return Vec::new();
        };
        let result = match response.get("subtype").and_then(Value::as_str) {
            Some("success") => Ok(response.get("response").cloned().unwrap_or(Value::Null)),
            _ => Err(response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Claude refused the request.")
                .to_owned()),
        };
        match control {
            Control::Initialize => {
                self.handshake = match &result {
                    Ok(response) => Handshake::Ready(Settings::from_handshake(response)),
                    Err(message) => {
                        Handshake::Failed(format!("Claude's handshake failed: {message}"))
                    }
                };
                let mut outs = match self.opening.take() {
                    Some(id) => self.opened(id),
                    None => Vec::new(),
                };
                if let Ok(response) = &result {
                    outs.extend(self.commands(response.get("commands")));
                }
                outs
            }
            Control::Interrupt => Vec::new(),
            Control::Mode { acp, mode } => {
                if result.is_ok()
                    && let Handshake::Ready(settings) = &mut self.handshake
                {
                    settings.set_mode(&mode);
                }
                vec![reply(acp, result.map(|_| json!({})).map_err(failure))]
            }
            Control::Model { acp, value } => self.option_set(acp, result, |settings| {
                settings.set_model(&value);
            }),
            Control::Effort { acp, level } => self.option_set(acp, result, |settings| {
                settings.set_effort(&level);
            }),
        }
    }

    fn option_set(
        &mut self,
        acp: wire::RequestId,
        result: Result<Value, String>,
        apply: impl FnOnce(&mut Settings),
    ) -> Vec<Out> {
        let result = match (result, &mut self.handshake) {
            (Ok(_), Handshake::Ready(settings)) => {
                apply(settings);
                Ok(json!({"configOptions": settings.options()}))
            }
            (Ok(_), _) => Err(wire::Error::internal_error()),
            (Err(message), _) => Err(failure(message)),
        };
        vec![reply(acp, result)]
    }

    fn control_request(&mut self, message: &Value) -> Vec<Out> {
        let Some(control) = message.get("request_id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let request = message.get("request").unwrap_or(&Value::Null);
        if request.get("subtype").and_then(Value::as_str) != Some("can_use_tool") {
            return vec![Out::Harness(json!({
                "type": "control_response",
                "response": {
                    "subtype": "error",
                    "request_id": control,
                    "error": "Herdr does not support this request.",
                },
            }))];
        }
        let tool_use = request
            .get("tool_use_id")
            .and_then(Value::as_str)
            .unwrap_or(control);
        let name = request
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let input = request.get("input").cloned().unwrap_or(Value::Null);
        let (options, ask) = permission::ask(control.to_owned(), request);
        self.next_request += 1;
        self.asks.insert(self.next_request, ask);
        let fields = wire::ToolCallUpdateFields::new()
            .title(Some(tools::title(name, &input)))
            .raw_input(Some(input));
        let params = wire::RequestPermissionRequest::new(
            self.session.clone(),
            wire::ToolCallUpdate::new(tool_use.to_owned(), fields),
            options,
        );
        vec![Out::Session(Inbound::Request {
            id: wire::RequestId::Number(self.next_request),
            method: "session/request_permission".to_owned(),
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        })]
    }

    fn system(&mut self, message: &Value) -> Vec<Out> {
        let field = |key: &str| message.get(key).and_then(Value::as_str);
        match field("subtype") {
            Some("init") => {
                let mut outs = self.mode_seen(field("permissionMode"));
                let options = match (field("model"), &mut self.handshake) {
                    (Some(model), Handshake::Ready(settings)) => {
                        settings.observe_model(model).then(|| settings.options())
                    }
                    _ => None,
                };
                if let Some(options) = options {
                    outs.push(self.update(wire::SessionUpdate::ConfigOptionUpdate(
                        wire::ConfigOptionUpdate::new(options),
                    )));
                }
                outs
            }
            Some("status") => self.mode_seen(field("permissionMode")),
            Some("commands_changed") => self.commands(message.get("commands")),
            Some("task_started") => {
                if let Some(task) = field("task_id") {
                    self.background.task_started(task, field("tool_use_id"));
                }
                Vec::new()
            }
            Some("task_notification") => field("task_id")
                .zip(field("status"))
                .and_then(|(task, status)| {
                    self.background
                        .notified(task, field("tool_use_id"), status, field("summary"))
                })
                .map(|elsewhere| self.reported(elsewhere))
                .into_iter()
                .collect(),
            Some("compact_boundary") => {
                vec![self.update(wire::SessionUpdate::Notice(wire::Notice::new(
                    wire::NoticeSeverity::Info,
                    "Claude compacted the conversation.",
                )))]
            }
            _ => Vec::new(),
        }
    }

    /// Follows a mode Claude moved to by itself, such as after a plan was
    /// approved.
    fn mode_seen(&mut self, mode: Option<&str>) -> Vec<Out> {
        let (Some(mode), Handshake::Ready(settings)) = (mode, &mut self.handshake) else {
            return Vec::new();
        };
        if !settings.set_mode(mode) {
            return Vec::new();
        }
        let mode = settings.mode().to_owned();
        vec![self.update(wire::SessionUpdate::CurrentModeUpdate(
            wire::CurrentModeUpdate::new(mode),
        ))]
    }

    fn commands(&self, commands: Option<&Value>) -> Vec<Out> {
        let Some(Value::Array(commands)) = commands else {
            return Vec::new();
        };
        let commands = commands
            .iter()
            .filter_map(|command| {
                let text = |key: &str| command.get(key).and_then(Value::as_str);
                let mut available = wire::AvailableCommand::new(
                    text("name")?,
                    text("description").unwrap_or_default(),
                );
                if let Some(hint) = text("argumentHint").filter(|hint| !hint.is_empty()) {
                    available = available.input(Some(wire::AvailableCommandInput::Unstructured(
                        wire::UnstructuredCommandInput::new(hint),
                    )));
                }
                Some(available)
            })
            .collect();
        vec![self.update(wire::SessionUpdate::AvailableCommandsUpdate(
            wire::AvailableCommandsUpdate::new(commands),
        ))]
    }

    fn stream(&mut self, event: &Value) -> Vec<Out> {
        match event.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                self.message = event
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Vec::new()
            }
            Some("content_block_delta") => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                let text = |key: &str| {
                    delta
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                };
                let (thought, text) = match delta.get("type").and_then(Value::as_str) {
                    Some("text_delta") => (false, text("text")),
                    Some("thinking_delta") => (true, text("thinking")),
                    _ => (false, None),
                };
                let Some(text) = text else {
                    return Vec::new();
                };
                if let Some(message) = &self.message {
                    self.streamed.insert(message.clone());
                }
                vec![self.chunk(thought, text, self.message.clone())]
            }
            _ => Vec::new(),
        }
    }

    fn chunk(&self, thought: bool, text: &str, message: Option<String>) -> Out {
        out::chunk(&self.session, thought, text, message.as_deref())
    }

    fn assistant(&mut self, message: Option<&Value>) -> Vec<Out> {
        let Some(message) = message else {
            return Vec::new();
        };
        let id = message.get("id").and_then(Value::as_str).map(str::to_owned);
        let streamed = id.as_ref().is_some_and(|id| self.streamed.contains(id));
        let mut outs = Vec::new();
        for block in blocks(message) {
            let text = |key: &str| {
                block
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
            };
            match block.get("type").and_then(Value::as_str) {
                Some("text") if !streamed => {
                    outs.extend(text("text").map(|text| self.chunk(false, text, id.clone())));
                }
                Some("thinking") if !streamed => {
                    outs.extend(text("thinking").map(|text| self.chunk(true, text, id.clone())));
                }
                Some("tool_use") => outs.extend(self.tool_use(block, None)),
                _ => {}
            }
        }
        outs
    }

    /// A call as it starts: the plan for the to-do list, else a card,
    /// tagged with the sub-agent it is a step of and the activity it starts.
    fn tool_use(&mut self, block: &Value, parent: Option<&str>) -> Option<Out> {
        let text = |key: &str| block.get(key).and_then(Value::as_str);
        let (call, name) = (text("id")?, text("name")?);
        let input = block.get("input").cloned().unwrap_or(Value::Null);
        let hidden = name == tools::TODOS;
        // A sub-agent's own to-do list is not the conversation's plan.
        if hidden && parent.is_some() {
            return None;
        }
        self.calls.insert(
            call.to_owned(),
            Call {
                hidden,
                name: name.to_owned(),
            },
        );
        if hidden {
            return Some(self.update(wire::SessionUpdate::Plan(tools::plan(&input))));
        }
        let tag = self.background.started(call, name, &input, parent);
        let mut tool_call = tools::call(call, name, &input);
        if tag != Tag::default() {
            tool_call = tool_call.meta(tag.meta());
        }
        Some(self.update(wire::SessionUpdate::ToolCall(tool_call)))
    }

    /// A message of the sub-agent `parent`: its calls and their results.
    fn subagent(&mut self, message: &Value, parent: &str) -> Vec<Out> {
        match message.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let Some(inner) = message.get("message") else {
                    return Vec::new();
                };
                let uses: Vec<Value> = blocks(inner)
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
                    .cloned()
                    .collect();
                uses.iter()
                    .filter_map(|block| self.tool_use(block, Some(parent)))
                    .collect()
            }
            Some("user") => self.results(message),
            _ => Vec::new(),
        }
    }

    /// The update that says how another call's activity stands.
    fn reported(&self, (call, report): background::Elsewhere) -> Out {
        let update = wire::ToolCallUpdate::new(call, wire::ToolCallUpdateFields::new())
            .meta(Tag::activity(report).meta());
        self.update(wire::SessionUpdate::ToolCallUpdate(update))
    }

    fn user(&mut self, envelope: &Value) -> Vec<Out> {
        let Some(message) = envelope.get("message") else {
            return Vec::new();
        };
        if let Some(text) = message.get("content").and_then(Value::as_str) {
            if let Some((task, call, status, summary)) = background::task_notification(text) {
                return self
                    .background
                    .notified(task, call, status, summary)
                    .map(|elsewhere| self.reported(elsewhere))
                    .into_iter()
                    .collect();
            }
            return local_output(text)
                .map(|output| {
                    self.update(wire::SessionUpdate::Notice(wire::Notice::new(
                        wire::NoticeSeverity::Info,
                        output,
                    )))
                })
                .into_iter()
                .collect();
        }
        self.results(envelope)
    }

    /// The tool results in a user message, and what they say about
    /// sub-agents and background shells.
    fn results(&mut self, envelope: &Value) -> Vec<Out> {
        let Some(message) = envelope.get("message") else {
            return Vec::new();
        };
        let results = blocks(message)
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
            .count();
        // Claude Code's structured result belongs to the message's one result.
        let structured = envelope.get("tool_use_result").filter(|_| results == 1);
        let mut outs = Vec::new();
        for block in blocks(message) {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(call) = self.calls.get(id).filter(|call| !call.hidden) else {
                continue;
            };
            let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
            let output = block.get("content").unwrap_or(&Value::Null);
            let name = call.name.clone();
            let (report, elsewhere) = self
                .background
                .result(id, &name, failed, output, structured);
            let fields = wire::ToolCallUpdateFields::new()
                .status(Some(if failed {
                    wire::ToolCallStatus::Failed
                } else {
                    wire::ToolCallStatus::Completed
                }))
                .content(tools::result(&name, output, failed));
            let mut update = wire::ToolCallUpdate::new(id.to_owned(), fields);
            if let Some(report) = report {
                update = update.meta(Tag::activity(report).meta());
            }
            outs.push(self.update(wire::SessionUpdate::ToolCallUpdate(update)));
            outs.extend(elsewhere.map(|elsewhere| self.reported(elsewhere)));
        }
        outs
    }

    fn result(&mut self, message: &Value) -> Vec<Out> {
        let mut outs: Vec<Out> = usage(message)
            .map(|usage| self.update(wire::SessionUpdate::UsageUpdate(usage)))
            .into_iter()
            .collect();
        let Some(id) = self.prompt.take() else {
            return outs;
        };
        let field = |key: &str| message.get(key).and_then(Value::as_str);
        let failed = message.get("is_error").and_then(Value::as_bool) == Some(true);
        let stop = match (self.interrupted, field("subtype")) {
            (true, _) => Ok(wire::StopReason::Cancelled),
            (false, Some("success")) if !failed => Ok(match field("stop_reason") {
                Some("max_tokens") => wire::StopReason::MaxTokens,
                Some("refusal") => wire::StopReason::Refusal,
                _ => wire::StopReason::EndTurn,
            }),
            (false, Some("error_max_turns" | "error_max_budget_usd")) => {
                Ok(wire::StopReason::MaxTurnRequests)
            }
            (false, subtype) => Err(error_text(message, subtype)),
        };
        self.interrupted = false;
        let stopped = self.background.turn_ended();
        outs.extend(
            stopped
                .into_iter()
                .map(|elsewhere| self.reported(elsewhere)),
        );
        self.calls.clear();
        self.streamed.clear();
        self.message = None;
        let result = match stop {
            Ok(stop) => to_value(&wire::PromptResponse::new(stop)).map(|mut response| {
                if let Some(tokens) = tokens(message) {
                    response["usage"] = tokens;
                }
                response
            }),
            Err(message) => Err(failure(message)),
        };
        outs.push(reply(id, result));
        outs
    }

    fn update(&self, update: wire::SessionUpdate) -> Out {
        out::update(&self.session, update)
    }
}

fn blocks(message: &Value) -> impl Iterator<Item = &Value> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// What a local command such as `/model` printed.
fn local_output(text: &str) -> Option<&str> {
    let inner = text
        .strip_prefix("<local-command-stdout>")?
        .strip_suffix("</local-command-stdout>")?
        .trim();
    (!inner.is_empty()).then_some(inner)
}

fn error_text(message: &Value, subtype: Option<&str>) -> String {
    let errors: Vec<&str> = message
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if !errors.is_empty() {
        return errors.join("\n");
    }
    message
        .get("result")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map_or_else(
            || format!("Claude stopped: {}.", subtype.unwrap_or("unknown error")),
            str::to_owned,
        )
}

/// The context in use after a turn, out of the model's window, and what the
/// session has cost so far.
fn usage(message: &Value) -> Option<wire::UsageUpdate> {
    let size = message
        .get("modelUsage")?
        .as_object()?
        .values()
        .filter_map(|model| model.get("contextWindow").and_then(Value::as_u64))
        .max()?;
    let last = message.pointer("/usage/iterations")?.as_array()?.last()?;
    let count = |key: &str| last.get(key).and_then(Value::as_u64).unwrap_or(0);
    let used = count("input_tokens")
        + count("cache_read_input_tokens")
        + count("cache_creation_input_tokens")
        + count("output_tokens");
    let mut update = wire::UsageUpdate::new(used, size);
    if let Some(cost) = message.get("total_cost_usd").and_then(Value::as_f64) {
        update = update.cost(Some(wire::Cost::new(cost, "USD")));
    }
    Some(update)
}

/// The turn's tokens in the shape the session reads from a prompt result.
fn tokens(message: &Value) -> Option<Value> {
    let usage = message.get("usage")?;
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let input = count("input_tokens") + count("cache_creation_input_tokens");
    let cached = count("cache_read_input_tokens");
    let output = count("output_tokens");
    let thought = usage
        .pointer("/output_tokens_details/thinking_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Some(json!({
        "inputTokens": input,
        "outputTokens": output,
        "cachedReadTokens": cached,
        "thoughtTokens": thought,
        "totalTokens": input + cached + output,
    }))
}

#[cfg(test)]
mod tests;
