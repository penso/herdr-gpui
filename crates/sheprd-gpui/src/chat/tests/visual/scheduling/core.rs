use super::*;
use crate::{browser::Location, core_terminals::panes::Panes};
use sheprd_backend::TerminalId;

#[gpui::test]
fn core_panes_share_the_budget_without_a_herdr_surface_and_skip_hidden_tabs(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx);
    let previous = take(&view, cx);
    assert_eq!(previous.len(), visual::MAX_RUNNING);
    let (group, driven, mirror) = view.update(cx, |view, cx| {
        let mut targets = targets();
        let first = TerminalId::new("t1").unwrap();
        let second = TerminalId::new("t2").unwrap();
        let mut panes = Panes::single(first.clone());
        panes
            .split(
                &first,
                crate::core_terminals::panes::Axis::Row,
                second.clone(),
            )
            .unwrap();
        for (target, terminal) in targets.iter_mut().zip([first, second]) {
            target.pane_id = core_pane::key(&terminal);
            target.surface = Surface::Core(terminal);
        }
        install(view, targets);
        let (scope, workspace) = view.browser_key().unwrap();
        let mut add = |observe| {
            let location = Location::CoreTerminal {
                panes: panes.clone(),
                observe,
            };
            let tab = Store::update(cx, |store| {
                store.open(scope.clone(), &workspace, Some(location), None)
            })
            .unwrap();
            view.core_terminals
                .show_for_test(tab, panes.clone(), observe, Vec::new());
            tab
        };
        let driven = add(false);
        let mirror = add(true);
        let layout = view.ensure_layout().unwrap();
        let group = layout.active();
        layout.choose(group, Pick::Page(driven));
        view.live.surface = None;
        (group, driven, mirror)
    });
    // Old jobs still occupy the shared slots even after their panes disappear.
    assert!(take(&view, cx).is_empty());
    for job in previous {
        finish(&view, job, cx);
    }
    let jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    assert_ne!(jobs[0].pane_id, jobs[1].pane_id);
    assert!(
        jobs.iter()
            .all(|claimed| claimed.pane_id.starts_with("core:"))
    );
    assert!(take(&view, cx).is_empty());
    view.update(cx, |view, _| {
        view.ensure_layout()
            .unwrap()
            .choose(group, Pick::Page(mirror));
    });
    for job in jobs {
        finish(&view, job, cx);
    }
    assert!(
        take(&view, cx).is_empty(),
        "the mirror cannot admit chat work"
    );
    view.update(cx, |view, _| {
        view.ensure_layout()
            .unwrap()
            .choose(group, Pick::Page(driven));
        for pane in ["core:t1", "core:t2"] {
            view.chats.set_raw(pane, true);
        }
    });
    assert!(take(&view, cx).is_empty(), "raw panes show no rich view");
    view.update(cx, |view, _| {
        view.chats.set_raw("core:t2", false);
    });
    let jobs = take(&view, cx);
    assert!(!jobs.is_empty());
    assert!(jobs.iter().all(|claimed| claimed.pane_id == "core:t2"));
    for job in jobs {
        finish(&view, job, cx);
    }
}
