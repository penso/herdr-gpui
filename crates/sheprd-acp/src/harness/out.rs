//! The ACP messages every translator sends the session.

use acp::v1 as wire;
use serde_json::Value;

use super::Out;
use crate::rpc::Inbound;

/// A `session/update` for `session`.
pub(crate) fn update(session: &str, update: wire::SessionUpdate) -> Out {
    let notification = wire::SessionNotification::new(session.to_owned(), update);
    Out::Session(Inbound::Notification {
        method: "session/update".to_owned(),
        params: serde_json::to_value(notification).unwrap_or(Value::Null),
    })
}

/// The answer to the session's request `id`.
pub(crate) fn reply(id: wire::RequestId, result: Result<Value, wire::Error>) -> Out {
    Out::Session(Inbound::Response { id, result })
}

/// A request of the harness's own the session answers, such as a permission
/// question.
pub(crate) fn ask(id: i64, method: &str, params: &impl serde::Serialize) -> Out {
    Out::Session(Inbound::Request {
        id: wire::RequestId::Number(id),
        method: method.to_owned(),
        params: serde_json::to_value(params).unwrap_or(Value::Null),
    })
}

/// A piece of the agent's answer, or of its reasoning when `thought`.
pub(crate) fn chunk(session: &str, thought: bool, text: &str, message: Option<&str>) -> Out {
    let chunk = wire::ContentChunk::new(wire::ContentBlock::Text(wire::TextContent::new(text)))
        .message_id(message.map(wire::MessageId::new));
    update(
        session,
        if thought {
            wire::SessionUpdate::AgentThoughtChunk(chunk)
        } else {
            wire::SessionUpdate::AgentMessageChunk(chunk)
        },
    )
}

/// Something the session should say, such as a harness's warning.
pub(crate) fn notice(
    session: &str,
    severity: wire::NoticeSeverity,
    text: impl Into<String>,
) -> Out {
    update(
        session,
        wire::SessionUpdate::Notice(wire::Notice::new(severity, text)),
    )
}

pub(crate) fn to_value(value: &impl serde::Serialize) -> Result<Value, wire::Error> {
    serde_json::to_value(value).map_err(|_| wire::Error::internal_error())
}

pub(crate) fn failure(message: impl Into<String>) -> wire::Error {
    wire::Error::new(-32603, message)
}

/// A text block of tool call content.
pub(crate) fn text(text: &str) -> wire::ToolCallContent {
    wire::ToolCallContent::Content(wire::Content::new(wire::ContentBlock::Text(
        wire::TextContent::new(text),
    )))
}

/// The capabilities every translator offers: pictures and files in prompts,
/// and continuing a session.
pub(crate) fn initialized() -> Result<Value, wire::Error> {
    let capabilities = wire::AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(
            wire::PromptCapabilities::new()
                .image(true)
                .embedded_context(true),
        );
    to_value(
        &wire::InitializeResponse::new(acp::ProtocolVersion::V1).agent_capabilities(capabilities),
    )
}

/// A select setting, for a model or a reasoning level.
pub(crate) fn select(
    id: &str,
    name: &str,
    category: wire::SessionConfigOptionCategory,
    current: &str,
    choices: impl IntoIterator<Item = (String, String, Option<String>)>,
) -> wire::SessionConfigOption {
    let choices: Vec<_> = choices
        .into_iter()
        .map(|(value, name, description)| {
            wire::SessionConfigSelectOption::new(value, name).description(description)
        })
        .collect();
    wire::SessionConfigOption::select(id.to_owned(), name, current.to_owned(), choices)
        .category(Some(category))
}

/// The answer to `session/new` or `session/load`: the session, its modes,
/// and its settings.
pub(crate) fn opened(
    session: &str,
    modes: Option<wire::SessionModeState>,
    options: &[wire::SessionConfigOption],
) -> Result<Value, wire::Error> {
    let mut response = to_value(&wire::NewSessionResponse::new(session.to_owned()).modes(modes))?;
    if let Value::Object(fields) = &mut response {
        fields.insert(
            "configOptions".to_owned(),
            serde_json::to_value(options).unwrap_or(Value::Null),
        );
    }
    Ok(response)
}

/// The value id a `session/set_config_option` sets: its setting and value.
pub(crate) fn chosen(params: Value) -> Option<(String, String)> {
    let request = serde_json::from_value::<wire::SetSessionConfigOptionRequest>(params).ok()?;
    let wire::SessionConfigOptionValue::ValueId { value } = request.value else {
        return None;
    };
    Some((request.config_id.0.to_string(), value.0.to_string()))
}

/// The option the session chose for a permission request, if it chose one.
pub(crate) fn selected(result: &Result<Value, wire::Error>) -> Option<String> {
    let value = result.as_ref().ok()?;
    let response = serde_json::from_value::<wire::RequestPermissionResponse>(value.clone()).ok()?;
    match response.outcome {
        wire::RequestPermissionOutcome::Selected(selected) => {
            Some(selected.option_id.0.to_string())
        }
        _ => None,
    }
}

/// A `file://` URI's path, or the URI itself.
pub(crate) fn file_path(uri: &str) -> String {
    url::Url::parse(uri)
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .map_or_else(|| uri.to_owned(), |path| path.display().to_string())
}

/// A file the user attached, as text the model reads.
pub(crate) fn embedded_file(uri: &str, text: &str) -> String {
    format!("<file path=\"{}\">\n{text}\n</file>", file_path(uri))
}

/// A command's first line, saying how many more it has, so a call or a
/// permission request never hides what it runs behind its title.
pub(crate) fn command_title(command: &str) -> String {
    let mut lines = command.lines();
    let first = lines.next().unwrap_or_default();
    match lines.count() {
        0 => format!("`{first}`"),
        1 => format!("`{first}` (+1 more line)"),
        more => format!("`{first}` (+{more} more lines)"),
    }
}

/// The `_meta` key that marks an option only a person may choose.
pub(crate) const MANUAL: &str = "herdr.manual";

/// `option`, marked so auto-accepting never chooses it: an answer to a
/// question, an extension's dialog, or a broader grant than one call.
/// Other ACP clients still see its kind.
pub(crate) fn manual(option: wire::PermissionOption) -> wire::PermissionOption {
    let mut meta = serde_json::Map::new();
    meta.insert(MANUAL.to_owned(), Value::Bool(true));
    option.meta(Some(meta))
}
