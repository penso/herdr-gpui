//! A usable terminal connection can still fail the local Git trust checks.
//! Keep that diagnosis separate from transport failure until the UI presents it.
use super::UntrustedEndpoint;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalPeerWarning {
    reason: UntrustedEndpoint,
    socket: PathBuf,
}

impl LocalPeerWarning {
    pub(crate) fn new(reason: UntrustedEndpoint, socket: PathBuf) -> Self {
        Self { reason, socket }
    }

    pub(crate) fn notice(&self, now: std::time::Instant) -> crate::notifications::Notice {
        use herdr_client::protocol::{SemanticNotification, SemanticNotificationKind};
        crate::notifications::Notice::local_feedback_multiline(
            SemanticNotification {
                kind: SemanticNotificationKind::NeedsAttention,
                title: "Local Git and PR details unavailable".into(),
                body: Some(self.body()),
                sound: None,
                agent: None,
                workspace_id: None,
                tab_id: None,
                pane_id: None,
                position: None,
            },
            now,
        )
    }

    fn body(&self) -> String {
        use UntrustedEndpoint::*;
        let path = match self.reason {
            DirectoryPermissions => self.socket.parent().unwrap_or(&self.socket),
            _ => &self.socket,
        };
        let advice = match self.reason {
            DirectoryPermissions | SocketPermissions => {
                let (mode, access) = match self.reason {
                    DirectoryPermissions => ("o-w", "world-write"),
                    _ => ("go-w", "group/other write"),
                };
                // Do not turn a lossy, sanitized, or truncated path into a command.
                let command = path
                    .to_str()
                    .filter(|path| {
                        path.len() <= 200 && !path.chars().any(crate::notifications::unsafe_char)
                    })
                    .map(herdr_client::shell_quote)
                    .filter(|quoted| quoted.len() <= 200);
                match command {
                    Some(path) => format!(
                        "Check ownership and remove {access} access, then reconnect the GUI.\nchmod -- {mode} {path}"
                    ),
                    None => format!(
                        "Make it owned by your user, remove {access} access, then reconnect the GUI."
                    ),
                }
            }
            PeerUser => "Run the local daemon as your user, then reconnect the GUI.".into(),
            NotSocket | OtherSocket => {
                "Connect to the standard local session to enable Git and PR details.".into()
            }
        };
        format!("{} {advice}", self.reason)
    }
}

#[cfg(test)]
mod tests;
