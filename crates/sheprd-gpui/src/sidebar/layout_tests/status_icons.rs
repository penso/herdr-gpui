use super::*;

/// Every layout draws the chosen icon set, with or without the edge rail: the
/// rail's bar sits on each working row's leading edge only while it is on,
/// and Herdr's own rows center a drawn icon on their text column instead of
/// its first line.
#[gpui::test]
fn status_icons_fit_every_layout(cx: &mut gpui::TestAppContext) {
    use crate::config::{LayoutMode, StatusIcons};
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(900.)));
    // Devices draws Herdr's rows under device headers, which this fixture has none of.
    let modes = LayoutMode::ALL
        .into_iter()
        .filter(|mode| *mode != LayoutMode::Devices);
    for (mode, icons, font) in modes.flat_map(|mode| {
        StatusIcons::ALL
            .iter()
            .copied()
            .flat_map(move |icons| [8., 13., 20.].map(|font| (mode, icons, font)))
    }) {
        // The rail is a switch of its own; try it on with every other set.
        let rail = StatusIcons::ALL
            .iter()
            .position(|&set| set == icons)
            .unwrap_or(0)
            % 2
            == 0;
        view.update(cx, |view, cx| {
            view.config.layout.mode = mode;
            view.config.status_icons = icons;
            view.config.edge_rail = rail;
            view.config.sidebar.size = font;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let case = format!("{mode:?} {icons:?} rail {rail} at {font}");
        let sidebar = cx.debug_bounds("sidebar").unwrap();
        for key in ["herdr", "agent-p0"] {
            let row = cx
                .debug_bounds(Box::leak(format!("row-{key}").into_boxed_str()))
                .unwrap_or_else(|| panic!("{case}: no row-{key}"));
            assert!(row.right() <= sidebar.right(), "{case}: row-{key} {row:?}");
            match (
                rail,
                cx.debug_bounds(Box::leak(format!("rail-{key}").into_boxed_str())),
            ) {
                (true, Some(rail)) => assert!(
                    // Orca's cards are bordered, and the rail sits inside it.
                    rail.size.height >= px(4.)
                        && rail.left() >= row.left()
                        && rail.left() - row.left() <= px(1.)
                        && rail.top() >= row.top()
                        && rail.bottom() <= row.bottom(),
                    "{case}: rail-{key} {rail:?} is not on row {row:?}"
                ),
                (true, None) => panic!("{case}: working row-{key} has no rail"),
                (_, Some(rail)) => panic!("{case}: unexpected rail-{key} {rail:?}"),
                (_, None) => {}
            }
        }
        // Dots keep upstream's place on the first line; drawn icons center.
        if matches!(mode, LayoutMode::Classic { .. }) && icons != StatusIcons::Dots {
            let row = cx.debug_bounds("row-herdr").unwrap();
            let mark = cx.debug_bounds("mark-herdr").unwrap();
            let column = cx.debug_bounds("column-herdr").unwrap();
            assert!(
                mark.right() <= column.left(),
                "{case}: {mark:?} overlaps {column:?}"
            );
            assert!(
                (mark.center().y - column.center().y).abs() <= px(1.),
                "{case}: {mark:?} is not centered on {column:?}"
            );
            assert!(
                mark.top() >= row.top() && mark.bottom() <= row.bottom(),
                "{case}: {mark:?} leaves {row:?}"
            );
        }
    }
}
