//! The orchestrator opens from a group's "…" menu, on hosts that can run
//! its scripts.
use super::*;

#[gpui::test]
fn the_group_menu_offers_the_orchestrator_where_scripts_run(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    // The fixture's explicit socket cannot run scripts.
    assert!(!actions(&view, cx, group).contains(&Action::Orchestrator));
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
        })
    });
    let offered = actions(&view, cx, group);
    assert!(offered.contains(&Action::Orchestrator));
    assert_eq!(Action::Orchestrator.label(), "Orchestrator");
    // It sits with the other tabs the menu opens, before the closing rows.
    let at = offered
        .iter()
        .position(|a| *a == Action::Orchestrator)
        .unwrap();
    assert!(offered[..at].iter().all(|a| a.section() == 0));
}
