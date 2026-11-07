use super::*;
use crate::{
    browser::{Pick, Shown},
    control::{Opening, Placed, Placement, Target},
};

/// Opens a request's tab without switching to it, so no native page is made.
fn request(
    view: &Entity<SheprdWindow>,
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
                &Opening {
                    location: &url("http://localhost:3000/"),
                    focus: false,
                    placement: Placement::Tab,
                },
                window,
                cx,
            )
        })
    })
}

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
        Some(Placed::Opened { workspace_id, .. }) if workspace_id == "w0"
    ));
    assert!(matches!(
        request(&view, cx, None, Some("w1"), true),
        Some(Placed::Opened { workspace_id, .. }) if workspace_id == "w1"
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

/// Puts the caller's pane `p0` in Herdr tab `tab` of workspace `w0`.
fn caller_in(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: &str) {
    let fixture: sheprd_client::protocol::ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    let mut pane = fixture.panes[0].clone();
    pane.pane_id = "p0".into();
    pane.workspace_id = "w0".into();
    pane.tab_id = tab.into();
    view.update(cx, |view, _| {
        if let Some(snapshot) = view.live.snapshot.as_mut() {
            Arc::make_mut(snapshot).panes.push(pane);
        }
    });
}

/// Opens `page` beside pane `p0`'s terminal, in workspace `workspace`.
fn beside(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    workspace: &str,
    page: &str,
) -> Option<Placed> {
    beside_with(view, cx, workspace, page, true)
}

fn beside_with(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    workspace: &str,
    page: &str,
    focus: bool,
) -> Option<Placed> {
    let placed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let target = Target {
                daemon: None,
                workspace: Some(workspace),
                pane: Some("p0"),
            };
            view.open_requested_browser_tab(
                &target,
                true,
                &Opening {
                    location: &url(page),
                    focus,
                    placement: Placement::Beside,
                },
                window,
                cx,
            )
        })
    });
    draw(cx);
    placed
}

fn shown(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<Shown> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_slots()
            .into_iter()
            .map(|slot| view.group_shown(slot.id, cx))
            .collect()
    })
}

#[gpui::test]
fn pages_opened_beside_split_off_the_terminal_once(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    caller_in(&view, cx, "t0");
    draw(cx);
    let placed = beside(&view, cx, "w0", "http://localhost:3000/");
    assert!(matches!(
        placed,
        Some(Placed::Opened {
            placement: Placement::Beside,
            ..
        })
    ));
    let first = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[0];
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(first)]);
    // The terminal's group stays in use, so typing still goes to the agent.
    let (home, groups) = view.read_with(cx, |view, _| {
        let groups: Vec<_> = view.group_slots().into_iter().map(|slot| slot.id).collect();
        (view.active_group(), groups)
    });
    assert_eq!(home, Some(groups[0]));
    assert!(cx.debug_bounds("terminal").is_some());

    // Another page joins the page's group instead of splitting again, and
    // showing the first page again brings it back there.
    beside(&view, cx, "w0", "http://localhost:4000/");
    let second = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[1];
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(second)]);
    beside(&view, cx, "w0", "http://localhost:3000/");
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(first)]);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.group_tabs(groups[1], cx),
            [Pick::Page(first), Pick::Page(second)]
        );
        assert_eq!(view.active_group(), Some(groups[0]));
    });
}

#[gpui::test]
fn a_page_beside_a_workspace_not_shown_opens_as_a_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let placed = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let target = Target {
                daemon: None,
                workspace: Some("w1"),
                pane: Some("p1"),
            };
            view.open_requested_browser_tab(
                &target,
                true,
                &Opening {
                    location: &url("http://localhost:3000/"),
                    focus: false,
                    placement: Placement::Beside,
                },
                window,
                cx,
            )
        })
    });
    assert!(matches!(
        placed,
        Some(Placed::Opened { placement: Placement::Tab, workspace_id }) if workspace_id == "w1"
    ));
    draw(cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
}

#[gpui::test]
fn a_page_beside_a_terminal_the_user_left_opens_as_a_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    // The caller runs in tab t1, but the window shows t0: splitting beside
    // t0 would leave the caller hidden and the keyboard with another agent.
    caller_in(&view, cx, "t1");
    draw(cx);
    let placed = beside(&view, cx, "w0", "http://localhost:3000/");
    assert!(matches!(
        placed,
        Some(Placed::Opened {
            placement: Placement::Tab,
            ..
        })
    ));
    assert_eq!(view.read_with(cx, |view, _| view.group_slots().len()), 1);
}

#[gpui::test]
fn a_background_page_beside_leaves_the_keyboard_where_it_was(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    caller_in(&view, cx, "t0");
    draw(cx);
    beside(&view, cx, "w0", "http://localhost:3000/");
    let groups: Vec<_> = view.read_with(cx, |view, _| {
        view.group_slots().into_iter().map(|slot| slot.id).collect()
    });
    let first = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[0];
    // The user moves to the page's group; a page opened with --no-focus
    // joins its strip without replacing the page the user is reading or
    // handing the keyboard to the agent's terminal.
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_group(groups[1], window, cx)));
    let placed = beside_with(&view, cx, "w0", "http://localhost:4000/", false);
    assert!(matches!(
        placed,
        Some(Placed::Opened {
            placement: Placement::Beside,
            ..
        })
    ));
    let second = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[1];
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(first)]);
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).group_tabs(groups[1], cx),
            [Pick::Page(first), Pick::Page(second)]
        );
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(groups[1])
    );
    // With focus, the page is shown and the terminal's group takes the
    // keyboard back.
    beside(&view, cx, "w0", "http://localhost:5000/");
    let third = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[2];
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(third)]);
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(groups[0])
    );
}
