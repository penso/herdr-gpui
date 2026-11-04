use super::*;
use crate::config::{AgentToken, SidebarConfigError, SidebarLayout, SidebarScope};

fn token(scope: SidebarScope, token: &str, shown: bool) -> Edit {
    Edit::SidebarToken {
        scope,
        token: token.into(),
        shown,
    }
}

fn layout(path: &Path) -> anyhow::Result<SidebarLayout> {
    Ok(SidebarLayout::from_daemon_config(
        &fs::read_to_string(path)?.parse()?,
    )?)
}

#[test]
fn showing_on_unset_rows_keeps_the_defaults_and_hiding_restores_them() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let original = "# shared\n[ui]\nsound = { enabled = true }\n";
    fs::write(&path, original)?;
    let settings = Settings::load_path(path.clone())?
        .save(token(SidebarScope::Agents, "$summary", true))?
        .save(token(SidebarScope::Agents, "$summary", true))?;
    assert!(fs::read_to_string(&path)?.starts_with(original));
    let mut expected = SidebarLayout::default();
    expected
        .agents
        .rows
        .push(vec![crate::config::sidebar::ConfiguredToken::plain(
            AgentToken::Custom("summary".into()),
        )]);
    assert_eq!(layout(&path)?, expected);
    assert!(layout(&path)?.shows(SidebarScope::Agents, "$summary"));

    settings
        .save(token(SidebarScope::Agents, "$summary", false))?
        .save(token(SidebarScope::Agents, "$summary", false))?
        .save(token(SidebarScope::Spaces, "$ci", false))?;
    assert_eq!(layout(&path)?, SidebarLayout::default());
    assert!(!fs::read_to_string(&path)?.contains("[ui.sidebar.spaces]"));
    Ok(())
}

#[test]
fn hiding_keeps_styles_neighbours_comments_and_per_agent_overrides() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    let original = "[ui.sidebar.agents]\nrows = [\n  [\"state_icon\", { token = \"$model\", bold = true }], # first\n  [\"$model\"],\n]\nrow_gap = 1 # gap\n[ui.sidebar.agents.rows_by_agent]\nclaude = [[\"$model\"]]\n";
    fs::write(&path, original)?;
    // A styled occurrence already counts as shown.
    let settings =
        Settings::load_path(path.clone())?.save(token(SidebarScope::Agents, "$model", true))?;
    assert_eq!(fs::read_to_string(&path)?, original);
    settings.save(token(SidebarScope::Agents, "$model", false))?;
    let text = fs::read_to_string(&path)?;
    for kept in [
        "[\"state_icon\"]",
        "# first",
        "row_gap = 1 # gap",
        "claude = [[\"$model\"]]",
    ] {
        assert!(text.contains(kept), "{kept} in {text}");
    }
    let layout = layout(&path)?;
    assert_eq!(layout.agents.rows.len(), 1);
    assert!(!layout.shows(SidebarScope::Agents, "$model"));
    Ok(())
}

#[test]
fn refusals_write_nothing_and_unrelated_edits_ignore_a_bad_layout() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    fs::write(&path, "")?;
    let settings = Settings::load_path(path.clone())?;
    for (scope, bad) in [
        (SidebarScope::Agents, "$has space"),
        (SidebarScope::Agents, "summary"),
        (SidebarScope::Spaces, "terminal_title"),
    ] {
        let error = settings
            .save(token(scope, bad, true))
            .err()
            .ok_or_else(|| anyhow::anyhow!("saved {bad}"))?;
        assert!(
            matches!(source(&error), Some(Error::SidebarToken(_))),
            "{bad}"
        );
    }
    assert_eq!(fs::read_to_string(&path)?, "");

    let full = format!(
        "[ui.sidebar.spaces]\nrows = [{}]\n",
        "[\"workspace\"],".repeat(crate::config::MAX_SIDEBAR_ROWS)
    );
    fs::write(&path, &full)?;
    let error = Settings::load_path(path.clone())?
        .save(token(SidebarScope::Spaces, "$ci", true))
        .err()
        .ok_or_else(|| anyhow::anyhow!("saved a 17th row"))?;
    assert!(matches!(
        source(&error),
        Some(Error::SidebarToken(SidebarConfigError::TooManyRows))
    ));
    assert_eq!(fs::read_to_string(&path)?, full);

    let broken = "[ui.sidebar.agents]\nrows = [[\"bogus\"]]\n";
    fs::write(&path, broken)?;
    let settings = Settings::load_path(path.clone())?;
    assert!(
        settings
            .save(token(SidebarScope::Agents, "$x", true))
            .is_err()
    );
    assert_eq!(fs::read_to_string(&path)?, broken);
    settings.save(Edit::Sound(false))?;
    Ok(())
}

#[test]
fn an_external_change_is_a_conflict_not_an_overwrite() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("config.toml");
    fs::write(&path, "")?;
    let settings = Settings::load_path(path.clone())?;
    fs::write(&path, "[ui]\nfuture = 1\n")?;
    let error = settings
        .save(token(SidebarScope::Agents, "$summary", true))
        .err()
        .ok_or_else(|| anyhow::anyhow!("overwrote an external change"))?;
    assert!(matches!(source(&error), Some(Error::Conflict)));
    assert_eq!(fs::read_to_string(&path)?, "[ui]\nfuture = 1\n");
    Ok(())
}
