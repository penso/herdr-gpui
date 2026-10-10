//! VS Code opens as a tab of the editor groups, like a browser tab: in the
//! group in use, beside the workspace's other tabs, to split beside a
//! terminal or show alone. A workspace has one VS Code tab.
use super::*;
use crate::controls::Command;
#[cfg(any(target_os = "macos", windows))]
use crate::{browser::Shown, code_server::Server};

#[cfg(any(target_os = "macos", windows))]
fn refuses(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::code_server::Error::TokenRefused.into())
}

/// The window with a server set, which refuses its token, so no page is
/// ever created.
#[cfg(any(target_os = "macos", windows))]
fn with_server(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
        view.browser.code_server.probe = refuses;
    });
    draw(cx);
    (view, cx)
}

fn code_tab(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
) -> Option<crate::browser::TabId> {
    cx.update(|_, cx| view.read(cx).code_tab_id(cx))
}

#[cfg(any(target_os = "macos", windows))]
fn shown(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<Shown> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_slots()
            .into_iter()
            .map(|slot| view.group_shown(slot.id, cx))
            .collect()
    })
}

#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn vs_code_opens_as_a_tab_in_the_group_in_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = with_server(cx);
    assert!(code_tab(&view, cx).is_none());
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    cx.run_until_parked();
    draw(cx);
    let id = code_tab(&view, cx).unwrap();
    // The lone group shows it, as a tab of its strip, and says why it has
    // no page yet; there is no side panel.
    assert_eq!(shown(&view, cx), [Shown::Page(id)]);
    let listed: Vec<_> = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        cx.global::<Store>()
            .in_workspace(&tab_scope, "w0")
            .map(|tab| tab.id)
            .collect()
    });
    assert_eq!(listed, [id]);
    assert!(cx.debug_bounds("code-unreachable").is_some());
    assert!(cx.debug_bounds("code").is_none());
    assert!(cx.debug_bounds("code-resize").is_none());
    // VS Code fills its tab, without a browser's toolbar: no navigation,
    // address, notes, or link out.
    let area = cx.debug_bounds("browser").unwrap();
    for control in [
        "browser-back",
        "browser-forward",
        "browser-reload",
        "browser-annotate",
        "browser-external",
    ] {
        assert!(cx.debug_bounds(control).is_none(), "{control}");
    }
    let status = cx.debug_bounds("code-unreachable").unwrap();
    assert!(status.top() >= area.top());

    // A browser tab beside it keeps its toolbar.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::NewBrowserTab, window, cx)
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("browser-reload").is_some());
}

/// Split, VS Code shows in one group beside a terminal in the other; opened
/// again in the other group, it is the same tab there.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn the_one_vs_code_tab_shows_beside_a_terminal_or_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = with_server(cx);
    let first = view.read_with(cx, |view, _| view.group_slots()[0].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.split_group(first, window, cx)));
    draw(cx);
    let second = view.read_with(cx, |view, _| view.group_slots()[1].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code(Some(second), window, cx)));
    cx.run_until_parked();
    draw(cx);
    let id = code_tab(&view, cx).unwrap();
    let groups = shown(&view, cx);
    assert_eq!(groups[1], Shown::Page(id));
    assert_ne!(
        groups[0],
        Shown::Page(id),
        "the first still shows its terminal"
    );

    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code(Some(first), window, cx)));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(code_tab(&view, cx), Some(id), "still one VS Code tab");
    assert_eq!(shown(&view, cx)[0], Shown::Page(id));
}

/// Without a server there is nothing to open, and builds that show no pages
/// open no VS Code.
#[gpui::test]
fn vs_code_needs_a_server_and_a_build_that_shows_pages(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    draw(cx);
    assert!(code_tab(&view, cx).is_none(), "no server set");
    view.update(cx, |view, _| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(Command::OpenCode, window, cx)));
    draw(cx);
    assert_eq!(
        code_tab(&view, cx).is_some(),
        crate::browser::EMBEDDED,
        "only builds that show pages open it"
    );
}
