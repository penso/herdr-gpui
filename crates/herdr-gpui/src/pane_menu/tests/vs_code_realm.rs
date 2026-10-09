//! Herdr's menus keep to the Herdr realm beside the VS Code column: the
//! VS Code page draws above GPUI, so it would hide what reaches under it.
use super::*;
use crate::{controls::Command, menu::Cover};
use gpui::{TestAppContext, VisualTestContext, point, px, size};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn menus_and_dialogs_stay_left_of_the_vs_code_column(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.simulate_resize(size(px(1400.), px(800.)));
    draw(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| v.command(Command::ToggleCode, window, cx)));
    draw(cx);
    let code = cx.debug_bounds("code").unwrap();
    let realm = view.read_with(cx, |v, _| v.herdr_realm()).unwrap();
    assert!((realm - code.left()).abs() <= px(1.), "{realm:?} {code:?}");

    // A context menu opened by the column's edge hangs back into the realm.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.open_pane_menu(
                "inactive",
                point(code.left() - px(10.), px(200.)),
                window,
                cx,
            )
        })
    });
    draw(cx);
    let panel = cx.debug_bounds("menu-panel").unwrap();
    assert!(panel.right() <= code.left(), "{panel:?} {code:?}");

    // Its Close asks in a dialog centred in the realm, and the realm alone
    // is dimmed and covered, so the VS Code page keeps showing.
    cx.update(|window, cx| {
        view.update(cx, |v, cx| v.activate_pane_menu(Action::Close, window, cx))
    });
    draw(cx);
    assert_eq!(
        view.read_with(cx, |v, _| v.menu.page),
        Some(Page::ConfirmClose)
    );
    let overlay = cx.debug_bounds("menu-overlay").unwrap();
    assert!(
        (overlay.right() - code.left()).abs() <= px(1.),
        "{overlay:?}"
    );
    let dialog = cx.debug_bounds("menu-panel").unwrap();
    assert!(dialog.right() <= code.left());
    assert!((dialog.center().x - overlay.center().x).abs() <= px(1.));
    view.read_with(cx, |v, _| {
        assert!(v.menu_dims());
        let cover = Cover::dimmed(v.herdr_realm());
        assert_eq!(v.menu.cover.get(), cover);
        assert!(!cover.covers(Some(code)));
        assert!(cover.covers(Some(Bounds::new(
            point(px(0.), px(0.)),
            size(px(100.), px(100.))
        ))));
    });

    // Without the column, a dialog dims the whole window again.
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| view.update(cx, |v, cx| v.command(Command::ToggleCode, window, cx)));
    cx.update(|window, cx| view.update(cx, |v, cx| v.command(Command::Palette, window, cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.menu.cover.get()), Cover::All);
}

#[test]
fn a_cover_measured_for_another_page_is_stale() {
    let cover = Cover::Panel(Bounds::new(point(px(0.), px(0.)), size(px(10.), px(10.))));
    assert_eq!(cover.settled(Some(Page::Pane), Some(Page::Pane)), cover);
    assert_eq!(
        cover.settled(Some(Page::Pane), Some(Page::ConfirmClose)),
        Cover::Unknown
    );
    assert_eq!(cover.settled(None, Some(Page::Pane)), Cover::Unknown);
}
