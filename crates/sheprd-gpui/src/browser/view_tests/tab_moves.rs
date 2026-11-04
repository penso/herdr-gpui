use super::*;
use crate::{
    browser::{Pick, TabId},
    controls::Command,
};

fn run(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, command: Command) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

fn open_page(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_browser_tab(None, window, cx)));
    draw(cx);
}

fn pages(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<TabId> {
    cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))
}

fn shown(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<Pick> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_pick(view.active_group().unwrap())
    })
}

fn herdr_tabs(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_ref().unwrap();
        snapshot.tabs.iter().map(|tab| tab.tab_id.clone()).collect()
    })
}

/// The daemon knows only its own tabs, so a page moves among the pages, and
/// never asks it, even one that cannot move tabs at all.
#[gpui::test]
fn move_tab_moves_the_page_shown_among_the_pages(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    for _ in 0..3 {
        open_page(&view, cx);
    }
    let [a, b, c] = [0, 1, 2].map(TabId::test);
    assert_eq!(pages(&view, cx), [a, b, c]);
    assert_eq!(shown(&view, cx), Some(Pick::Page(c)));
    let terminals = herdr_tabs(&view, cx);
    for (command, order) in [
        (Command::MoveTabPrevious, [a, c, b]),
        (Command::MoveTabPrevious, [c, a, b]),
        // The first page wraps around to the end, past the terminal tabs
        // ahead of it, as Herdr wraps its own.
        (Command::MoveTabPrevious, [a, b, c]),
        (Command::MoveTabNext, [c, a, b]),
        (Command::MoveTabNext, [a, c, b]),
    ] {
        run(&view, cx, command);
        assert_eq!(pages(&view, cx), order, "{command:?}");
        assert_eq!(shown(&view, cx), Some(Pick::Page(c)));
    }
    assert_eq!(herdr_tabs(&view, cx), terminals);
    view.read_with(cx, |view, _| assert_eq!(view.local_error, None));
}

#[gpui::test]
fn move_tab_leaves_a_lone_page_in_place(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    open_page(&view, cx);
    run(&view, cx, Command::MoveTabNext);
    run(&view, cx, Command::MoveTabPrevious);
    assert_eq!(pages(&view, cx), [TabId::test(0)]);
    assert_eq!(shown(&view, cx), Some(Pick::Page(TabId::test(0))));
    view.read_with(cx, |view, _| assert_eq!(view.local_error, None));
}

/// A terminal tab is still the daemon's to move, so this one, which cannot,
/// says so.
#[gpui::test]
fn move_tab_on_a_terminal_still_asks_the_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    open_page(&view, cx);
    open_page(&view, cx);
    run(&view, cx, Command::TabNumber(1));
    assert_eq!(shown(&view, cx), Some(Pick::Herdr("t0".into())));
    run(&view, cx, Command::MoveTabNext);
    assert_eq!(pages(&view, cx), [TabId::test(0), TabId::test(1)]);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error.as_deref(),
            Some("Moving tabs needs a newer Herdr daemon.")
        )
    });
}

/// In a split, a page moves among its own group's pages: the store keeps one
/// order for the workspace, interleaving both groups' pages, and the other
/// group's strip and choice stay as they were.
#[gpui::test]
fn move_tab_stays_within_the_group_in_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    for _ in 0..4 {
        open_page(&view, cx);
    }
    let [p0, p1, p2, p3] = [0, 1, 2, 3].map(TabId::test);
    let (left, right) = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let left = view.active_group().unwrap();
            view.split_group(left, window, cx);
            let right = view.active_group().unwrap();
            view.move_tab_to_group(Pick::Page(p1), right, window, cx);
            view.move_tab_to_group(Pick::Page(p3), right, window, cx);
            (left, right)
        })
    });
    draw(cx);
    let strip = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, group| {
        cx.update(|_, cx| {
            view.read(cx)
                .group_tabs(group, cx)
                .into_iter()
                .filter_map(|pick| match pick {
                    Pick::Page(id) => Some(id),
                    Pick::Herdr(_) => None,
                })
                .collect::<Vec<_>>()
        })
    };
    let picks = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (view.group_pick(left), view.group_pick(right))
        })
    };
    assert_eq!(pages(&view, cx), [p0, p1, p2, p3]);
    assert_eq!(strip(&view, cx, left), [p0, p2]);
    assert_eq!(strip(&view, cx, right), [p1, p3]);
    let shown = picks(&view, cx);
    assert_eq!(shown.1, Some(Pick::Page(p3)));
    for (command, order) in [
        (Command::MoveTabPrevious, [p3, p1]),
        // Wraps within the group, not past the other group's pages.
        (Command::MoveTabPrevious, [p1, p3]),
        (Command::MoveTabNext, [p3, p1]),
    ] {
        run(&view, cx, command);
        assert_eq!(strip(&view, cx, right), order, "{command:?}");
        assert_eq!(strip(&view, cx, left), [p0, p2], "{command:?}");
        assert_eq!(picks(&view, cx), shown, "{command:?}");
    }
}
