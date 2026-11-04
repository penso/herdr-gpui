use super::*;

fn layout(text: &str) -> anyhow::Result<SidebarLayout> {
    Ok(SidebarLayout::from_daemon_config(&text.parse()?)?)
}

#[test]
fn scope_defaults_spell_the_typed_defaults() -> anyhow::Result<()> {
    let rows = |scope: SidebarScope| {
        let rows: Vec<String> = scope
            .default_rows()
            .iter()
            .map(|row| format!("{row:?}"))
            .collect();
        format!(
            "[ui.sidebar.{}]\nrows = [{}]\n",
            scope.key(),
            rows.join(", ")
        )
    };
    let text = format!(
        "{}{}",
        rows(SidebarScope::Agents),
        rows(SidebarScope::Spaces)
    );
    assert_eq!(layout(&text)?, SidebarLayout::default());
    Ok(())
}

#[test]
fn shows_reads_only_the_default_rows_of_its_scope() -> anyhow::Result<()> {
    let layout = layout(
        "[ui.sidebar.agents]\nrows = [[{ token = \"$model\", bold = true }]]\n\
         [ui.sidebar.agents.rows_by_agent]\nclaude = [[\"$summary\"]]\n\
         [ui.sidebar.spaces]\nrows = [[\"state_text\"]]\n",
    )?;
    assert!(layout.shows(SidebarScope::Agents, "$model"));
    assert!(!layout.shows(SidebarScope::Agents, "$summary"));
    assert!(!layout.shows(SidebarScope::Agents, "state_text"));
    assert!(layout.shows(SidebarScope::Spaces, "state_text"));
    assert!(!layout.shows(SidebarScope::Spaces, "$model"));
    assert!(!layout.shows(SidebarScope::Agents, "$bad name"));
    assert!(SidebarScope::Agents.check("terminal_title").is_ok());
    assert!(SidebarScope::Spaces.check("terminal_title").is_err());
    assert!(SidebarScope::Spaces.check("$x y").is_err());
    assert!(SidebarScope::Agents.check("summary").is_err());
    Ok(())
}
