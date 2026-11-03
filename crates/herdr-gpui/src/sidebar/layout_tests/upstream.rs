use super::*;

#[gpui::test]
fn normal_sidebar_gives_upstream_counts_their_natural_width(cx: &mut gpui::TestAppContext) {
    use crate::config::{Density, LayoutMode, Style};

    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut snapshot = snapshot(2);
        snapshot.workspaces[0].git_ahead_behind = Some((4, 3));
        view.live.snapshot = Some(Arc::new(snapshot));
        view.config.layout.mode = LayoutMode::new(Density::Normal, Style::Flat);
        view.sidebar_width = Some(320.);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();

    for (font_size, width, branch) in [
        (12., 320., "main"),
        (18., 320., "main"),
        (12., 160., "feature/a-very-long-branch-name"),
        (18., 160., "feature/a-very-long-branch-name"),
    ] {
        view.update(cx, |view, cx| {
            view.config.sidebar.size = font_size;
            view.sidebar_width = Some(width);
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[0].branch =
                Some(branch.into());
            cx.notify();
        });
        cx.update(|window, cx| {
            cx.default_global::<TextProbes>().0.clear();
            full_draw(window, cx).clear(cx);
        });
        let upstream = cx.debug_bounds("upstream-herdr").unwrap();
        let row = cx.debug_bounds("row-herdr").unwrap();
        assert!(upstream.right() <= row.right());
        cx.update(|_, cx| {
            let probes = &cx.global::<TextProbes>().0;
            let (_, rendered_branch, _) = &probes[branch];
            if branch != "main" {
                assert!(rendered_branch.ends_with('…'));
            } else {
                assert_eq!(rendered_branch, branch);
            }
            for text in ["↑4", "↓3"] {
                let (bounds, rendered, _) = &probes[text];
                assert_eq!(rendered, text);
                assert!(bounds.left() >= upstream.left());
                assert!(bounds.right() <= upstream.right() + px(1.));
                if text == "↓3" {
                    // The final count defines the container's width. A fixed
                    // glyph estimate can either cut it off or leave empty room.
                    assert_eq!(bounds.right(), upstream.right());
                }
            }
        });
    }
}
