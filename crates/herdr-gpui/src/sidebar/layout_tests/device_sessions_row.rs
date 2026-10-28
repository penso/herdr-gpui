use super::*;

fn open_picker(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let picker = cx.debug_bounds("device-picker").unwrap().center();
    cx.simulate_click(picker, Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

fn page(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> Option<crate::menu::Page> {
    view.read_with(cx, |view, _| view.menu.page)
}

// The fixture has one device: All Devices, Local, Add Device, Sessions.
const SESSIONS_ROW: &str = "device-row-3";

#[gpui::test]
fn the_device_picker_opens_the_session_list_where_it_was(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    let picker = cx.debug_bounds("menu-panel").unwrap();
    let row = cx.debug_bounds(SESSIONS_ROW).unwrap();
    cx.simulate_click(row.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(page(&view, cx), Some(crate::menu::Page::Sessions));
    let list = cx.debug_bounds("menu-panel").unwrap();
    assert_eq!(list.left(), picker.left());
    assert_eq!(list.bottom(), picker.bottom());
}

#[gpui::test]
fn up_from_all_devices_wraps_to_sessions(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    // Up from All Devices wraps to the last row, Sessions.
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), Some(crate::menu::Page::Sessions));
}

#[gpui::test]
fn four_rows_down_wraps_back_to_all_devices(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    // All Devices closes the picker, so wrapping past Sessions lands there.
    cx.simulate_keystrokes("down down down down enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), None);
}
