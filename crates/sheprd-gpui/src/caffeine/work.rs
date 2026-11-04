//! Whether a window's agents are still working, as its daemons report them.

use sheprd_client::protocol::{AgentStatus, ClientShellSnapshot};

/// What one window's agents are doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    Working,
    /// Every agent is idle, blocked on the user, or done.
    Idle,
    /// No agent is working, but an endpoint has no snapshot yet or is
    /// reconnecting, so its agents cannot be read.
    Unknown,
}

impl Work {
    /// Takes each enabled endpoint's snapshot, `None` while it is not
    /// connected. A blocked agent waits on the user, so it keeps nothing awake.
    pub(crate) fn of<'a>(
        snapshots: impl IntoIterator<Item = Option<&'a ClientShellSnapshot>>,
    ) -> Self {
        let mut work = Self::Idle;
        for snapshot in snapshots {
            match snapshot {
                Some(snapshot)
                    if snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.agent_status == AgentStatus::Working) =>
                {
                    return Self::Working;
                }
                Some(_) => {}
                None => work = Self::Unknown,
            }
        }
        work
    }
}
