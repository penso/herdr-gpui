#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    browser::{Store, WebUrl},
    listening_ports::Link,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
};
use core::prelude::v1::test;
use gpui::{TestAppContext, VisualTestContext};
use std::sync::Arc;

fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    draw(cx);
}

fn rows(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<Row> {
    view.read_with(cx, |view, _| view.new_tab_rows())
}

/// Workspace `w0` listening on every interface and on loopback alone.
fn seed_ports(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    seed(
        view,
        cx,
        "L 1 *:3000 node\nL 1 127.0.0.1:5173 vite\nE 1 w0\n",
    );
}

fn seed(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, scan: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.listening_ports.seed(
                crate::usage::Host::Local,
                crate::listening_ports::parse(scan).unwrap(),
            );
            cx.notify();
        })
    });
    draw(cx);
}

#[gpui::test]
fn plus_offers_tab_kinds_instead_of_opening_one(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let herdr = view.read_with(cx, |view, _| {
        view.live.snapshot.as_ref().unwrap().tabs.len()
    });
    click(cx, "new-tab");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::NewTab));
        // Nothing opened yet: no Herdr tab asked for, none placed.
        assert_eq!(view.expected_new_tab_group(), None);
        assert_eq!(view.live.snapshot.as_ref().unwrap().tabs.len(), herdr);
    });
    // Without a checkout there is nothing to review, and no ports were scanned.
    assert_eq!(rows(&view, cx), [Row::Terminal, Row::Browser]);
    assert!(cx.debug_bounds("new-tab-menu-Terminal").is_some());
    assert!(cx.debug_bounds("new-tab-menu-Browser").is_some());
    assert!(cx.debug_bounds("new-tab-menu-Review").is_none());

    // The keyboard steps through the rows, wrapping, and Escape leaves.
    cx.simulate_keystrokes("down down down");
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.new_tab.as_ref().unwrap().selected),
        Some(0)
    );
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

#[gpui::test]
fn a_new_terminal_tab_lands_in_the_group_that_asked(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let right = view.read_with(cx, |view, _| view.group_slots()[1].id);
    click(cx, "g1-new-tab");
    click(cx, "new-tab-menu-Terminal");
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert_eq!(view.expected_new_tab_group(), Some(right));
    });
}

/// A blank tab needs no native page, but only builds that show pages open
/// one rather than handing the request to the system browser.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_new_browser_tab_opens_blank_in_the_group_that_asked(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let right = view.read_with(cx, |view, _| view.group_slots()[1].id);
    click(cx, "g1-new-tab");
    // Keyboard: the second row is New Browser Tab.
    cx.simulate_keystrokes("down down enter");
    draw(cx);
    let picked = view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        view.group_pick(right)
    });
    let Some(crate::browser::Pick::Page(id)) = picked else {
        panic!("the right group shows the new page, not {picked:?}")
    };
    let blank = cx.update(|_, cx| cx.global::<Store>().get(id).unwrap().location.is_none());
    assert!(blank);
    assert!(cx.debug_bounds("g1-browser-placeholder").is_some());
}

#[gpui::test]
fn review_is_offered_for_a_tracked_checkout(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.git = crate::git::Git::fixture(
                crate::pull_request::Input {
                    checkout: None,
                    repo_key: Some("github.com/herdrdev/herdr-gpui".into()),
                    branch: "develop".into(),
                },
                crate::git::Status::default(),
            );
            let group = view.group_slots()[0].id;
            view.open_new_tab_menu(group, Point::default(), window, cx);
        })
    });
    assert_eq!(rows(&view, cx), [Row::Terminal, Row::Browser, Row::Review]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.activate_new_tab_row(Row::Review, window, cx)
        })
    });
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert_eq!(view.reviews.len(), 1);
    });
    // A second review of the same checkout brings its tab back.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let group = view.group_slots()[0].id;
            view.open_new_tab_menu(group, Point::default(), window, cx);
            view.activate_new_tab_row(Row::Review, window, cx)
        })
    });
    let reviews = cx.update(|_, cx| {
        cx.try_global::<Store>().map_or(0, |store| {
            (0..64)
                .filter_map(|id| store.get(crate::browser::TabId::test(id)))
                .filter(|tab| matches!(tab.location, Some(crate::browser::Location::Review { .. })))
                .count()
        })
    });
    assert_eq!(reviews, 1);
    view.read_with(cx, |view, _| assert_eq!(view.reviews.len(), 1));
}

#[gpui::test]
fn listening_ports_follow_the_tab_kinds(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    seed_ports(&view, cx);
    click(cx, "new-tab");
    let ports: Vec<_> = rows(&view, cx)
        .into_iter()
        .filter_map(|row| match row {
            Row::Port { link, process, .. } => Some((link.label(), process)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ports,
        [
            ("localhost:3000".to_owned(), "node".to_owned()),
            ("localhost:5173".to_owned(), "vite".to_owned()),
        ]
    );
    let browser = cx.debug_bounds("new-tab-menu-Browser").unwrap();
    let first = cx.debug_bounds("new-tab-menu-Port3000").unwrap();
    assert!(first.top() > browser.bottom());
    // Hidden ports are not offered either.
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| view.update(cx, |view, _| view.config.show_listening_ports = false));
    assert_eq!(rows(&view, cx), [Row::Terminal, Row::Browser]);
}

#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_blank_tab_lists_the_listening_ports(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::NewBrowserTab, window, cx)
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("browser-placeholder").is_some());
    assert!(cx.debug_bounds("blank-port-3000").is_none());
    seed_ports(&view, cx);
    let prompt = cx.debug_bounds("browser-placeholder").unwrap();
    let chip = cx.debug_bounds("blank-port-3000").unwrap();
    assert!(chip.top() > prompt.bottom());
    assert!(cx.debug_bounds("blank-port-5173").is_some());
}

#[test]
fn a_port_link_names_where_it_opens() {
    let page = Link::Page(WebUrl::try_from("http://localhost:5173/").unwrap());
    assert_eq!(page.label(), "localhost:5173");
    // The whole value stays in the row; only the selector is short.
    let row = Row::Port {
        number: 5173,
        process: "vite".into(),
        link: page,
    };
    assert_eq!(row.selector(), "new-tab-menu-Port5173");
}

#[gpui::test]
fn a_long_process_name_leaves_the_port_address_readable(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    // The scan keeps at most 32 characters of a process name.
    let name = "w".repeat(32);
    seed(&view, cx, &format!("L 1 127.0.0.1:5173 {name}\nE 1 w0\n"));
    click(cx, "new-tab");
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let row = cx.debug_bounds("new-tab-menu-Port5173").unwrap();
    let address = cx.debug_bounds("new-tab-menu-Port5173-label").unwrap();
    let process = cx.debug_bounds("new-tab-menu-Port5173-detail").unwrap();
    assert!(process.size.width <= px(PROCESS_WIDTH), "{process:?}");
    assert!(address.right() <= process.left(), "{address:?} {process:?}");
    assert!(row.right() <= panel.right(), "{row:?} {panel:?}");
    // The address keeps most of the row rather than being squeezed out.
    assert!(address.size.width >= px(100.), "{address:?}");
    // A shortcut hint is not capped.
    let shortcut = cx.debug_bounds("new-tab-menu-Browser-detail").unwrap();
    assert!(shortcut.size.width > px(0.));
}
