//! Claude Code hook payloads in, hook decisions out. Shapes follow
//! <https://code.claude.com/docs/en/hooks>; unknown events are recorded by name
//! and answered with no decision, so newer Claude Code versions keep working.

use crate::{
    broker::{ASK_USER_QUESTION, Decision, EventKind, Origin, PendingRequest},
    transcript::{clip, tool_summary},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Text copied into the event feed is clipped so one long turn cannot crowd
/// out the rest of the bounded feed.
const MAX_EVENT_TEXT: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
struct Common {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    transcript_path: Option<PathBuf>,
    hook_event_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "hook_event_name")]
enum Payload {
    PermissionRequest {
        tool_name: String,
        #[serde(default)]
        tool_input: Value,
        #[serde(default)]
        tool_use_id: Option<String>,
    },
    PreToolUse {
        tool_name: String,
        #[serde(default)]
        tool_input: Value,
    },
    PostToolUse {
        tool_name: String,
        #[serde(default)]
        tool_input: Value,
    },
    PostToolUseFailure {
        tool_name: String,
        #[serde(default)]
        tool_input: Value,
    },
    Notification {
        #[serde(default)]
        notification_type: Option<String>,
        #[serde(default)]
        message: String,
    },
    Stop {
        #[serde(default)]
        last_assistant_message: Option<String>,
    },
    UserPromptSubmit {
        #[serde(default)]
        prompt: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Hook {
    pub(crate) origin: Origin,
    /// Only absolute `.jsonl` paths are kept; anything else is not a
    /// transcript the companion should open.
    pub(crate) transcript_path: Option<PathBuf>,
    pub(crate) action: Action,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Wait for a decision from the phone.
    Permission {
        tool_name: String,
        tool_input: Value,
        tool_use_id: Option<String>,
    },
    /// Record the event and answer at once.
    Record(EventKind),
}

impl Hook {
    pub(crate) fn parse(body: &[u8], pane_id: Option<&str>) -> crate::Result<Self> {
        let value: Value = serde_json::from_slice(body)?;
        let common = Common::deserialize(&value)?;
        let origin = Origin {
            session_id: common.session_id,
            pane_id: pane_id.filter(|pane| !pane.is_empty()).map(str::to_owned),
            cwd: common.cwd,
        };
        let transcript_path = common.transcript_path.filter(|path| {
            path.is_absolute() && path.extension().is_some_and(|ext| ext == "jsonl")
        });
        let tool = |tool_name: String, tool_input: &Value, failed: Option<bool>| {
            let summary = tool_summary(&tool_name, tool_input);
            match failed {
                None => EventKind::ToolStarted { tool_name, summary },
                Some(failed) => EventKind::ToolFinished {
                    tool_name,
                    summary,
                    failed,
                },
            }
        };
        let kind = match Payload::deserialize(value)? {
            Payload::PermissionRequest {
                tool_name,
                tool_input,
                tool_use_id,
            } => {
                let action = Action::Permission {
                    tool_name,
                    tool_input,
                    tool_use_id,
                };
                return Ok(Self {
                    origin,
                    transcript_path,
                    action,
                });
            }
            Payload::PreToolUse {
                tool_name,
                tool_input,
            } => tool(tool_name, &tool_input, None),
            Payload::PostToolUse {
                tool_name,
                tool_input,
            } => tool(tool_name, &tool_input, Some(false)),
            Payload::PostToolUseFailure {
                tool_name,
                tool_input,
            } => tool(tool_name, &tool_input, Some(true)),
            Payload::Notification {
                notification_type,
                message,
            } => EventKind::Notification {
                notification_type,
                message: clip(&message, MAX_EVENT_TEXT),
            },
            Payload::Stop {
                last_assistant_message,
            } => EventKind::Stopped {
                last_assistant_message: last_assistant_message
                    .map(|text| clip(&text, MAX_EVENT_TEXT)),
            },
            Payload::UserPromptSubmit { prompt } => EventKind::PromptSubmitted {
                prompt: clip(&prompt, MAX_EVENT_TEXT),
            },
            Payload::Other => EventKind::Hook {
                hook_event_name: common.hook_event_name,
            },
        };
        Ok(Self {
            origin,
            transcript_path,
            action: Action::Record(kind),
        })
    }
}

/// The `PermissionRequest` hook response for a decision, or `None` to let
/// Claude Code continue to its own prompt.
pub(crate) fn permission_output(request: &PendingRequest, decision: Decision) -> Option<Value> {
    let decision = match decision {
        Decision::Terminal => return None,
        // Claude Code before 2.1.207 denied an allow without `updatedInput`,
        // so the original input is echoed when the phone did not change it.
        Decision::Allow {
            updated_input,
            updated_permissions,
        } => {
            let mut decision = json!({
                "behavior": "allow",
                "updatedInput": updated_input.unwrap_or_else(|| request.tool_input.clone()),
            });
            if !updated_permissions.is_empty() {
                decision["updatedPermissions"] = json!(updated_permissions);
            }
            decision
        }
        Decision::Answer { answers } => {
            debug_assert_eq!(request.tool_name, ASK_USER_QUESTION);
            json!({
                "behavior": "allow",
                "updatedInput": {
                    "questions": request.tool_input.get("questions").cloned().unwrap_or(Value::Null),
                    "answers": answers,
                },
            })
        }
        Decision::Deny { message } => json!({ "behavior": "deny", "message": message }),
    };
    Some(json!({
        "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": decision,
        }
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn request(tool_name: &str, tool_input: Value) -> PendingRequest {
        PendingRequest {
            id: 1,
            origin: Origin::default(),
            tool_name: tool_name.into(),
            tool_input,
            tool_use_id: None,
        }
    }

    fn parse(body: &Value, pane: Option<&str>) -> Hook {
        Hook::parse(body.to_string().as_bytes(), pane).unwrap()
    }

    fn recorded(body: &Value) -> EventKind {
        match parse(body, None).action {
            Action::Record(kind) => kind,
            action => panic!("expected an event, got {action:?}"),
        }
    }

    #[test]
    fn parses_a_permission_request_with_its_pane_and_transcript() {
        let body = json!({
            "session_id": "abc", "cwd": "/w", "hook_event_name": "PermissionRequest",
            "transcript_path": "/home/u/.claude/projects/w/abc.jsonl",
            "tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "t1",
            "permission_mode": "default"
        });
        assert_eq!(
            parse(&body, Some("p_3")),
            Hook {
                origin: Origin {
                    session_id: "abc".into(),
                    pane_id: Some("p_3".into()),
                    cwd: Some("/w".into()),
                },
                transcript_path: Some("/home/u/.claude/projects/w/abc.jsonl".into()),
                action: Action::Permission {
                    tool_name: "Bash".into(),
                    tool_input: json!({"command": "ls"}),
                    tool_use_id: Some("t1".into()),
                },
            }
        );
    }

    #[test]
    fn keeps_only_absolute_jsonl_transcripts() {
        for path in ["relative.jsonl", "/etc/passwd", ""] {
            let body =
                json!({"session_id": "s", "hook_event_name": "Stop", "transcript_path": path});
            assert_eq!(parse(&body, None).transcript_path, None, "{path}");
        }
    }

    #[test]
    fn an_empty_pane_header_means_no_pane() {
        // Claude Code interpolates an unset `$HERDR_PANE_ID` as an empty string.
        let body = json!({"session_id": "s", "hook_event_name": "Stop"});
        let hook = parse(&body, Some(""));
        assert_eq!(hook.origin.pane_id, None);
        assert_eq!(
            hook.action,
            Action::Record(EventKind::Stopped {
                last_assistant_message: None
            })
        );
    }

    #[test]
    fn tool_hooks_become_progress_events() {
        let event = |name: &str| json!({"session_id": "s", "hook_event_name": name, "tool_name": "Bash", "tool_input": {"command": "cargo test"}});
        assert_eq!(
            recorded(&event("PreToolUse")),
            EventKind::ToolStarted {
                tool_name: "Bash".into(),
                summary: "cargo test".into()
            }
        );
        assert_eq!(
            recorded(&event("PostToolUse")),
            EventKind::ToolFinished {
                tool_name: "Bash".into(),
                summary: "cargo test".into(),
                failed: false
            }
        );
        assert_eq!(
            recorded(&event("PostToolUseFailure")),
            EventKind::ToolFinished {
                tool_name: "Bash".into(),
                summary: "cargo test".into(),
                failed: true
            }
        );
    }

    #[test]
    fn unknown_events_are_recorded_by_name() {
        let body = json!({"session_id": "s", "hook_event_name": "PreCompact", "trigger": "auto"});
        assert_eq!(
            recorded(&body),
            EventKind::Hook {
                hook_event_name: "PreCompact".into()
            }
        );
    }

    #[test]
    fn long_text_is_clipped_on_a_char_boundary() {
        let message = "é".repeat(MAX_EVENT_TEXT);
        let body =
            json!({"session_id": "s", "hook_event_name": "Notification", "message": message});
        let EventKind::Notification { message, .. } = recorded(&body) else {
            panic!("Notification is an event");
        };
        assert!(message.len() <= MAX_EVENT_TEXT + '…'.len_utf8());
        assert!(message.ends_with('…'));
    }

    #[test]
    fn malformed_payloads_are_errors() {
        assert!(Hook::parse(b"not json", None).is_err());
        assert!(Hook::parse(br#"{"session_id":"s"}"#, None).is_err());
        assert!(Hook::parse(br#"{"hook_event_name":"PermissionRequest"}"#, None).is_err());
    }

    #[test]
    fn allow_echoes_the_original_input_unless_changed() {
        let request = request("Bash", json!({"command": "ls"}));
        let output = permission_output(
            &request,
            Decision::Allow {
                updated_input: None,
                updated_permissions: vec!["Bash(ls *)".into()],
            },
        )
        .unwrap();
        assert_eq!(
            output,
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {
                "behavior": "allow", "updatedInput": {"command": "ls"}, "updatedPermissions": ["Bash(ls *)"]
            }}})
        );
    }

    #[test]
    fn answers_pass_the_questions_back_with_the_answers() {
        let questions =
            json!([{"question": "Which?", "header": "Pick", "options": [], "multiSelect": false}]);
        let request = request(ASK_USER_QUESTION, json!({"questions": questions}));
        let mut answers = Map::new();
        answers.insert("Which?".into(), json!("A"));
        let output = permission_output(&request, Decision::Answer { answers }).unwrap();
        assert_eq!(
            output["hookSpecificOutput"]["decision"],
            json!({"behavior": "allow", "updatedInput": {"questions": questions, "answers": {"Which?": "A"}}})
        );
    }

    #[test]
    fn deny_carries_its_message_and_terminal_renders_no_decision() {
        let request = request("Bash", json!({}));
        let output = permission_output(
            &request,
            Decision::Deny {
                message: "not now".into(),
            },
        )
        .unwrap();
        assert_eq!(
            output["hookSpecificOutput"]["decision"],
            json!({"behavior": "deny", "message": "not now"})
        );
        assert_eq!(permission_output(&request, Decision::Terminal), None);
    }
}
