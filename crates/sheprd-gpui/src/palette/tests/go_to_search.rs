use super::*;

#[test]
fn go_to_finds_agents_by_kind_name_and_status_and_tabs_by_label() {
    let snapshot = go_to_fixture();
    let mut entries = Vec::new();
    go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
    let found = |query: &str| {
        search::rank(
            &entries,
            0..entries.len(),
            &search::Query::parse(query),
            &mut search::matcher(),
        )
        .first()
        .map(|hit| entries[hit.index].action.identity())
    };
    let pane = |id: &str| {
        Some(Identity::Go(
            crate::endpoint::LOCAL.into(),
            NavigationTarget::Pane(id.into()),
        ))
    };
    for query in ["claude", "reviewer", "blocked", "waiting", "agent"] {
        assert_eq!(found(query), pane("w1:p1"), "{query}");
    }
    assert_eq!(
        found("logs tab"),
        Some(Identity::Go(
            crate::endpoint::LOCAL.into(),
            NavigationTarget::Tab("w1:t2".into())
        ))
    );
}

#[test]
fn go_to_finds_agents_by_the_titles_and_label_the_sidebar_shows() {
    let mut snapshot = go_to_fixture();
    snapshot.agents[0].title = Some("Fix flaky checkout spec".into());
    snapshot.agents[0].terminal_title_stripped = Some("doorbuster product swap".into());
    snapshot.panes[0].label = Some("nightly triage".into());
    let mut entries = Vec::new();
    go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
    let found = |query: &str| {
        search::rank(
            &entries,
            0..entries.len(),
            &search::Query::parse(query),
            &mut search::matcher(),
        )
        .first()
        .map(|hit| entries[hit.index].action.identity())
    };
    let agent_pane = Some(Identity::Go(
        crate::endpoint::LOCAL.into(),
        NavigationTarget::Pane("w1:p1".into()),
    ));
    for query in ["flaky checkout", "doorbuster swap", "nightly triage"] {
        assert_eq!(found(query), agent_pane, "{query}");
    }
}

#[test]
fn go_to_titles_agents_with_the_pane_name_the_sidebar_shows() {
    let agent_row_title = |label: Option<&str>, title: Option<&str>| {
        let mut snapshot = go_to_fixture();
        snapshot.panes[0].label = label.map(str::to_owned);
        snapshot.agents[0].title = title.map(str::to_owned);
        let mut entries = Vec::new();
        go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
        let agent_pane = Identity::Go(
            crate::endpoint::LOCAL.into(),
            NavigationTarget::Pane("w1:p1".into()),
        );
        entries
            .into_iter()
            .find(|entry| entry.action.identity() == agent_pane)
            .map(|entry| entry.label.to_string())
    };
    assert_eq!(
        agent_row_title(Some("shop-cash-eng question"), Some("Review")).as_deref(),
        Some("Claude · shop-cash-eng question")
    );
    assert_eq!(
        agent_row_title(None, Some("Review")).as_deref(),
        Some("Claude · Review")
    );
    assert_eq!(agent_row_title(None, None).as_deref(), Some("Claude"));
}
