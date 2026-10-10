use super::rows::*;
use crate::orchestrator::{
    Activity, Backend, HerdrSession, Item, ItemKey, Owner, Provider, Run, RunState, SourceKey,
    Workspace,
};
use chrono::{DateTime, Utc};
use herdr_client::protocol::AgentStatus;

mod clicks;
mod conversation;
mod dialog;
mod markdown;
mod rows;

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn source(provider: Provider) -> SourceKey {
    SourceKey {
        provider,
        host: if provider == Provider::Beads {
            "local"
        } else {
            "github.com"
        }
        .into(),
        repository: "penso/app".into(),
    }
}

fn item(provider: Provider, id: &str, title: &str, updated: &str) -> Item {
    Item {
        key: ItemKey {
            source: source(provider),
            native_id: id.into(),
        },
        identifier: id.into(),
        title: title.into(),
        description: None,
        state: "open".into(),
        url: None,
        author: Some("penso".into()),
        labels: Vec::new(),
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: Some(at(updated)),
        updated_at: Some(at(updated)),
        pull_request: None,
        activity: Some(Activity::default()),
    }
}

fn run(id: &str, item: &Item, state: RunState, started: &str) -> Run {
    Run {
        id: id.into(),
        item_key: item.key.canonical(),
        workspace: Some(Workspace {
            backend: Backend::Herdr,
            id: format!("w-{id}"),
            host: None,
            path: None,
            branch: format!("b-{id}"),
        }),
        agent: "claude".into(),
        model: None,
        state,
        message: None,
        session_id: None,
        started_at: at(started),
        updated_at: at(started),
        owner: Owner::HerdrGpui,
    }
}

fn session(run: &Run, pane: &str) -> HerdrSession {
    HerdrSession {
        run_id: run.id.clone(),
        host: None,
        session: None,
        workspace_id: format!("w-{}", run.id),
        pane_id: pane.into(),
        agent_name: format!("herdr-gpui-{}", run.id),
        updated_at: run.started_at,
    }
}

fn live(run: &Run, pane: &str, status: AgentStatus) -> LiveAgent {
    LiveAgent {
        endpoint: 0,
        host: None,
        workspace_id: format!("w-{}", run.id),
        pane_id: pane.into(),
        status,
        title: Some("Running tests".into()),
    }
}
