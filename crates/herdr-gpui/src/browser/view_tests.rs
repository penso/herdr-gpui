#![allow(clippy::unwrap_used)]

use super::{Location, Store, WebUrl, view::scope};
#[cfg(unix)]
use crate::control::{Placed, Target};
use crate::{
    HerdrWindow,
    sidebar::layout_tests::{fixture_window, full_draw, snapshot},
};
use gpui::{Entity, VisualTestContext};
use std::sync::Arc;

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

fn url(value: &str) -> Location {
    Location::Web {
        url: WebUrl::try_from(value).unwrap(),
    }
}

/// The fixture window, showing workspace `w0` as a connected daemon would.
fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut shown = snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

/// Needs a build that shows pages: elsewhere a new tab opens nothing.
#[cfg(any(target_os = "macos", windows))]
mod embedded;

/// Editor groups. Blank browser tabs need no native page, so none is
/// created, and a split alone needs no page at all.
mod groups;

/// A browser tab is this client's own, so dropping one reorders the
/// workspace's tabs at once, without a daemon.
#[gpui::test]
fn dragging_a_browser_tab_reorders_the_workspace_s_tabs(cx: &mut gpui::TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px};

    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(px(1600.), px(600.)));
    let ids = cx.update(|_, cx| {
        let scope = scope(&view.read(cx).endpoints[0]);
        (0..3)
            .map(|_| {
                Store::update(cx, |store| {
                    store.open(scope.clone(), "w0", None, None).unwrap()
                })
            })
            .collect::<Vec<_>>()
    });
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.appear = Default::default()));
    draw(cx);
    draw(cx);
    let order = |view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let scope = scope(&view.read(cx).endpoints[0]);
            cx.global::<Store>()
                .in_workspace(&scope, "w0")
                .map(|tab| tab.id)
                .collect::<Vec<_>>()
        })
    };
    let first = cx.debug_bounds("browser-tab-0").unwrap();
    let second = cx.debug_bounds("browser-tab-1").unwrap();
    // Browser tabs move among their own: over the Herdr tab ahead of them,
    // the first stays first.
    let herdr = cx.debug_bounds("tab-t0").unwrap();
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    for _ in 0..2 {
        cx.simulate_mouse_move(herdr.center(), MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    view.read_with(cx, |view, _| {
        assert!(!view.tab_drag.as_ref().unwrap().has_target())
    });
    let over = point(second.right() - px(2.), first.center().y);
    for _ in 0..2 {
        cx.simulate_mouse_move(over, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    cx.simulate_mouse_up(over, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.tab_drag.is_none()));
    assert_eq!(order(&view, cx), [ids[1], ids[0], ids[2]]);
    // The release was the drop, not a click showing the tab.
    view.read_with(cx, |view, _| {
        assert_ne!(
            view.group_pick(view.group_slots()[0].id),
            Some(super::Pick::Page(ids[0]))
        )
    });
    assert_eq!(
        cx.debug_bounds("browser-tab-1").unwrap().left(),
        first.left()
    );
}

/// Tabs a workspace already has appear at once; one that opens later grows
/// into the strip.
#[gpui::test]
fn a_tab_that_opens_grows_into_the_strip(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let tab = cx.debug_bounds("tab-t0").unwrap();
    assert!(tab.size.width >= gpui::px(crate::TAB_WIDTH));
    view.read_with(cx, |view, _| assert!(!view.tabs_growing()));
    // Held from before it starts, so no frame can outrun it: narrow and clear.
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.appear.hold()));
    cx.update(|_, cx| {
        let scope = scope(&view.read(cx).endpoints[0]);
        Store::update(cx, |store| store.open(scope, "w0", None, None));
    });
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.tabs_growing()));
    let growing = cx.debug_bounds("browser-tab-0").unwrap();
    assert!(
        growing.size.width < gpui::px(crate::TAB_WIDTH / 2.),
        "{growing:?}"
    );

    // Closed, it shrinks out where it stood, from its full width.
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.appear = Default::default()));
    draw(cx);
    let whole = cx.debug_bounds("browser-tab-0").unwrap();
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.appear.hold()));
    cx.update(|_, cx| Store::update(cx, |store| store.close(crate::browser::TabId::test(0))));
    draw(cx);
    assert!(cx.debug_bounds("browser-tab-0").is_none());
    let leaving = cx.debug_bounds("leaving-tab").unwrap();
    assert_eq!(leaving.left(), whole.left());
    // Its measured width, within a pixel or two of the tab it replaces.
    assert!(
        (leaving.size.width - whole.size.width).abs() < gpui::px(4.),
        "{leaving:?} {whole:?}"
    );
    view.read_with(cx, |view, _| assert!(view.tabs_growing()));
}

/// Opens a request's tab without switching to it, so no native page is made.
#[cfg(unix)]
fn request(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    daemon: Option<&str>,
    workspace: Option<&str>,
    strict: bool,
) -> Option<Placed> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let target = Target {
                daemon: daemon.map(std::path::Path::new),
                workspace,
                pane: Some("w0:p1"),
            };
            view.open_requested_browser_tab(
                &target,
                strict,
                &url("http://localhost:3000/"),
                false,
                window,
                cx,
            )
        })
    })
}

#[cfg(unix)]
#[gpui::test]
fn requests_open_tabs_only_in_a_workspace_the_window_shows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let socket = view.read_with(cx, |view, _| {
        view.endpoints[0]
            .connection
            .target
            .socket_path()
            .ok()
            .map(|path| path.to_string_lossy().into_owned())
    });
    // No named workspace: the one the window shows.
    assert!(matches!(
        request(&view, cx, None, None, true),
        Some(Placed::Opened { workspace_id }) if workspace_id == "w0"
    ));
    assert!(matches!(
        request(&view, cx, None, Some("w1"), true),
        Some(Placed::Opened { workspace_id }) if workspace_id == "w1"
    ));
    assert!(request(&view, cx, None, Some("w_missing"), true).is_none());
    // Another daemon's socket never matches strictly, but its workspace ID
    // still finds the window once socket spellings are ignored.
    assert!(
        request(
            &view,
            cx,
            Some("/elsewhere/herdr-client.sock"),
            Some("w0"),
            true
        )
        .is_none()
    );
    assert!(
        request(
            &view,
            cx,
            Some("/elsewhere/herdr-client.sock"),
            Some("w0"),
            false
        )
        .is_some()
    );
    assert!(request(&view, cx, Some("/elsewhere/herdr-client.sock"), None, false).is_none());
    if let Some(socket) = socket {
        assert!(request(&view, cx, Some(&socket), Some("w0"), true).is_some());
    }
    // Opened without focus: the terminal stays in front.
    draw(cx);
    assert!(cx.debug_bounds("terminal").is_some());
    assert!(cx.debug_bounds("browser-tab-0").is_some());
    // The same pane showing the same page again got its tab back each time:
    // one tab in w0 and one in w1.
    cx.update(|_, cx| {
        let store = cx.global::<Store>();
        assert_eq!(store.opened_by("w0:p1").count(), 2);
    });
}

#[gpui::test]
fn tabs_of_a_closed_workspace_are_forgotten_but_a_restart_keeps_them(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = window(cx);
    let tab_scope = view.read_with(cx, |view, _| scope(&view.endpoints[0]));
    let open = |cx: &mut VisualTestContext, workspace: &str| {
        cx.update(|_, cx| {
            Store::update(cx, |store| {
                store.open(
                    tab_scope.clone(),
                    workspace,
                    Some(url("https://a.test/")),
                    None,
                )
            })
            .unwrap()
        })
    };
    let (kept, closed) = (open(cx, "w0"), open(cx, "w2"));
    let exists =
        |cx: &mut VisualTestContext, id| cx.update(|_, cx| cx.global::<Store>().get(id).is_some());
    let poll = |view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, boot: &str, count| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let mut next = snapshot(count);
                next.boot_id = boot.into();
                next.focused_workspace_id = Some("w0".into());
                view.live.snapshot = Some(Arc::new(next));
                view.poll_browser(window, cx);
            })
        });
    };
    poll(&view, cx, "boot-1", 3);
    // A daemon that restarted without w2 proves nothing about w2.
    poll(&view, cx, "boot-2", 2);
    assert!(exists(cx, closed));
    poll(&view, cx, "boot-2", 3);
    // The same daemon dropping w2 means it was closed.
    poll(&view, cx, "boot-2", 2);
    assert!(!exists(cx, closed));
    assert!(exists(cx, kept));
}

/// Notes need a page to annotate, which Linux builds do not show.
#[cfg(any(target_os = "macos", windows))]
mod notes;
