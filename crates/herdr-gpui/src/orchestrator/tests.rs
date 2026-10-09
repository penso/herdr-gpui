use super::*;
use chrono::{DateTime, Utc};

mod keys;
mod location;
mod store;

fn at(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn github() -> SourceKey {
    SourceKey {
        provider: Provider::Github,
        host: "github.com".into(),
        repository: "penso/herdr-gpui".into(),
    }
}

fn beads() -> SourceKey {
    SourceKey {
        provider: Provider::Beads,
        host: "local".into(),
        repository: "/Users/me/src/herdr-gpui".into(),
    }
}

fn item(source: &SourceKey, native_id: &str, title: &str) -> Item {
    Item {
        key: ItemKey {
            source: source.clone(),
            native_id: native_id.into(),
        },
        identifier: format!("#{native_id}"),
        title: title.into(),
        description: Some("Body".into()),
        state: "open".into(),
        url: Some(format!(
            "https://github.com/penso/herdr-gpui/issues/{native_id}"
        )),
        author: Some("penso".into()),
        labels: vec!["bug".into()],
        parent_id: None,
        blocked_by: Vec::new(),
        priority: None,
        created_at: Some(at("2026-10-08T23:23:37+00:00")),
        updated_at: Some(at("2026-10-09T01:00:00.5+00:00")),
        pull_request: None,
        activity: Some(Activity {
            comments: Some(2),
            ..Activity::default()
        }),
    }
}

fn run(id: &str, owner: Owner) -> Run {
    Run {
        id: id.into(),
        item_key: item(&github(), "377", "Icons").key.canonical(),
        workspace: Some(Workspace {
            backend: Backend::Herdr,
            id: "w12".into(),
            host: None,
            path: Some("/Users/me/.herdr/worktrees/app/377-icons".into()),
            branch: "377-icons".into(),
        }),
        agent: "claude".into(),
        model: None,
        state: RunState::Running,
        message: None,
        session_id: Some("herdr-gpui-0123456789abcdef0123456".into()),
        started_at: at("2026-10-08T06:12:44.896730+00:00"),
        updated_at: at("2026-10-08T06:12:47.451152+00:00"),
        owner,
    }
}

fn session(run_id: &str) -> HerdrSession {
    HerdrSession {
        run_id: run_id.into(),
        host: None,
        session: None,
        workspace_id: "w12".into(),
        pane_id: "p31".into(),
        agent_name: "herdr-gpui-0123456789abcdef0123456".into(),
        updated_at: at("2026-10-08T06:12:47+00:00"),
    }
}
