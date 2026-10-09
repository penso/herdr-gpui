//! Moving the VS Code tab between its panel and the editor groups.
use super::*;
use crate::{
    browser::{TabId, store::Place},
    code_server::Server,
    controls::Command,
};

fn run(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, command: Command) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

fn refuses(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::Error::CodeTokenRefused)
}

/// The window with a VS Code server set, which refuses its token, and
/// workspace `w0`'s VS Code tab open in its panel.
fn with_code_tab(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut VisualTestContext, TabId) {
    let (view, cx) = window(cx);
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        view.update(cx, |view, _| {
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
            view.browser.code_server.probe = refuses;
        });
        Store::update(cx, |store| {
            store.open_code_tab(tab_scope, "w0", url("http://127.0.0.1:8000/"))
        })
        .unwrap()
    });
    run(&view, cx, Command::ToggleCode);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("code").is_some());
    (view, cx, id)
}

/// The tab's entry in the second group's strip. Debug selectors are looked
/// up as `'static`.
fn strip_tab(id: TabId) -> &'static str {
    Box::leak(format!("g1-browser-tab-{id}").into_boxed_str())
}

fn place(cx: &mut VisualTestContext, id: TabId) -> Place {
    cx.update(|_, cx| cx.global::<Store>().get(id).unwrap().place)
}

#[gpui::test]
fn the_vs_code_tab_moves_into_a_group_and_back(cx: &mut gpui::TestAppContext) {
    let (view, cx, id) = with_code_tab(cx);

    // Into a new group to the right, as a tab of the strips; the panel
    // closes.
    run(&view, cx, Command::MoveCodeToGroup);
    assert_eq!(place(cx, id), Place::CodeGroup);
    assert!(cx.debug_bounds("code").is_none());
    assert!(cx.debug_bounds(strip_tab(id)).is_some());
    view.update(cx, |view, cx| {
        assert_eq!(view.ensure_layout().unwrap().len(), 2);
        assert!(view.group_shows_page(id, cx));
        assert!(!view.shown_code());
    });

    // Toggling shows it where it is, rather than opening the panel.
    run(&view, cx, Command::ToggleCode);
    assert!(cx.debug_bounds("code").is_none());
    view.update(cx, |view, cx| assert!(view.group_shows_page(id, cx)));

    // Moved again, nothing changes.
    run(&view, cx, Command::MoveCodeToGroup);
    view.update(cx, |view, _| {
        assert_eq!(view.ensure_layout().unwrap().len(), 2)
    });

    // Back to the panel: the group shows a terminal again.
    run(&view, cx, Command::MoveCodeToPanel);
    assert_eq!(place(cx, id), Place::Code);
    assert!(cx.debug_bounds("code").is_some());
    assert!(cx.debug_bounds(strip_tab(id)).is_none());
    view.update(cx, |view, cx| {
        assert!(!view.group_shows_page(id, cx));
        assert!(view.shown_code());
    });
}

/// Closed in a strip, the tab is gone, and Toggle VS Code opens a new one
/// in the panel.
#[gpui::test]
fn a_vs_code_tab_closed_in_a_group_reopens_in_the_panel(cx: &mut gpui::TestAppContext) {
    let (view, cx, id) = with_code_tab(cx);
    run(&view, cx, Command::MoveCodeToGroup);
    cx.update(|window, cx| view.update(cx, |view, cx| view.close_browser_tab(id, window, cx)));
    draw(cx);
    cx.update(|_, cx| assert!(cx.global::<Store>().get(id).is_none()));

    run(&view, cx, Command::ToggleCode);
    assert!(cx.debug_bounds("code").is_some());
    view.update(cx, |view, _| assert!(view.shown_code()));
}

/// In a group, the tab waits for its server as the panel does, and says why
/// its page is not there.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_grouped_vs_code_tab_says_its_server_cannot_be_reached(cx: &mut gpui::TestAppContext) {
    let (view, cx, id) = with_code_tab(cx);
    run(&view, cx, Command::MoveCodeToGroup);
    cx.run_until_parked();
    draw(cx);
    view.read_with(cx, |view, _| assert!(!view.browser.pages.contains(id)));
    assert!(cx.debug_bounds("code").is_none());
    assert!(cx.debug_bounds("code-unreachable").is_some());
}
