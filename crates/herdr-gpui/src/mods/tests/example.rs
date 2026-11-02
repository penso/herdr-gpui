use super::*;
use herdr_mods::{Agent, AgentStatus, Limits, Mod, Sources};
use std::time::SystemTime;

/// The example shipped beside the app loads, draws, and answers its command.
#[test]
fn the_shipped_example_works() {
    let sources = Sources::new([
        (
            "mod.toml".to_owned(),
            include_bytes!("../../../mods/agents/mod.toml").to_vec(),
        ),
        (
            "main.luau".to_owned(),
            include_bytes!("../../../mods/agents/main.luau").to_vec(),
        ),
    ])
    .unwrap();
    let now = SystemTime::now();
    let mut module = Mod::load(&sources, Limits::default(), now).unwrap();
    let quiet = module.view(&State::default(), now).unwrap().unwrap();
    let Node::Row(quiet) = quiet else {
        panic!("expected a row, got {quiet:?}");
    };
    assert_eq!(text(&quiet.children[0]), "No agents busy");

    let blocked = State {
        agents: vec![Agent {
            id: "p1".into(),
            host: "local".into(),
            workspace: "w".into(),
            name: "claude".into(),
            status: AgentStatus::Blocked,
        }],
        ..State::default()
    };
    module.update(&Data::from("command:names"), now).unwrap();
    let busy = module.view(&blocked, now).unwrap().unwrap();
    let Node::Tooltip(tooltip) = busy else {
        panic!("expected a tooltip, got {busy:?}");
    };
    let Node::Row(row) = *tooltip.child else {
        panic!("expected a row");
    };
    assert!(matches!(&row.children[0], Node::Badge(badge) if badge.text == "1 waiting"));
    assert_eq!(text(&row.children[1]), "claude");
}

/// The shipped activity bar app loads, declares its icon, and draws.
#[test]
fn the_shipped_dashboard_app_works() {
    let sources = Sources::new([
        (
            "mod.toml".to_owned(),
            include_bytes!("../../../mods/dashboard/mod.toml").to_vec(),
        ),
        (
            "main.luau".to_owned(),
            include_bytes!("../../../mods/dashboard/main.luau").to_vec(),
        ),
        (
            "icon.svg".to_owned(),
            include_bytes!("../../../mods/dashboard/icon.svg").to_vec(),
        ),
    ])
    .unwrap();
    let activity = sources.manifest.activity.clone().unwrap();
    assert_eq!(activity.title, "Dashboard");
    assert!(
        sources
            .icon(&activity.icon)
            .unwrap()
            .contains("currentColor")
    );
    let now = SystemTime::now();
    let mut module = Mod::load(&sources, Limits::default(), now).unwrap();
    let tree = module.view(&State::default(), now).unwrap().unwrap();
    assert!(matches!(tree, Node::Column(_)));
    module.update(&Data::from("reviewed"), now).unwrap();
}

/// The shipped panel section shows the focused workspace, and nothing
/// while no workspace is focused.
#[test]
fn the_shipped_workspace_section_works() {
    use herdr_mods::{Focus, Tab, Workspace};
    let sources = Sources::new([
        (
            "mod.toml".to_owned(),
            include_bytes!("../../../mods/workspace/mod.toml").to_vec(),
        ),
        (
            "main.luau".to_owned(),
            include_bytes!("../../../mods/workspace/main.luau").to_vec(),
        ),
    ])
    .unwrap();
    assert_eq!(
        sources.manifest.panel.title.as_deref(),
        Some("This workspace")
    );
    let now = SystemTime::now();
    let mut module = Mod::load(&sources, Limits::default(), now).unwrap();
    assert_eq!(module.view(&State::default(), now).unwrap(), None);

    let state = State {
        workspaces: vec![Workspace {
            id: "w1".into(),
            host: "local".into(),
            name: "api".into(),
            focused: true,
            tabs: vec![Tab {
                id: "t1".into(),
                name: "server".into(),
                focused: true,
            }],
        }],
        agents: vec![Agent {
            id: "p1".into(),
            host: "local".into(),
            workspace: "w1".into(),
            name: "claude".into(),
            status: AgentStatus::Working,
        }],
        focus: Some(Focus {
            host: "local".into(),
            workspace: Some("w1".into()),
            tab: Some("t1".into()),
            name: Some("api".into()),
            path: Some("/src/api".into()),
        }),
        ..State::default()
    };
    let Some(Node::Column(column)) = module.view(&state, now).unwrap() else {
        panic!("expected a column");
    };
    assert_eq!(text(&column.children[0]), "/src/api");
    let Node::List(agents) = &column.children[2] else {
        panic!("expected the agents");
    };
    assert_eq!(agents.items.len(), 1);
}
