use super::*;
use crate::{
    config::LayoutMode,
    notification_mutes::{Daemon, Mutes, Scope},
};
use gpui::{MouseButton, VisualTestContext};

fn muted(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, id: &str) -> bool {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let snapshot = view.live.snapshot.clone().unwrap();
        Mutes::of(cx).is_some_and(|mutes| {
            mutes
                .silenced(&Daemon::of(&view.endpoints[0]), &snapshot)
                .workspace(id)
        })
    })
}

fn window(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.simulate_resize(size(px(800.), px(672.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

#[gpui::test]
fn a_right_click_mutes_a_workspace_marks_its_row_and_unmutes_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    assert!(cx.debug_bounds("muted-herdr").is_none());
    for after in [true, false] {
        let row = cx.debug_bounds("row-herdr").unwrap();
        let position = point(px(200.), row.center().y);
        cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        // The toggle sits in the header, beside the workspace's name.
        let header = cx.debug_bounds("workspace-menu-header").unwrap();
        let toggle = cx
            .debug_bounds(if after {
                "workspace-menu-Mute notifications"
            } else {
                "workspace-menu-Unmute notifications"
            })
            .unwrap();
        let icon = cx.debug_bounds("workspace-menu-mute-icon").unwrap();
        let name = cx.debug_bounds("workspace-menu-name").unwrap();
        assert!(
            header.contains(&toggle.origin) && toggle.right() <= header.right(),
            "{toggle:?} in {header:?}"
        );
        assert!(toggle.contains(&icon.center()));
        assert!(name.right() <= toggle.left(), "{name:?} beside {toggle:?}");
        cx.simulate_click(toggle.center(), Modifiers::none());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none(), "applied at once"));
        assert_eq!(muted(&view, cx, "w0"), after);
        if after {
            let row = cx.debug_bounds("row-herdr").unwrap();
            let mark = cx.debug_bounds("muted-herdr").unwrap();
            assert!(
                mark.left() >= row.left() && mark.right() <= row.right(),
                "{mark:?} in {row:?}"
            );
        }
    }
    // Only the muted workspace was ever marked.
    assert!(cx.debug_bounds("muted-agent-launcher").is_none());
}

/// Whatever the layout, a muted workspace's row carries the mark inside it,
/// a Git checkout or not, and never over its name.
#[gpui::test]
fn every_sidebar_layout_marks_a_muted_workspace_inside_its_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        let daemon = Daemon::of(&view.endpoints[0]);
        let snapshot = view.live.snapshot.clone().unwrap();
        // w0 has a branch but no worktree; w3 is a repository's checkout.
        for id in ["w0", "w3"] {
            assert!(Mutes::update(cx, |mutes| {
                mutes.set(
                    &daemon,
                    &snapshot,
                    Scope::Workspace,
                    id,
                    true,
                    std::time::SystemTime::now(),
                )
            }));
        }
        cx.notify();
    });
    let mut names = 0;
    for mode in LayoutMode::ALL {
        if mode == LayoutMode::Devices {
            continue;
        }
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            full_draw(window, cx).clear(cx)
        });
        for label in ["herdr", "agent-launcher"] {
            let row = cx
                .debug_bounds(format!("row-{label}").leak())
                .unwrap_or_else(|| panic!("{mode:?}: no row {label}"));
            let mark = cx
                .debug_bounds(format!("muted-{label}").leak())
                .unwrap_or_else(|| panic!("{mode:?}: {label} has no muted mark"));
            assert!(
                mark.left() >= row.left()
                    && mark.right() <= row.right()
                    && mark.top() >= row.top()
                    && mark.bottom() <= row.bottom(),
                "{mode:?}: {label}'s {mark:?} in {row:?}"
            );
            let name =
                cx.update(|_, cx| cx.global::<TextProbes>().0.get(label).map(|probe| probe.0));
            if let Some(name) = name.filter(|name| name.intersects(&row)) {
                names += 1;
                assert!(
                    !name.intersects(&mark),
                    "{mode:?}: {label}'s mark {mark:?} covers its name {name:?}"
                );
            }
        }
    }
    assert!(names > 0, "no layout painted a name to compare against");
}
