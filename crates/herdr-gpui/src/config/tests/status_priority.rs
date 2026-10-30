use super::*;
use crate::config::preferences::Preference;
use herdr_client::protocol::AgentStatus;

#[test]
fn status_priority_defaults_parses_and_completes_partial_orders() -> anyhow::Result<()> {
    let default = DEFAULT_STATUS_PRIORITY;
    assert_eq!(Config::default().sidebar_style.status_priority, default);
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.sidebar_style.status_priority,
        default
    );
    assert_eq!(
        Config::parse("[sidebar]\nstatus_priority = []")?
            .sidebar_style
            .status_priority,
        default
    );
    assert_eq!(
        Config::parse("[sidebar]\nstatus_priority = [\"working\", \"blocked\"]")?
            .sidebar_style
            .status_priority,
        [
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Idle,
        ]
    );
    assert_eq!(
        Config::parse(
            "[sidebar]\nstatus_priority = [\"blocked\", \"working\", \"done\", \"idle\"]"
        )?
        .sidebar_style
        .status_priority,
        [
            AgentStatus::Blocked,
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Idle,
        ]
    );
    Ok(())
}

#[test]
fn status_priority_rejects_unknown_and_duplicate_names() {
    assert!(matches!(
        Config::parse("[sidebar]\nstatus_priority = [\"unknown\"]"),
        Err(crate::Error::Settings(Error::InvalidSidebarStatus(name))) if name == "unknown"
    ));
    assert!(matches!(
        Config::parse("[sidebar]\nstatus_priority = [\"working\", \"working\"]"),
        Err(crate::Error::Settings(Error::DuplicateSidebarStatus(name))) if name == "working"
    ));
}

#[test]
fn saving_custom_and_default_priority_writes_then_removes_the_key() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nstatus_priority = [] # local\n")?;
    Config::save_preference_path(
        Preference::SidebarStatusPriority(vec![
            AgentStatus::Blocked,
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Idle,
        ]),
        &path,
    )?;
    let text = fs::read_to_string(&path)?;
    assert!(
        text.contains("status_priority = [\"blocked\", \"working\", \"done\", \"idle\"] # local"),
        "{text}"
    );
    assert_eq!(
        Config::parse(&text)?.sidebar_style.status_priority,
        [
            AgentStatus::Blocked,
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Idle,
        ]
    );
    Config::save_preference_path(
        Preference::SidebarStatusPriority(DEFAULT_STATUS_PRIORITY.to_vec()),
        &path,
    )?;
    assert!(!fs::read_to_string(&path)?.contains("status_priority"));
    Ok(())
}
