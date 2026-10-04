//! Workspace context menu, PR menu, menu anchor, and rename dialog checks.
use super::*;
use gpui::MouseButton;

pub(super) fn check_workspace_menu_rows(
    view: &Entity<HerdrWindow>,
    before: &ClientShellSnapshot,
    cx: &mut gpui::VisualTestContext,
) -> Bounds<Pixels> {
    // Exercise the actual right-click overlay and platform text handler, without a daemon.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.live.status = crate::state::ConnectionStatus::Connected;
        })
    });
    let parent = cx.debug_bounds("row-agent-launcher").unwrap();
    cx.simulate_mouse_down(parent.center(), MouseButton::Right, Default::default());
    cx.simulate_mouse_up(parent.center(), MouseButton::Right, Default::default());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::Workspace));
        assert_eq!(view.read(cx).live.snapshot.as_deref(), Some(before));
    });
    assert!(cx.debug_bounds("workspace-menu-Close group").is_some());
    assert!(cx.debug_bounds("workspace-menu-New worktree").is_some());
    // Actions share the session picker's trailing 14px icon in a 24px slot.
    for (row, icon, text) in [
        (
            "workspace-menu-Rename",
            "workspace-menu-icon-Rename",
            "workspace-menu-label-Rename",
        ),
        (
            "workspace-menu-Close group",
            "workspace-menu-icon-Close group",
            "workspace-menu-label-Close group",
        ),
        (
            "workspace-menu-New worktree",
            "workspace-menu-icon-New worktree",
            "workspace-menu-label-New worktree",
        ),
        (
            "workspace-menu-Open worktree...",
            "workspace-menu-icon-Open worktree...",
            "workspace-menu-label-Open worktree...",
        ),
    ] {
        let label = row;
        let row = cx.debug_bounds(row).unwrap();
        let icon = cx.debug_bounds(icon).unwrap();
        assert_eq!(icon.size, size(px(14.), px(14.)), "{label}");
        assert_eq!(row.right() - icon.right(), px(13.), "{label}");
        let text = cx.debug_bounds(text).unwrap();
        assert!(
            text.right() <= icon.left(),
            "{label}: label must precede icon"
        );
        assert!(
            (icon.center().y - row.center().y).abs() <= px(1.),
            "{label}"
        );
    }
    crate::menu::workspace_tests::check_menu_interactions(view, cx);
    // PR data is fixture-only: no daemon, local Git, or GitHub calls in layout tests.
    crate::menu::workspace_tests::check_pr_fences(view, cx);
    parent
}

pub(super) fn check_pr_menu(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    for width in [320., 800.] {
        cx.simulate_resize(size(px(width), px(600.)));
        for state in 0..5 {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.pr.clear();
                    view.menu.pr.loading = state == 0;
                    if state >= 2 {
                        view.menu.github = crate::github::Auth::connected_fixture();
                        view.menu.pr.value = Some(crate::pull_request::fixture().unwrap());
                    }
                    if state == 3 {
                        view.menu.pr.message = Some("Authentication unavailable".into());
                    }
                    cx.notify();
                });
                full_draw(window, cx).clear(cx);
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(panel.bottom() <= px(600.));
            let open_row = cx.debug_bounds("workspace-menu-Open worktree...").unwrap();
            // Preserve the content budget apart from the action row and target header.
            let row_height = cx.update(|_, cx| px(view.read(cx).config.ui.line_height() + 12.));
            let header_height = cx
                .debug_bounds("workspace-menu-header")
                .unwrap()
                .size
                .height
                + px(4.);
            assert!((open_row.size.height - row_height).abs() <= px(1.));
            assert!(
                panel.size.height < px(320.) + row_height + header_height,
                "PR menu should size to its content: {panel:?}"
            );
            assert!(cx.debug_bounds("workspace-pr").is_some());
            if state >= 2 {
                let title = cx.debug_bounds("workspace-pr-title").unwrap();
                assert!(title.left() >= panel.left() && title.right() <= panel.right());
            }
        }
    }
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.menu.pr.clear());
    });
}

pub(super) fn check_menu_anchor(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    for dialog in [false, true] {
        if dialog {
            cx.simulate_keystrokes("down enter");
        }
        for anchor in [
            point(px(200.), px(400.)),
            point(px(795.), px(595.)),
            point(px(-10.), px(-20.)),
        ] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.menu.anchor = anchor;
                    cx.notify();
                });
                full_draw(window, cx).clear(cx);
            });
            let panel = cx.debug_bounds("menu-panel").unwrap();
            assert_eq!(panel.size.width, px(if dialog { 420. } else { 340. }));
            if dialog {
                // A dialog is a modal decision, so it centres on the window and
                // ignores the anchor the row menu was opened from.
                let offset = panel.center() - point(px(400.), px(300.));
                assert!(
                    offset.x.abs() <= px(1.) && offset.y.abs() <= px(1.),
                    "{anchor:?}: {panel:?}"
                );
            } else {
                let expected = |position: Pixels, extent: Pixels, viewport: Pixels| {
                    if position + extent > viewport {
                        (viewport - extent - px(12.)).round()
                    } else if position < px(0.) {
                        px(12.)
                    } else {
                        position.round()
                    }
                };
                assert_eq!(panel.left(), expected(anchor.x, panel.size.width, px(800.)));
                assert_eq!(panel.top(), expected(anchor.y, panel.size.height, px(600.)));
            }
            assert!(panel.right() <= px(800.) && panel.bottom() <= px(600.));
        }
    }
}

pub(super) fn check_rename_dialog(
    view: &Entity<HerdrWindow>,
    parent: Bounds<Pixels>,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.menu.anchor = parent.center();
            cx.notify();
        });
        full_draw(window, cx).clear(cx);
    });
    cx.simulate_input("\u{65e5}\u{672c}\u{1f600}");
    cx.update(|window, cx| {
        use gpui::EntityInputHandler;
        view.update(cx, |view, cx| {
            assert_eq!(
                view.menu.input.as_ref().unwrap().text,
                "\u{65e5}\u{672c}\u{1f600}"
            );
            view.replace_and_mark_text_in_range(Some(2..4), "\u{304b}", Some(1..1), window, cx);
            assert_eq!(view.marked_text_range(window, cx), Some(2..3));
            assert_eq!(
                view.selected_text_range(false, window, cx).unwrap().range,
                3..3
            );
            view.replace_text_in_range(None, "\u{6f22}", window, cx);
            assert_eq!(
                view.menu.input.as_ref().unwrap().text,
                "\u{65e5}\u{672c}\u{6f22}"
            );
            assert!(view.marked.is_empty());
            view.command(crate::controls::Command::Workspace, window, cx);
            assert!(view.local_error.is_none());
        });
        full_draw(window, cx).clear(cx);
        view.update(cx, |view, cx| {
            let bounds = view
                .bounds_for_range(3..3, Bounds::default(), window, cx)
                .unwrap();
            assert!(
                view.menu
                    .input
                    .as_ref()
                    .unwrap()
                    .bounds
                    .contains(&bounds.origin)
            );
        });
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        // No handle: a queue failure must preserve the draft, not claim success.
        assert!(
            view.read(cx).menu.page
                == Some(crate::menu::Page::Dialog(
                    crate::menu::WorkspaceAction::Rename
                ))
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("   ");
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert_eq!(view.read(cx).menu.input.as_ref().unwrap().text, "   ");
        assert!(
            view.read(cx).menu.page
                == Some(crate::menu::Page::Dialog(
                    crate::menu::WorkspaceAction::Rename
                ))
        );
    });
    assert!(cx.debug_bounds("dialog-error").is_some());
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).menu.input.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
}
