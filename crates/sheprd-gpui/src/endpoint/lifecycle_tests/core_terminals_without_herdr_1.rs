//! With no Herdr 1 installed there is no snapshot to name a workspace, so
//! the window keeps herdr-core's tabs in a workspace of its own.

use super::*;
use crate::{
    SheprdWindow,
    browser::{Location, Pick, Store, TabId},
    core_terminals::{WORKSPACE, panes::Panes},
};

fn without_herdr_1(view: &mut SheprdWindow) {
    view.live.snapshot = None;
    view.live.missing_installation = true;
}

fn open_core_tab(view: &SheprdWindow, cx: &mut gpui::App) -> TabId {
    let (scope, workspace) = view.browser_key().unwrap();
    let terminal = sheprd_backend::TerminalId::new("t1").unwrap();
    let location = Location::CoreTerminal {
        observe: false,
        panes: Panes::single(terminal),
    };
    Store::update(cx, |store| {
        store.open(scope, &workspace, Some(location), None)
    })
    .unwrap()
}

#[gpui::test]
fn tabs_have_a_workspace_with_no_herdr_1(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
        Store::default().install(cx);
        without_herdr_1(view);
        assert_eq!(view.browser_key().unwrap().1, WORKSPACE);
        let tab = open_core_tab(view, cx);
        assert_eq!(view.browser_tab_ids(cx), [tab]);
    });
}

#[gpui::test]
fn only_a_missing_install_gets_the_core_workspace(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::palette::fixture_window);
    view.update(cx, |view, _| {
        let herdr_1 = view.browser_key().unwrap().1;
        assert_ne!(herdr_1, WORKSPACE);
        // Herdr 1 answering again wins over a stale flag.
        view.live.missing_installation = true;
        assert_eq!(view.browser_key().unwrap().1, herdr_1);
        // A daemon still connecting is not a missing install.
        view.live.snapshot = None;
        view.live.missing_installation = false;
        assert!(view.browser_key().is_none());
    });
}

#[gpui::test]
fn closing_a_core_tab_settles_its_group_with_no_herdr_1(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    cx.update(|window, cx| {
        Store::default().install(cx);
        view.update(cx, |view, cx| {
            without_herdr_1(view);
            let tab = open_core_tab(view, cx);
            view.settle_groups(window, cx);
            assert!(view.open_workspace_tabs(cx).contains(&Pick::Page(tab)));
            Store::update(cx, |store| store.close(tab));
            view.settle_groups(window, cx);
            assert!(!view.open_workspace_tabs(cx).contains(&Pick::Page(tab)));
        })
    });
}
