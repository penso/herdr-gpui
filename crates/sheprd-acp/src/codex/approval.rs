//! Codex's approval requests as ACP permission requests, and the answers
//! back.

use acp::v1 as wire;
use serde_json::{Value, json};

use crate::harness::out;

/// A question waiting for its answer: what each option tells Codex.
#[derive(Debug)]
pub(super) struct Ask {
    /// Codex's request id, answered with the reply.
    pub request: Value,
    choices: Vec<(String, Value)>,
    /// What a dropped question tells Codex.
    dropped: Value,
}

/// The tool call and options to offer for a server request, and the
/// question that keeps them; `None` for a request this program does not
/// answer.
pub(super) fn ask(
    method: &str,
    request: Value,
    params: &Value,
) -> Option<(wire::ToolCallUpdate, Vec<wire::PermissionOption>, Ask)> {
    let field = |key: &str| params.get(key).and_then(Value::as_str);
    let call = field("itemId").unwrap_or("approval").to_owned();
    let reason = field("reason").map(str::to_owned);
    let decision = |decision: &str| json!({"decision": decision});
    // Titles say what the request does; Codex's own reason only follows,
    // so it cannot pass a request off as something else.
    let because = |content: &mut Vec<wire::ToolCallContent>| {
        if let Some(reason) = &reason {
            content.push(out::text(&format!("Codex says: {reason}")));
        }
    };
    let mut content = Vec::new();
    let (title, options, dropped) = match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            let title = match (method, field("command")) {
                (_, Some(command)) => {
                    content.push(out::text(command));
                    because(&mut content);
                    out::command_title(command)
                }
                // The call already shows the change's diff, which content
                // here would replace.
                ("item/fileChange/requestApproval", None) => match field("grantRoot") {
                    Some(root) => format!("Codex asks to write under {root}"),
                    None => "Codex asks to apply its changes".to_owned(),
                },
                _ => {
                    because(&mut content);
                    "Codex asks to run a command".to_owned()
                }
            };
            let options = vec![
                choice(
                    "accept",
                    "Yes",
                    wire::PermissionOptionKind::AllowOnce,
                    decision("accept"),
                ),
                choice(
                    "session",
                    "Yes, for this session",
                    wire::PermissionOptionKind::AllowAlways,
                    decision("acceptForSession"),
                ),
                choice(
                    "decline",
                    "No",
                    wire::PermissionOptionKind::RejectOnce,
                    decision("decline"),
                ),
            ];
            (title, options, decision("cancel"))
        }
        "item/permissions/requestApproval" => {
            let requested = params.get("permissions").cloned().unwrap_or(Value::Null);
            let grant = |scope: &str| json!({"permissions": requested, "scope": scope});
            // More access than one call takes is a person's call.
            let options = vec![
                manual(
                    "turn",
                    "Yes, for this turn",
                    wire::PermissionOptionKind::AllowOnce,
                    grant("turn"),
                ),
                manual(
                    "session",
                    "Yes, for this session",
                    wire::PermissionOptionKind::AllowAlways,
                    grant("session"),
                ),
                choice(
                    "decline",
                    "No",
                    wire::PermissionOptionKind::RejectOnce,
                    json!({"permissions": {}, "scope": "turn"}),
                ),
            ];
            let refused = json!({"permissions": {}, "scope": "turn"});
            content.push(out::text(&access(&requested)));
            because(&mut content);
            ("Codex asks for more access".to_owned(), options, refused)
        }
        "item/tool/requestUserInput" => question(params)?,
        _ => return None,
    };
    let (options, choices) = options.into_iter().unzip();
    let mut fields = wire::ToolCallUpdateFields::new().title(Some(title));
    if !content.is_empty() {
        fields = fields.content(Some(content));
    }
    Some((
        wire::ToolCallUpdate::new(call, fields),
        options,
        Ask {
            request,
            choices,
            dropped,
        },
    ))
}

impl Ask {
    /// The JSON-RPC response for the session's answer.
    pub(super) fn answer(self, result: &Result<Value, wire::Error>) -> Value {
        let reply = out::selected(result)
            .and_then(|id| {
                self.choices
                    .into_iter()
                    .find(|(known, _)| *known == id)
                    .map(|(_, reply)| reply)
            })
            .unwrap_or(self.dropped);
        json!({"id": self.request, "result": reply})
    }
}

/// What an extra-access request grants, as the user reads it.
fn access(requested: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(network) = requested
        .get("network")
        .filter(|network| !network.is_null())
    {
        lines.push(format!("Network: {}", compact(network)));
    }
    if let Some(files) = requested.get("fileSystem").filter(|files| !files.is_null()) {
        lines.push(format!("Files: {}", compact(files)));
    }
    if lines.is_empty() {
        lines.push(format!("Access: {}", compact(requested)));
    }
    lines.join("\n")
}

/// A value as one bounded line of JSON.
fn compact(value: &Value) -> String {
    const MAX: usize = 2000;
    let mut text = value.to_string();
    if text.len() > MAX {
        let mut cut = MAX;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push('…');
    }
    text
}

type Offered = (wire::PermissionOption, (String, Value));

fn choice(id: &str, name: &str, kind: wire::PermissionOptionKind, reply: Value) -> Offered {
    (
        wire::PermissionOption::new(id.to_owned(), name, kind),
        (id.to_owned(), reply),
    )
}

/// A choice only a person may make; see [`out::manual`].
fn manual(id: &str, name: &str, kind: wire::PermissionOptionKind, reply: Value) -> Offered {
    let (option, reply) = choice(id, name, kind, reply);
    (out::manual(option), reply)
}

/// One question with choices becomes the options; anything else is left to
/// the chat, answered with nothing.
fn question(params: &Value) -> Option<(String, Vec<Offered>, Value)> {
    let unanswered = json!({"answers": {}});
    let questions = params.get("questions")?.as_array()?;
    let [only] = questions.as_slice() else {
        let options = vec![choice(
            "decline",
            "Answer in the chat",
            wire::PermissionOptionKind::RejectOnce,
            unanswered.clone(),
        )];
        return Some(("Codex has questions".to_owned(), options, unanswered));
    };
    let id = only.get("id")?.as_str()?;
    let asked = only
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or("Codex asks");
    let mut options: Vec<Offered> = only
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| option.get("label").and_then(Value::as_str))
        .enumerate()
        .map(|(index, label)| {
            manual(
                &format!("answer-{index}"),
                label,
                wire::PermissionOptionKind::AllowOnce,
                json!({"answers": {id: {"answers": [label]}}}),
            )
        })
        .collect();
    options.push(choice(
        "decline",
        "Answer in the chat",
        wire::PermissionOptionKind::RejectOnce,
        unanswered.clone(),
    ));
    Some((asked.to_owned(), options, unanswered))
}
