use super::*;
use crate::hub::{ChatRow, Hub, Layout, TerminalPane, Workspace};
use herdr_chat_protocol::{EndpointId, PermissionId};
use herdr_client::protocol::AgentStatus;

mod agent_name;
mod create;
mod fixtures;
mod hosts;
#[cfg(unix)]
mod live_chat;
mod messages;
mod pictures;
mod projection;
mod reducer;
mod socket;
mod term_scroll;
mod term_size;
mod views;

/// A layout with one workspace holding chat `p1` and terminal `t2`.
pub(super) fn layout(endpoint: EndpointId) -> Layout {
    Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "herdr-gpui".into(),
            branch: Some("main".into()),
            cwd: "/work".into(),
            worktree: None,
            chats: vec![ChatRow {
                pane: "p1".into(),
                tab: "t1".into(),
                endpoint,
                agent: Some("claude".into()),
                title: "Equalize splits".into(),
                status: AgentStatus::Working,
            }],
            terminals: vec![crate::hub::Terminal {
                tab: "t2".into(),
                label: "zsh".into(),
                cwd: Some("/work".into()),
                panes: vec![TerminalPane {
                    pane: "p2".into(),
                    label: None,
                    cwd: Some("/work".into()),
                    focused: true,
                }],
                zoomed: false,
                agent: Some(crate::hub::TerminalAgent {
                    name: Some("claude".into()),
                    status: AgentStatus::Done,
                    pane: "p2".into(),
                    title: Some("Fix the build".into()),
                }),
            }],
        }],
    }
}

/// A server state showing `layout`, with no daemon or chat behind it.
pub(super) fn offline(dir: &std::path::Path, layout: Layout) -> Shared {
    crate::web::tests::state_with(dir, Hub::offline(layout))
}

/// A permission request with an allow and a reject option.
pub(super) fn permission(id: u64) -> herdr_chat_protocol::Permission {
    use herdr_chat_protocol::{PermissionKind, PermissionOption};
    herdr_chat_protocol::Permission {
        call_id: Some("call-1".into()),
        id: PermissionId(id),
        options: vec![
            PermissionOption {
                id: "allow".into(),
                kind: PermissionKind::AllowOnce,
                name: "Allow once".into(),
            },
            PermissionOption {
                id: "reject".into(),
                kind: PermissionKind::RejectOnce,
                name: "Deny".into(),
            },
        ],
        title: "Run `git push origin fix/mise`?".into(),
    }
}

/// The session a phone driving `run` directly holds.
pub(super) struct Session {
    pub(super) state: Shared,
    pub(super) device: DeviceId,
    pub(super) to_live: mpsc::Sender<ClientMessage>,
    pub(super) from_live: mpsc::Receiver<ServerMessage>,
}

impl Session {
    pub(super) fn start(state: Shared) -> Self {
        let (device, token) = crate::web::tests::signed_in(&state, 7);
        let (to_live, inbound) = mpsc::channel(INBOUND_QUEUE);
        let (outbound, from_live) = mpsc::channel(OUTBOUND_QUEUE);
        tokio::spawn(live::run(
            state.clone(),
            token,
            device.clone(),
            inbound,
            outbound,
        ));
        Self {
            state,
            device,
            to_live,
            from_live,
        }
    }

    pub(super) async fn send(&self, message: ClientMessage) {
        self.to_live.send(message).await.unwrap();
    }

    /// The next message, within five seconds.
    pub(super) async fn next(&mut self) -> ServerMessage {
        tokio::time::timeout(Duration::from_secs(5), self.from_live.recv())
            .await
            .expect("no message in time")
            .expect("the session ended")
    }
}
