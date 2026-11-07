use super::*;

mod transport;

fn pane(id: &str, label: Option<&str>) -> ClientShellPane {
    ClientShellPane {
        pane_id: id.to_owned(),
        workspace_id: "w0".to_owned(),
        tab_id: "t0".to_owned(),
        label: label.map(str::to_owned),
        cwd: None,
        foreground_cwd: None,
        focused: false,
        right_click_passthrough: false,
    }
}

/// The pane name an agent called "grok" shows.
fn named(snapshot: &ClientShellSnapshot) -> Option<String> {
    pane_name(&snapshot.agents[0], snapshot, "grok")
}

/// Rename's label, then the title an unnamed pane already carries.
#[test]
fn pane_names_prefer_a_rename_over_the_title() {
    let mut snapshot = layout_tests::snapshot(1);
    let agent = &mut snapshot.agents[0];
    agent.pane_id = "p0".into();
    agent.title = Some("reported".into());
    snapshot.panes = vec![pane("p0", Some("  renamed  "))];
    assert_eq!(named(&snapshot).as_deref(), Some("renamed"));

    snapshot.panes[0].label = Some(" \t ".into());
    assert_eq!(named(&snapshot).as_deref(), Some("reported"));

    snapshot.agents[0].title = Some("  ".into());
    snapshot.panes[0].label = None;
    assert_eq!(named(&snapshot), None);

    snapshot.panes.clear();
    snapshot.agents[0].title = Some("from-title".into());
    assert_eq!(named(&snapshot).as_deref(), Some("from-title"));

    snapshot.panes = vec![pane("other", Some("elsewhere"))];
    assert_eq!(named(&snapshot).as_deref(), Some("from-title"));
}

/// A pane name that only repeats the agent name adds nothing to the row or
/// to what a screen reader says for it.
#[test]
fn pane_names_drop_a_repeat_of_the_agent_name() {
    let mut snapshot = layout_tests::snapshot(1);
    snapshot.agents[0].pane_id = "p0".into();
    snapshot.agents[0].title = Some(" grok ".into());
    assert_eq!(named(&snapshot), None);
    snapshot.panes = vec![pane("p0", Some("grok"))];
    assert_eq!(named(&snapshot), None);
    snapshot.panes[0].label = Some("grok api".into());
    assert_eq!(named(&snapshot).as_deref(), Some("grok api"));
}

/// Pane names are untrusted text: controls and direction overrides are
/// dropped, a name made only of them is no name, and the length is bounded.
#[test]
fn pane_names_drop_controls_and_direction_overrides() {
    let mut snapshot = layout_tests::snapshot(1);
    let agent = &mut snapshot.agents[0];
    agent.pane_id = "p0".into();
    agent.title = Some("fallback".into());
    snapshot.panes = vec![pane("p0", Some(" ap\u{7}i\u{202e}bew "))];
    assert_eq!(named(&snapshot).as_deref(), Some("apibew"));

    snapshot.panes[0].label = Some("\u{202e}\u{1b} ".into());
    assert_eq!(named(&snapshot).as_deref(), Some("fallback"));

    snapshot.panes[0].label = Some("x".repeat(10_000));
    let capped = named(&snapshot).map(|text| text.chars().count());
    assert_eq!(capped, Some(PANE_CAPTION_LIMIT));
}

/// Different pane names stay different on the row, including one that matches
/// the workspace.
#[test]
fn agent_rows_carry_their_pane_name() {
    let place = Some(("herdr-gpui", None));
    let named = |pane| agent_labels("grok", place, None, pane);
    assert_ne!(named(Some("api")), named(Some("web")));
    assert_eq!(
        named(Some("api")),
        (vec![("herdr-gpui", true), ("api", false)], "grok")
    );
    assert_ne!(named(Some("herdr-gpui")), named(None));
    assert_eq!(
        agent_labels("grok", None, None, Some("api")),
        (vec![("grok", true), ("api", false)], "")
    );
}
