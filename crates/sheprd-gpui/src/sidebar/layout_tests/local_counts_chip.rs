use super::*;
use crate::{
    config::{ChangeCounts, LayoutMode},
    git::{Counted, Status},
};

/// Local counts ride in the dirty mark's chip under the pull request number,
/// in place of the bare mark, and fit the badge column it reserves.
#[gpui::test]
fn local_counts_sit_in_the_dirty_chip_under_the_number(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let now = std::time::Instant::now();
            let input = |branch: &str| crate::pull_request::Input {
                checkout: None,
                repo_key: Some(REPO_KEY.into()),
                branch: branch.into(),
            };
            let mut pr = crate::pull_request::fixture().unwrap();
            pr.number = 7;
            pr.additions = 23;
            pr.deletions = 342;
            view.seed_pr(input("worktree/sidebar-child"), pr, now);
            let counted = |additions, deletions| Counted {
                lines: Status {
                    additions,
                    deletions,
                    untracked: 0,
                },
                dirty: true,
            };
            view.git
                .seed_counts(input("worktree/sidebar-child"), counted(42, 1730), now);
            // A binary-only edit: nothing to count, so the bare mark stays.
            view.git.seed_counts(input("develop"), counted(0, 0), now);
            view.config.sidebar_style.change_counts = ChangeCounts::Local;
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let row = cx.debug_bounds("row-sidebar-child").unwrap();
    let badge = cx.debug_bounds("pr-sidebar-child").unwrap();
    let chip = cx.debug_bounds("dirty-sidebar-child").unwrap();
    let counts = cx.debug_bounds("counts-sidebar-child").unwrap();
    // Under the number's line, flush with the column's edge, inside the row.
    assert!(chip.top() >= badge.top() + px(8.), "{chip:?} {badge:?}");
    assert_eq!(chip.right(), badge.right());
    assert!(badge.left() <= chip.left() && chip.bottom() <= row.bottom());
    assert!(chip.left() < counts.left() && counts.right() <= chip.right());
    // The name only yields the number's line, so it runs on above the
    // wider chip; the detail line beside the chip stops short of it.
    let name = cx.debug_bounds("name-sidebar-child").unwrap();
    assert!(name.right() > chip.left(), "{name:?} {chip:?}");
    assert!(name.right() < badge.right() - px(16.));
    if let Some(detail) = cx.debug_bounds("detail-sidebar-child") {
        assert!(detail.right() <= chip.left(), "{detail:?} {chip:?}");
    }
    // The head's binary edit keeps the bare mark on the number's line.
    let head = cx.debug_bounds("pr-agent-launcher").unwrap();
    let mark = cx.debug_bounds("dirty-agent-launcher").unwrap();
    assert_eq!(mark.size.width, px(12.));
    assert!(mark.top() < head.top() + px(8.));
    assert!(cx.debug_bounds("counts-agent-launcher").is_none());
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        for text in ["#7", "+42", "-1730"] {
            assert!(probes.contains_key(text), "missing {text}");
        }
        assert!(
            !probes.contains_key("+23"),
            "never the pull request's counts"
        );
    });

    // Rows the daemon's sidebar config lays out split the same way.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.sidebar_layout = toml::from_str(
                r#"
                [spaces]
                rows = [["workspace"], ["state_text"]]
            "#,
            )
            .unwrap();
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let chip = cx.debug_bounds("dirty-sidebar-child").unwrap();
    let name = cx.debug_bounds("name-sidebar-child").unwrap();
    let detail = cx.debug_bounds("detail-sidebar-child").unwrap();
    assert!(name.right() > chip.left(), "{name:?} {chip:?}");
    assert!(detail.right() <= chip.left(), "{detail:?} {chip:?}");

    // The card layout puts the same chip on the label line.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.layout.mode = LayoutMode::Superset;
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    let row = cx.debug_bounds("row-sidebar-child").unwrap();
    let chip = cx.debug_bounds("dirty-sidebar-child").unwrap();
    let counts = cx.debug_bounds("counts-sidebar-child").unwrap();
    assert!(row.left() <= chip.left() && chip.right() <= row.right());
    assert!(chip.left() < counts.left() && counts.right() <= chip.right());
    assert!(cx.debug_bounds("name-sidebar-child").unwrap().right() <= chip.left());
}
