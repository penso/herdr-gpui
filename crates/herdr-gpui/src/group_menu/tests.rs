#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    browser::Store,
    controls::Command,
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

/// Opens blank tabs, which need no native page, without showing them.
fn open_pages(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, count: usize) {
    cx.update(|_, cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| {
            for _ in 0..count {
                store.open(scope.clone(), "w0", None, None);
            }
        });
    });
}

fn groups(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<GroupId> {
    draw(cx);
    view.read_with(cx, |view, _| {
        view.group_slots().into_iter().map(|slot| slot.id).collect()
    })
}

fn actions(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, group: GroupId) -> Vec<Action> {
    cx.update(|_, cx| view.read(cx).group_actions(group, cx))
}

fn run(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, group: GroupId, action: Action) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_group_menu(group, Point::default(), window, cx);
            view.activate_group_menu(action, window, cx);
        })
    });
    draw(cx);
}

fn tabs(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, group: GroupId) -> Vec<Pick> {
    cx.update(|_, cx| view.read(cx).group_tabs(group, cx))
}

/// Every Herdr tab the daemon has, which closing in a group never
/// touches.
fn herdr_tabs(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> usize {
    view.read_with(cx, |view, _| {
        view.live.snapshot.as_ref().unwrap().tabs.len()
    })
}

#[gpui::test]
fn menus_say_what_they_cover(cx: &mut TestAppContext) {
    use crate::menu::Cover;
    let (view, cx) = window(cx);
    draw(cx);
    // A popover covers its own panel.
    let button = cx.debug_bounds("tab-actions").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let Cover::Panel(covered) = view.read_with(cx, |view, _| view.menu.cover.get()) else {
        panic!("a popover covers its panel")
    };
    // The panel, and a margin around it for its border and shadow.
    assert!(covered.contains(&panel.origin) && covered.contains(&panel.bottom_right()));
    assert!(covered.size.width < panel.size.width + px(24.));
    // A dimmed dialog covers the window.
    cx.simulate_keystrokes("escape");
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.cover.get()),
        Cover::Unknown
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::Palette, window, cx)));
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.cover.get()),
        Cover::All
    );
}

#[gpui::test]
fn a_lone_group_offers_no_close(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let [group] = groups(&view, cx)[..] else {
        panic!("one group")
    };
    assert_eq!(
        actions(&view, cx, group),
        [Action::NewBrowserTab, Action::Split]
    );
}

/// Moves page `id` to `group` and shows it there.
fn move_page(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    group: GroupId,
    id: crate::browser::TabId,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_browser_tab_in(Some(group), id, window, cx)
        })
    });
    draw(cx);
}

#[gpui::test]
fn a_split_group_closes_its_tab_for_good_and_itself_without_closing_any(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open_pages(&view, cx, 2);
    let herdr = herdr_tabs(&view, cx);
    let pages = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // Every tab stays with the group that was split; the new one has none
    // to close.
    assert_eq!(tabs(&view, cx, left).len(), herdr + pages.len());
    assert_eq!(tabs(&view, cx, right), []);
    assert_eq!(
        actions(&view, cx, right),
        [Action::NewBrowserTab, Action::CloseGroup, Action::Split]
    );
    assert!(actions(&view, cx, left).contains(&Action::Close));

    // A tab is listed in the one group it was moved to.
    move_page(&view, cx, right, pages[0]);
    move_page(&view, cx, right, pages[1]);
    assert_eq!(
        tabs(&view, cx, right),
        [Pick::Page(pages[0]), Pick::Page(pages[1])]
    );
    assert_eq!(tabs(&view, cx, left).len(), herdr);
    assert!(cx.debug_bounds("g1-browser-tab-0").is_some());
    assert!(cx.debug_bounds("browser-tab-0").is_none());

    // Close Tab closes the tab the group shows, for good, and the group
    // moves to another of its own.
    run(&view, cx, right, Action::Close);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).browser_tab_ids(cx)),
        [pages[0]]
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.group_pick(right)),
        Some(Pick::Page(pages[0]))
    );

    // Close Group closes no tab: its tabs go to the group beside it.
    run(&view, cx, right, Action::CloseGroup);
    assert_eq!(groups(&view, cx), [left]);
    assert_eq!(
        cx.update(|_, cx| view.read(cx).browser_tab_ids(cx)),
        [pages[0]]
    );
    assert_eq!(tabs(&view, cx, left).len(), herdr + 1);
    assert_eq!(herdr_tabs(&view, cx), herdr);
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
}

#[gpui::test]
fn a_tabs_close_button_in_a_split_closes_the_tab_through_its_confirmation(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let close = cx.debug_bounds("close-tab-t1").unwrap();
    cx.simulate_click(close.center(), Modifiers::none());
    draw(cx);
    // As without a split: the tab is closed in Herdr, which the dialog
    // guards, rather than taken out of one strip.
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::ConfirmClose));
        let snapshot = view.live.snapshot.as_ref().unwrap();
        assert_eq!(
            view.menu.close,
            crate::close_modal::CloseConfirmation::capture_tab(snapshot, "t1")
        );
    });
}

#[gpui::test]
fn a_group_whose_last_tab_closes_closes_too(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open_pages(&view, cx, 1);
    let pages = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // Empty since the split, the new group stays open.
    assert_eq!(groups(&view, cx).len(), 2);
    move_page(&view, cx, right, pages[0]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.close_strip_tab(Pick::Page(pages[0]), window, cx)
        })
    });
    assert_eq!(groups(&view, cx), [left]);
}

#[gpui::test]
fn the_menu_opens_from_the_strip_and_steps_with_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let button = cx.debug_bounds("tab-actions").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::Group)));
    for row in ["group-menu-NewBrowserTab", "group-menu-Split"] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(cx.debug_bounds("group-menu-CloseGroup").is_none());
    cx.simulate_keystrokes("down");
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.group.as_ref().unwrap().selected),
        Some(0)
    );
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

mod any_repository;
mod close_others;
mod closed_elsewhere;
mod new_tabs;
mod orchestrator;
#[cfg(unix)]
mod tunnels;

/// Showing and hiding VS Code from the menu, in place of a title bar button.
mod vs_code;
