//! `remote-files`: the copies of SSH hosts' files that link clicks fetched
//! into the window, read from what the window already keeps.
use crate::{
    control::protocol::{
        MAX_MESSAGE, Response,
        hosts::{CopyState, RemoteCopy, Reply},
    },
    remote_files::{Listed, Policy, SyncState},
};

/// Room for copies in one answer, short of the socket's message bound.
pub(super) const LIST_BUDGET: usize = MAX_MESSAGE - 4096;

impl From<SyncState> for CopyState {
    fn from(state: SyncState) -> Self {
        match state {
            SyncState::Synced => Self::Synced,
            SyncState::Edited { .. } => Self::Edited,
            SyncState::Asking { .. } => Self::Asking,
            SyncState::Uploading { .. } => Self::Uploading,
            SyncState::Conflict { .. } => Self::Conflict,
            SyncState::Declined { .. } => Self::Declined,
            SyncState::Refetching { .. } => Self::Refetching,
        }
    }
}

impl From<Listed<'_>> for RemoteCopy {
    fn from(listed: Listed<'_>) -> Self {
        Self {
            host: listed.target.to_owned(),
            path: listed.remote_path.to_owned(),
            local: listed.local.to_string_lossy().into_owned(),
            state: listed.state.into(),
            auto_upload: listed.policy == Policy::Always,
        }
    }
}

/// The copies, by host and path, that fit `budget` bytes of JSON, and how
/// many more did not.
pub(super) fn list<'a>(listed: impl IntoIterator<Item = Listed<'a>>, budget: usize) -> Response {
    let mut all: Vec<RemoteCopy> = listed.into_iter().map(RemoteCopy::from).collect();
    all.sort_by(|a, b| (&a.host, &a.path).cmp(&(&b.host, &b.path)));
    let total = all.len();
    let mut used = 0;
    let copies: Vec<RemoteCopy> = all
        .into_iter()
        .take_while(|copy| {
            used += serde_json::to_vec(copy).map_or(budget, |json| json.len() + 1);
            used <= budget
        })
        .collect();
    let omitted = total - copies.len();
    Response::Host {
        reply: Reply::RemoteFiles { copies, omitted },
    }
}
