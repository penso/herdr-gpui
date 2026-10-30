#![allow(clippy::expect_used, clippy::unwrap_used)]

mod boot_fence;
mod closed;
mod closed_page;
mod hosts;
mod marks;
mod mutes;
mod page;
mod presets;
mod sections;

use super::{Key, Open, Row, State, model::Pr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn key(agent: &str) -> Key {
    Key::Agent {
        endpoint: "local".into(),
        checkout: "herdr".into(),
        agent: agent.into(),
    }
}

fn row(agent: &str, state: State) -> Row {
    Row {
        key: key(agent),
        state,
        title: format!("{agent} task"),
        detail: agent.into(),
        identity: Some("claude".into()),
        workspace: "herdr".into(),
        branch: Some("main".into()),
        host: "MacBook Pro".into(),
        endpoint: "local".into(),
        since: None,
        pr: None::<Pr>,
        open: Open::Pane {
            endpoint: "local".into(),
            pane: format!("pane-{agent}"),
            tab: "t0".into(),
            workspace: "w0".into(),
        },
        muted: false,
    }
}

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}
