use super::*;
use gpui::{Bounds, Pixels, px, size};

fn bounds(cx: &mut VisualTestContext, selector: String) -> Bounds<Pixels> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
        .unwrap()
}

fn assert_load_fits(cx: &mut VisualTestContext, prefix: &str) {
    let tab = bounds(cx, format!("{prefix}devices-tab"));
    let table = bounds(cx, format!("{prefix}devices-table"));
    for part in ["header", "local"] {
        let load = bounds(cx, format!("{prefix}devices-load-{part}"));
        assert!(
            load.left() >= table.left() && load.right() <= table.right(),
            "{load:?} inside {table:?}"
        );
        assert!(load.right() <= tab.right(), "{load:?} inside {tab:?}");
        for column in 0..5 {
            let cell = bounds(cx, format!("{prefix}devices-load-{part}-{column}"));
            assert!(
                cell.left() >= load.left() && cell.right() <= load.right(),
                "{cell:?} inside {load:?}"
            );
            assert!(
                cell.top() >= load.top() && cell.bottom() <= load.bottom(),
                "{cell:?} inside {load:?}"
            );
            let header = bounds(cx, format!("{prefix}devices-load-header-{column}"));
            assert!(
                (header.left() - cell.left()).abs() < px(1.),
                "headers align with values"
            );
        }
        let first = bounds(cx, format!("{prefix}devices-load-{part}-0"));
        let last = bounds(cx, format!("{prefix}devices-load-{part}-4"));
        assert!(last.top() > first.top(), "columns wrap within the group");
    }
}

#[gpui::test]
fn load_columns_fit_the_minimum_window_width(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(640.), px(900.)));
    open(&view, cx);
    assert_load_fits(cx, "");
}

#[gpui::test]
fn load_columns_fit_a_narrow_editor_group(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1000.), px(900.)));
    open(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx);
        })
    });
    draw(cx);
    assert_load_fits(cx, "g1-");
}
