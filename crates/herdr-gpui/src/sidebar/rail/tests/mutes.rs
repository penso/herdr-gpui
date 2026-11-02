use super::*;
use crate::notification_mutes::{Daemon, Mutes, Scope};

fn mute(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, endpoint: usize, id: &str) {
    view.update(cx, |view, cx| {
        let daemon = Daemon::of(&view.endpoints[endpoint]);
        let snapshot = if endpoint == view.selected_endpoint {
            view.live.snapshot.clone()
        } else {
            view.endpoints[endpoint].live.snapshot.clone()
        }
        .unwrap();
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
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

/// One host: the rail keeps no icon column until a workspace is muted, then
/// draws its mark there and keeps every row's dot in one column, in every
/// layout.
#[gpui::test]
fn a_muted_workspace_takes_the_icon_column_and_the_dots_stay_aligned(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 1);
    let plain = placed(cx, "rail-dot-rail-workspace-local-w0");
    mute(&view, cx, 0, "w1");
    for mode in LayoutMode::ALL {
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            cx.notify();
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let rail = placed(cx, "sidebar-rail");
        let mark = placed(cx, "rail-muted-rail-workspace-local-w1");
        let row = placed(cx, "highlight-rail-workspace-local-w1");
        assert!(contains(row, mark), "{mode:?}: {mark:?} outside {row:?}");
        assert!(contains(rail, row), "{mode:?}: {row:?} outside the rail");
        assert!(
            cx.debug_bounds("rail-muted-rail-workspace-local-w0")
                .is_none(),
            "{mode:?}"
        );
        let dots = [
            "rail-dot-rail-workspace-local-w0",
            "rail-dot-rail-workspace-local-w1",
            "rail-dot-rail-agent-local-p0",
            "rail-dot-rail-agent-local-p1",
        ]
        .map(|selector| placed(cx, selector));
        for dot in dots {
            assert!(near(dot.left(), dots[0].left()), "{mode:?}: {dot:?}");
        }
        assert!(mark.right() <= dots[1].left(), "{mode:?}: before the dot");
        let number = placed(cx, "rail-mark-rail-workspace-local-w1");
        assert!(number.right() <= mark.left(), "{mode:?}: after the number");
    }
    // The column opened for the mute moved the dots over.
    assert!(dots_moved(
        plain,
        placed(cx, "rail-dot-rail-workspace-local-w0")
    ));
}

fn dots_moved(before: Bounds<Pixels>, after: Bounds<Pixels>) -> bool {
    after.left() > before.left()
}

/// Several hosts already keep the icon column; a mute on one host marks
/// only that host's workspace, never the same ID on another.
#[gpui::test]
fn a_mute_marks_only_its_own_hosts_workspace(cx: &mut TestAppContext) {
    let (view, cx) = rail_window(cx, 2);
    let before = placed(cx, "rail-dot-rail-workspace-local-w0");
    mute(&view, cx, 1, "w0");
    assert!(
        cx.debug_bounds("rail-muted-rail-workspace-ssh:host1-w0")
            .is_some()
    );
    assert!(
        cx.debug_bounds("rail-muted-rail-workspace-local-w0")
            .is_none()
    );
    let after = placed(cx, "rail-dot-rail-workspace-local-w0");
    assert!(near(before.left(), after.left()), "{before:?} {after:?}");
}

#[test]
fn a_muted_workspaces_tooltip_says_so() {
    assert_eq!(
        muted_word(Some("done".to_owned()), false).as_deref(),
        Some("done")
    );
    assert_eq!(
        muted_word(Some("done".to_owned()), true).as_deref(),
        Some("done, notifications muted")
    );
    assert_eq!(
        muted_word(None, true).as_deref(),
        Some("notifications muted")
    );
    assert_eq!(muted_word(None, false), None);
}
