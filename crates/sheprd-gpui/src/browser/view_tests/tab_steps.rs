use super::*;
use crate::{
    browser::{Pick, Shown, TabId},
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

/// What the group in use picked, and what it draws.
fn showing(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> (Option<Pick>, Shown) {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let group = view.active_group().unwrap();
        (view.group_pick(group), view.group_shown(group, cx))
    })
}

fn terminal(tab: &str) -> (Option<Pick>, Shown) {
    (Some(Pick::Herdr(tab.into())), Shown::Terminal)
}

fn page() -> (Option<Pick>, Shown) {
    let page = TabId::test(0);
    (Some(Pick::Page(page)), Shown::Page(page))
}

#[gpui::test]
fn tab_commands_leave_a_page_for_the_only_terminal_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.tabs.retain(|tab| tab.tab_id == "t0");
        })
    });
    draw(cx);
    open_page(&view, cx);
    assert_eq!(showing(&view, cx), page());
    // The daemon already focuses the tab behind the page, so asking it to
    // would change nothing.
    run(&view, cx, Command::NextTab);
    assert_eq!(showing(&view, cx), terminal("t0"));
    run(&view, cx, Command::PreviousTab);
    assert_eq!(showing(&view, cx), page());
    run(&view, cx, Command::TabNumber(1));
    assert_eq!(showing(&view, cx), terminal("t0"));
    run(&view, cx, Command::TabNumber(2));
    assert_eq!(showing(&view, cx), page());
    // A number past the strip goes nowhere.
    run(&view, cx, Command::TabNumber(3));
    assert_eq!(showing(&view, cx), page());
}

#[gpui::test]
fn tab_commands_step_through_pages_in_strip_order(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    open_page(&view, cx);
    assert_eq!(showing(&view, cx), page());
    // The strip is t0, t1, then the page. The daemon still focuses t0, the
    // tab the page was opened from, yet Next wraps around to it.
    run(&view, cx, Command::NextTab);
    assert_eq!(showing(&view, cx), terminal("t0"));
    // Previous wraps from the first tab back to the page, and on to t1. A
    // step from one terminal to another is the daemon's to make, as the
    // focus fence tests show.
    run(&view, cx, Command::PreviousTab);
    assert_eq!(showing(&view, cx), page());
    run(&view, cx, Command::PreviousTab);
    assert_eq!(showing(&view, cx), terminal("t1"));
    run(&view, cx, Command::NextTab);
    assert_eq!(showing(&view, cx), page());
    run(&view, cx, Command::TabNumber(1));
    assert_eq!(showing(&view, cx), terminal("t0"));
    run(&view, cx, Command::TabNumber(3));
    assert_eq!(showing(&view, cx), page());
}

#[gpui::test]
fn tab_commands_wait_for_a_focus_change_in_flight(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    open_page(&view, cx);
    run(&view, cx, Command::TabNumber(1));
    assert_eq!(showing(&view, cx), terminal("t0"));
    // Next Tab asked the daemon for t1, which has not answered yet.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.activation_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
        })
    });
    run(&view, cx, Command::TabNumber(3));
    assert_eq!(showing(&view, cx), terminal("t0"));
    // Its answer lands on t1 rather than taking a page shown since.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.activation_deadline = None;
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_tab_id = Some("t1".into());
            view.terminal_focus_moved("t1", false, cx);
        })
    });
    draw(cx);
    assert_eq!(showing(&view, cx), terminal("t1"));
    run(&view, cx, Command::TabNumber(3));
    assert_eq!(showing(&view, cx), page());
}
