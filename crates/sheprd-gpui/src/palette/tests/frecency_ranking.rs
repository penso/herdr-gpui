use super::frecency::Key;
use super::*;

fn project(label: &str, boost: u32) -> Entry {
    let mut entry = Entry::new(
        label.into(),
        String::new(),
        "Project",
        Action::Project(projects::Project {
            path: format!("/code/{label}").into(),
            label: label.into(),
        }),
        None,
    );
    entry.boost = boost;
    entry
}

fn labels(entries: &[Entry], hits: &[search::Hit]) -> Vec<String> {
    hits.iter()
        .map(|hit| entries[hit.index].label.to_string())
        .collect()
}

fn rows(filter: Filter, entries: &[Entry], query: &str) -> Vec<String> {
    let hits = rank_rows(
        filter,
        AgentFilter::All,
        entries,
        (0..entries.len()).collect(),
        &search::Query::parse(query),
        &mut search::matcher(),
    );
    labels(entries, &hits)
}

#[test]
fn a_frecent_row_rises_among_matches_but_never_makes_a_match() {
    let entries = [
        project("build-a", 0),
        project("build-b", 32),
        project("other", 64),
    ];
    assert_eq!(rows(Filter::All, &entries, "build"), ["build-b", "build-a"]);
    let unboosted = [project("build-a", 0), project("build-b", 0)];
    assert_eq!(
        rows(Filter::All, &unboosted, "build"),
        ["build-a", "build-b"]
    );
}

#[test]
fn only_the_flat_project_list_sorts_by_frecency_without_a_query() {
    let entries = [
        project("a", 0),
        project("b", 40),
        project("c", 0),
        project("d", 40),
    ];
    assert_eq!(rows(Filter::Projects, &entries, ""), ["b", "d", "a", "c"]);
    // Elsewhere an empty query keeps entry order, so Go To keeps its tree.
    assert_eq!(rows(Filter::All, &entries, ""), ["a", "b", "c", "d"]);
}

#[test]
fn go_to_rows_are_remembered_by_durable_or_boot_scoped_keys() {
    let snapshot = go_to_fixture();
    let mut entries = Vec::new();
    go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
    let key = |target: NavigationTarget<String>| {
        entries
            .iter()
            .find(|entry| {
                entry.action.identity()
                    == Identity::Go(crate::endpoint::LOCAL.into(), target.clone())
            })
            .and_then(|entry| entry.frecency.clone())
    };
    assert_eq!(
        key(NavigationTarget::Pane("w1:p2".into())),
        Some(Key::Pane {
            endpoint: crate::endpoint::LOCAL.into(),
            boot: snapshot.boot_id.clone(),
            id: "w1:p2".into(),
        })
    );
    assert_eq!(
        key(NavigationTarget::Tab("w1:t2".into())),
        Some(Key::Tab {
            endpoint: crate::endpoint::LOCAL.into(),
            boot: snapshot.boot_id.clone(),
            id: "w1:t2".into(),
        })
    );
    // A workspace outlives its daemon ID by its launch path and branch.
    let workspace = &snapshot.workspaces[0];
    assert_eq!(
        key(NavigationTarget::Workspace(workspace.workspace_id.clone())),
        launch_root(&snapshot, &workspace.workspace_id).map(|root| Key::Workspace {
            endpoint: crate::endpoint::LOCAL.into(),
            root: root.into(),
            branch: workspace.branch.clone(),
        })
    );
}

/// A row on another host needs only that host's snapshot to be chosen.
#[gpui::test]
fn choosing_a_go_to_row_boosts_it_the_next_time(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:box".into(),
            "Box".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = Some(Arc::new(snapshot()));
        remote.live.status = crate::state::ConnectionStatus::Connected;
        view.endpoints.push(remote);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Navigation, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    let key = view.update(cx, |view, _| {
        let palette = view.menu.palette.as_mut().unwrap();
        let row = palette
            .filtered
            .iter()
            .position(|hit| {
                matches!(
                    &palette.entries[hit.index].action,
                    Action::Go {
                        endpoint,
                        target: NavigationTarget::Pane(_),
                        ..
                    } if endpoint == "ssh:box"
                )
            })
            .unwrap();
        palette.selected = row;
        assert_eq!(palette.entries[palette.filtered[row].index].boost, 0);
        palette.entries[palette.filtered[row].index]
            .frecency
            .clone()
            .unwrap()
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        assert!(view.menu.palette.is_none());
        let history = Frecency::of(cx).unwrap();
        assert!(history.boosts(frecency::now()).of(Some(&key)) > 0);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Navigation, window, cx)
        })
    });
    view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        let entry = palette
            .entries
            .iter()
            .find(|entry| entry.frecency.as_ref() == Some(&key))
            .unwrap();
        assert!(entry.boost > 0);
    });
}
