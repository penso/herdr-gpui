use super::*;
use gpui::{Bounds, Pixels, px, size};

fn bounds(cx: &mut VisualTestContext, selector: String) -> Bounds<Pixels> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
        .unwrap()
}

fn assert_load_fits(cx: &mut VisualTestContext, prefix: &str, gpu: bool) {
    let tab = bounds(cx, format!("{prefix}devices-tab"));
    let table = bounds(cx, format!("{prefix}devices-table"));
    for part in ["header", "local"] {
        let load = bounds(cx, format!("{prefix}devices-load-{part}"));
        assert!(
            load.left() >= table.left() && load.right() <= table.right(),
            "{load:?} inside {table:?}"
        );
        assert!(load.right() <= tab.right(), "{load:?} inside {tab:?}");
        // Every column but the device's and the agents'.
        let columns = crate::devices_overview::table::load_columns(gpu);
        for column in 0..columns {
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
        // The GPU's columns are drawn only while it is shown.
        let past = format!("{prefix}devices-load-{part}-{columns}");
        assert!(cx.debug_bounds(Box::leak(past.into_boxed_str())).is_none());
        let first = bounds(cx, format!("{prefix}devices-load-{part}-0"));
        let last = bounds(cx, format!("{prefix}devices-load-{part}-{}", columns - 1));
        assert!(last.top() > first.top(), "columns wrap within the group");
    }
}

#[gpui::test]
fn load_columns_fit_the_minimum_window_width(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(640.), px(900.)));
    open(&view, cx);
    assert_load_fits(cx, "", false);
}

#[gpui::test]
fn gpu_columns_fit_the_minimum_window_width(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| view.config.show_gpu_load = true);
    cx.simulate_resize(size(px(640.), px(900.)));
    open(&view, cx);
    assert_load_fits(cx, "", true);
}

#[gpui::test]
fn load_columns_fit_a_narrow_editor_group(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1000.), px(900.)));
    open(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx);
            // The new group opens without tabs; the overview moves to it.
            let slots = view.group_slots();
            let Some(crate::browser::Pick::Page(overview)) = view.group_pick(slots[0].id) else {
                panic!("the overview is shown")
            };
            view.show_browser_tab_in(Some(slots[1].id), overview, window, cx);
        })
    });
    draw(cx);
    assert_load_fits(cx, "g1-", false);
}
