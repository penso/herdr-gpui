use super::*;

#[gpui::test]
fn raster_and_svg_agents_share_the_collapsed_rail_geometry(cx: &mut TestAppContext) {
    for hosts in [1, 2] {
        let (view, cx) = rail_window(cx, hosts);
        for mode in LayoutMode::ALL {
            let mut svg_bounds = None;
            for identity in ["pi", "omo"] {
                view.update(cx, |view, cx| {
                    view.config.layout.mode = mode;
                    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                    snapshot.agents[0].agent = Some(identity.into());
                    cx.notify();
                });
                cx.update(|window, cx| full_draw(window, cx).clear(cx));

                let icon = placed(cx, "rail-agent-icon-local-p0");
                let row = placed(cx, "highlight-rail-agent-local-p0");
                let dot = placed(cx, "rail-dot-rail-agent-local-p0");
                assert!(contains(row, icon), "{hosts} {mode:?} {identity}");
                assert!(icon.right() <= dot.left(), "{hosts} {mode:?} {identity}");
                if let Some(expected) = svg_bounds {
                    assert_eq!(icon, expected, "{hosts} {mode:?}");
                } else {
                    svg_bounds = Some(icon);
                }
            }
        }
    }
}
