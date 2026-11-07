use super::*;

#[test]
fn pasted_transport_markers_do_not_show_as_pane_or_tab_captions() {
    let mut snapshot = layout_tests::snapshot(1);
    snapshot.agents[0].title = Some("<pasted_content id=\"demo\">".into());
    snapshot.tabs[0].label = "<pasted_content id=\"demo\">".into();
    assert_eq!(named(&snapshot), None);
    assert_eq!(agent_place(&snapshot.agents[0], &snapshot).unwrap().1, None);
    snapshot.panes = vec![pane("p0", Some("#357 Windows GUI never connects"))];
    assert_eq!(
        named(&snapshot).as_deref(),
        Some("#357 Windows GUI never connects")
    );
    snapshot.tabs[0].label = "My chosen tab".into();
    assert_eq!(
        agent_place(&snapshot.agents[0], &snapshot).unwrap().1,
        Some("My chosen tab")
    );
}
