//! What the agent sends: responses to the session's requests, session updates,
//! and its own requests (permission questions).

use acp::v1 as wire;
use herdr_chat_protocol::{
    Change, NoticeLevel, Permission, PermissionId, PermissionKind, PermissionOption, Status,
    TurnEnd, TurnSummary,
};
use serde_json::Value;

use super::{Asked, Pending, Session, Update, now_ms};
use crate::{project, rpc::Inbound};

impl<R: FnMut(Update)> Session<R> {
    pub(super) fn agent(&mut self, inbound: Inbound) {
        match inbound {
            Inbound::Response { id, result } => {
                let wire::RequestId::Number(id) = id else {
                    return;
                };
                if let Some(pending) = self.pending.remove(&id) {
                    self.response(id, pending, result);
                }
            }
            Inbound::Notification { method, params } => {
                if method == "session/update" {
                    self.session_update(params);
                }
            }
            Inbound::Request { id, method, params } => self.agent_request(id, &method, params),
            Inbound::Closed(Some(error)) => self.fail(&error.to_string()),
            Inbound::Closed(None) => {}
        }
    }

    fn response(&mut self, id: i64, pending: Pending, result: Result<Value, wire::Error>) {
        match pending {
            Pending::Initialize => self.initialized(result),
            Pending::NewSession | Pending::LoadSession => {
                self.session_opened(matches!(pending, Pending::LoadSession), result);
            }
            Pending::Prompt => self.turn_ended(id, result),
            Pending::SetOption => match result {
                Ok(value) => {
                    if let Some(options) = value.get("configOptions") {
                        let options = project::options(Some(options));
                        self.emit(Change::Options { options });
                    }
                }
                Err(error) => {
                    let notice = self.projector.notice(
                        NoticeLevel::Warning,
                        format!("Could not change the setting: {}", error.message),
                    );
                    self.emit(notice);
                }
            },
            Pending::SetMode(mode) => match result {
                Ok(_) => self.emit(Change::CurrentMode { id: mode }),
                Err(error) => {
                    let notice = self.projector.notice(
                        NoticeLevel::Warning,
                        format!("Could not change mode: {}", error.message),
                    );
                    self.emit(notice);
                }
            },
        }
    }

    fn initialized(&mut self, result: Result<Value, wire::Error>) {
        let response = match result.map(serde_json::from_value::<wire::InitializeResponse>) {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                return self.fail(&format!("The agent's handshake was invalid: {error}"));
            }
            Err(error) => {
                return self.fail(&format!(
                    "The agent refused the handshake: {}",
                    error.message
                ));
            }
        };
        let prompts = &response.agent_capabilities.prompt_capabilities;
        self.accepts = crate::files::Accepts {
            embedded: prompts.embedded_context,
            image: prompts.image,
        };
        let cwd = self.config.cwd.clone();
        match self.config.load.clone() {
            Some(id) if response.agent_capabilities.load_session => {
                let id = wire::SessionId::new(id);
                self.acp_session = Some(id.clone());
                self.request(
                    "session/load",
                    &wire::LoadSessionRequest::new(id, cwd),
                    Pending::LoadSession,
                );
            }
            load => {
                if load.is_some() {
                    let notice = self.projector.notice(
                        NoticeLevel::Warning,
                        "This agent cannot continue a session; starting a new one.",
                    );
                    self.emit(notice);
                }
                self.request(
                    "session/new",
                    &wire::NewSessionRequest::new(cwd),
                    Pending::NewSession,
                );
            }
        }
    }

    fn session_opened(&mut self, loaded: bool, result: Result<Value, wire::Error>) {
        let mut value = match result {
            Ok(value) => value,
            Err(error) => {
                let message = if error.code == wire::Error::auth_required().code {
                    match &self.config.login_hint {
                        Some(hint) => format!("{} needs you to sign in: {hint}", self.config.agent),
                        None => format!("{} needs you to sign in first.", self.config.agent),
                    }
                } else {
                    format!("Could not start the session: {}", error.message)
                };
                return self.fail(&message);
            }
        };
        // Read apart from the rest, so a setting of a kind this program does
        // not know cannot fail the whole session.
        let options = value
            .as_object_mut()
            .and_then(|object| object.remove("configOptions"));
        let options = project::options(options.as_ref());
        let modes = if loaded {
            serde_json::from_value::<wire::LoadSessionResponse>(value)
                .map(|response| response.modes)
                .unwrap_or_default()
        } else {
            match serde_json::from_value::<wire::NewSessionResponse>(value) {
                Ok(response) => {
                    self.acp_session = Some(response.session_id);
                    response.modes
                }
                Err(error) => {
                    return self.fail(&format!("The agent's session was invalid: {error}"));
                }
            }
        };
        let Some(id) = self.acp_session.clone() else {
            return;
        };
        self.ready = true;
        self.emit(Change::Session {
            session: Some(project::session(&id, &self.config.cwd)),
        });
        if let Some(modes) = modes {
            self.emit(Change::Modes {
                modes: Some(project::modes(modes)),
            });
        }
        if !options.is_empty() {
            self.emit(Change::Options { options });
        }
        self.projector.close();
        self.status(Status::Idle);
        self.next_queued();
    }

    fn session_update(&mut self, params: Value) {
        let ours = params
            .get("sessionId")
            .and_then(Value::as_str)
            .is_some_and(|id| self.acp_session.as_ref().is_some_and(|ours| *ours.0 == *id));
        let update = params.get("update");
        if ours
            && update.and_then(|update| update.get("sessionUpdate"))
                == Some(&"config_option_update".into())
        {
            let options = project::options(update.and_then(|update| update.get("configOptions")));
            return self.emit(Change::Options { options });
        }
        // An update from a newer protocol revision may not decode; it is
        // skipped rather than ending the session.
        let Ok(notification) = serde_json::from_value::<wire::SessionNotification>(params) else {
            return;
        };
        if self.acp_session.as_ref() != Some(&notification.session_id) {
            return;
        }
        let changes = self.projector.update(&self.transcript, notification.update);
        self.emit_all(changes);
    }

    fn agent_request(&mut self, id: wire::RequestId, method: &str, params: Value) {
        if method != "session/request_permission" {
            // The session advertises no file system, terminal, or elicitation
            // support, so the agent has nothing else to ask for.
            let _ = self
                .outbox
                .respond(&id, Err(wire::Error::method_not_found()));
            return;
        }
        let request = match serde_json::from_value::<wire::RequestPermissionRequest>(params) {
            Ok(request) if self.acp_session.as_ref() == Some(&request.session_id) => request,
            _ => {
                let _ = self.outbox.respond(&id, Err(wire::Error::invalid_params()));
                return;
            }
        };
        let call_id = request.tool_call.tool_call_id.0.to_string();
        let fields = request.tool_call.fields;
        let title = fields.title.clone();
        let tool = self.projector.tool(&self.transcript, &call_id, |tool| {
            project::apply_fields(tool, fields);
        });
        let title = title
            .or_else(|| match &tool {
                Change::Push { entry } | Change::Replace { entry } => match &entry.kind {
                    herdr_chat_protocol::EntryKind::Tool(tool) => Some(tool.title.clone()),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_else(|| "The agent asks for permission".to_owned());
        self.emit(tool);
        self.next_permission += 1;
        let permission = PermissionId(self.next_permission);
        let options: Vec<PermissionOption> = request
            .options
            .into_iter()
            .map(|option| PermissionOption {
                id: option.option_id.0.to_string(),
                // An option only a person may choose is never an allow that
                // auto-accepting takes.
                kind: match option.kind {
                    _ if option.meta.as_ref().is_some_and(|meta| {
                        meta.get(crate::harness::out::MANUAL) == Some(&Value::Bool(true))
                    }) =>
                    {
                        PermissionKind::Other
                    }
                    wire::PermissionOptionKind::AllowOnce => PermissionKind::AllowOnce,
                    wire::PermissionOptionKind::AllowAlways => PermissionKind::AllowAlways,
                    wire::PermissionOptionKind::RejectOnce => PermissionKind::RejectOnce,
                    wire::PermissionOptionKind::RejectAlways => PermissionKind::RejectAlways,
                    _ => PermissionKind::Other,
                },
                name: option.name,
            })
            .collect();
        self.asked.insert(
            permission,
            Asked {
                options: options.iter().map(|option| option.id.clone()).collect(),
                request: id,
            },
        );
        self.emit(Change::PermissionAsked {
            permission: Permission {
                call_id: Some(call_id),
                id: permission,
                options,
                title,
            },
        });
        self.auto_answer(permission);
    }

    /// Answers one permission request, with an option or as cancelled.
    pub(super) fn resolve(&mut self, permission: PermissionId, option: Option<&str>) {
        let Some(asked) = self.asked.remove(&permission) else {
            return;
        };
        let outcome = match option {
            Some(option) => wire::RequestPermissionOutcome::Selected(
                wire::SelectedPermissionOutcome::new(option.to_owned()),
            ),
            None => wire::RequestPermissionOutcome::Cancelled,
        };
        let response = wire::RequestPermissionResponse::new(outcome);
        let result = serde_json::to_value(response).map_err(|_| wire::Error::internal_error());
        if let Err(error) = self.outbox.respond(&asked.request, result) {
            self.fail(&error.to_string());
        }
        self.emit(Change::PermissionResolved { id: permission });
    }

    pub(super) fn resolve_all(&mut self) {
        let mut asked: Vec<PermissionId> = self.asked.keys().copied().collect();
        asked.sort();
        for permission in asked {
            self.resolve(permission, None);
        }
    }

    fn turn_ended(&mut self, id: i64, result: Result<Value, wire::Error>) {
        if self.turn != Some(id) {
            return;
        }
        self.turn = None;
        self.resolve_all();
        self.projector.close();
        let tokens = result.as_ref().ok().and_then(project::tokens);
        let (end, tokens, error) = match result.map(serde_json::from_value::<wire::PromptResponse>)
        {
            Ok(Ok(response)) => {
                let end = match response.stop_reason {
                    wire::StopReason::EndTurn => TurnEnd::Done,
                    wire::StopReason::Cancelled => TurnEnd::Cancelled,
                    wire::StopReason::MaxTokens | wire::StopReason::MaxTurnRequests => {
                        TurnEnd::Limit
                    }
                    wire::StopReason::Refusal => TurnEnd::Refused,
                    _ => TurnEnd::Other,
                };
                (end, tokens, None)
            }
            Ok(Err(_)) => (TurnEnd::Other, None, None),
            Err(error) => (TurnEnd::Failed, None, Some(error.message)),
        };
        if let Some(error) = error {
            let notice = self.projector.notice(NoticeLevel::Error, error);
            self.emit(notice);
        }
        let started = self
            .transcript
            .turn
            .map_or_else(now_ms, |turn| turn.started_ms);
        let summary = self.projector.turn(TurnSummary {
            duration_ms: now_ms().saturating_sub(started),
            end,
            tokens,
        });
        self.emit(summary);
        self.emit(Change::Turn { turn: None });
        if matches!(self.transcript.status, Status::Working) {
            self.status(Status::Idle);
        }
        self.next_queued();
    }

    pub(super) fn exited(&mut self, code: Option<i32>, stderr: &[String]) {
        let summary = match code {
            Some(0) => format!("{} exited.", self.config.agent),
            Some(code) => format!("{} exited with status {code}.", self.config.agent),
            None => format!("{} was stopped.", self.config.agent),
        };
        let mut message = summary.clone();
        if !stderr.is_empty() && code != Some(0) {
            message.push('\n');
            message.push_str(&stderr.join("\n"));
        }
        self.asked.clear();
        let pending: Vec<PermissionId> = self.transcript.permissions.iter().map(|p| p.id).collect();
        for id in pending {
            self.emit(Change::PermissionResolved { id });
        }
        self.turn = None;
        self.ready = false;
        if self.transcript.turn.is_some() {
            self.emit(Change::Turn { turn: None });
        }
        self.clear_queue();
        let level = if code == Some(0) {
            NoticeLevel::Info
        } else {
            NoticeLevel::Error
        };
        let notice = self.projector.notice(level, message);
        self.emit(notice);
        self.status(Status::Exited { message: summary });
    }
}
