//! `chat status`, `chat show`, `chat hide`, and `chat toggle`.
use crate::control::protocol::{Call, ErrorCode, Response, chat::ChatOp};
use gpui::App;

pub(super) fn serve(call: Call<ChatOp>, cx: &mut App) -> Response {
    use super::{in_window, not_found};
    use crate::control::protocol::chat::Reply;

    let Call { caller, op } = call;
    match op {
        ChatOp::Status => in_window(&caller, cx, |view, _, _| Response::Chat {
            reply: Reply::Chats {
                chats: info::infos(view),
            },
        }),
        ChatOp::New { agent, pane } => {
            let Ok(agent) = agent.parse::<herdr_chat_protocol::Agent>() else {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    format!(
                        "Unknown agent {agent}; known agents are {}",
                        herdr_chat_protocol::Agent::names()
                    ),
                );
            };
            let beside = pane.or_else(|| caller.pane_id.clone());
            in_window(&caller, cx, move |view, _, cx| {
                match view.start_chat(agent, beside.as_deref()) {
                    Ok(()) => {
                        cx.notify();
                        Response::Chat {
                            reply: Reply::Started {
                                agent: agent.name().into(),
                            },
                        }
                    }
                    Err(error) => Response::error(start_code(&error), error.to_string()),
                }
            })
        }
        ChatOp::View { switch, pane } => in_window(&caller, cx, |view, window, cx| {
            let pane = pane.or_else(|| caller.pane_id.clone()).or_else(|| {
                view.live
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.focused_pane_id.clone())
            });
            let Some(pane) = pane else {
                return Response::error(ErrorCode::InvalidRequest, "Name a pane with --pane");
            };
            if !view.chats.pane(&pane).is_some_and(|chat| chat.available()) {
                return not_found(format!("Pane {pane} has no chat this window can show"));
            }
            let shown = switch.apply(view.chats.shown(&pane));
            view.set_chat_raw(&pane, !shown, window, cx);
            Response::Chat {
                reply: Reply::View {
                    pane_id: pane,
                    shown,
                },
            }
        }),
    }
}

/// How a chat that did not start is reported to the caller.
fn start_code(error: &crate::chat::StartError) -> ErrorCode {
    use crate::chat::StartError;
    match error {
        StartError::AgentMissing(_)
        | StartError::Disabled
        | StartError::NotLocal
        | StartError::ProgramMissing => ErrorCode::Unsupported,
        StartError::NoWorkspace => ErrorCode::NotFound,
        StartError::SlotBusy | StartError::ProgramUnknown => ErrorCode::Busy,
        StartError::NotConnected
        | StartError::Request(_)
        | StartError::Response
        | StartError::Timeout => ErrorCode::Failed,
    }
}

mod info {
    use crate::{
        HerdrWindow,
        control::protocol::chat::{ChatInfo, InputMode},
    };
    use herdr_chat_protocol::Status;

    pub(super) fn infos(view: &HerdrWindow) -> Vec<ChatInfo> {
        view.chats
            .panes()
            .map(|chat| {
                let transcript = chat.session.transcript();
                let mut status = match transcript.status {
                    Status::Starting => "starting",
                    Status::Idle => "idle",
                    Status::Working => "working",
                    Status::Failed { .. } => "failed",
                    Status::Exited { .. } => "exited",
                };
                if chat.target.existing.is_some()
                    && view.live.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.agents.iter().any(|agent| {
                            agent.pane_id == chat.target.pane_id
                                && agent.agent_status
                                    == herdr_client::protocol::AgentStatus::Blocked
                        })
                    })
                {
                    status = "blocked";
                }
                ChatInfo {
                    pane_id: chat.target.pane_id.clone(),
                    agent: chat.session.agent().to_owned(),
                    input_mode: if chat.target.existing.is_some() {
                        InputMode::Terminal
                    } else if transcript.read_only {
                        InputMode::ReadOnly
                    } else {
                        InputMode::Chat
                    },
                    session_id: transcript
                        .session
                        .as_ref()
                        .map(|session| session.id.clone()),
                    status: status.into(),
                    attached: chat.attached(),
                    shown: view.chats.shown(&chat.target.pane_id),
                    permissions: transcript.permissions.len(),
                    queued: transcript.queued.len(),
                    model: transcript
                        .option(herdr_chat_protocol::OptionCategory::Model)
                        .map(herdr_chat_protocol::ConfigOption::current_name),
                    context: transcript
                        .usage
                        .as_ref()
                        .filter(|usage| usage.size > 0)
                        .map(herdr_chat_protocol::Usage::percent),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
