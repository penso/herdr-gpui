//! A usable terminal connection can still fail the local Git trust checks.
//! Keep that diagnosis separate from transport failure until the UI presents it.
use super::UntrustedEndpoint;
use crate::i18n::fl;
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
        let (title, body) = (fl!("daemon-warning-title"), self.body());
        crate::notifications::Notice::local_feedback_multiline(
            SemanticNotification {
                kind: SemanticNotificationKind::NeedsAttention,
                title: title.clone(),
                body: Some(body.clone()),
                sound: None,
                agent: None,
                workspace_id: None,
                tab_id: None,
                pane_id: None,
                position: None,
            },
            now,
        )
        // The cleanup above is for daemon text. This is the app's own: Fluent
        // messages, whose direction marks a right-to-left language needs, and
        // a command whose path `body` already refused when it was unsafe.
        .with_message(title, body)
    }

    fn body(&self) -> String {
        use UntrustedEndpoint::*;
        let path = match self.reason {
            DirectoryPermissions => self.socket.parent().unwrap_or(&self.socket),
            _ => &self.socket,
        };
        // Do not turn a lossy, sanitized, or truncated path into a command.
        let quoted = path
            .to_str()
            .filter(|path| {
                path.len() <= 200 && !path.chars().any(crate::notifications::unsafe_char)
            })
            .map(herdr_client::shell_quote)
            .filter(|quoted| quoted.len() <= 200);
        let command = if quoted.is_some() { "yes" } else { "no" };
        let advice = match self.reason {
            DirectoryPermissions => fl!("daemon-warning-directory-permissions", command = command),
            SocketPermissions => fl!("daemon-warning-socket-permissions", command = command),
            PeerUser => fl!("daemon-warning-peer-user"),
            NotSocket => fl!("daemon-warning-not-socket"),
            OtherSocket => fl!("daemon-warning-other-socket"),
        };
        // The command is the shell's, the same in every language.
        let mode = match self.reason {
            DirectoryPermissions => Some("o-w"),
            SocketPermissions => Some("go-w"),
            PeerUser | NotSocket | OtherSocket => None,
        };
        match (mode, quoted) {
            (Some(mode), Some(path)) => format!("{advice}\nchmod -- {mode} {path}"),
            _ => advice,
        }
    }
}

#[cfg(test)]
mod tests;
