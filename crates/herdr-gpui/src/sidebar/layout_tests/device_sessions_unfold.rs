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

// The fixture has one device: All Devices, Local, Add Device.
const LOCAL_ROW: &str = "device-row-1";
const ADD_ROW: &str = "device-row-2";

fn unfolded(view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.menu.unfolded_device.is_some())
}

#[gpui::test]
fn the_session_chip_unfolds_sessions_under_their_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    let picker = cx.debug_bounds("menu-panel").unwrap();
    assert!(cx.debug_bounds("sessions-row-0").is_none());
    let chip = cx.debug_bounds("device-sessions-1").unwrap();
    let row = cx.debug_bounds(LOCAL_ROW).unwrap();
    assert!(
        row.contains(&chip.center()),
        "the chip sits on its device's row"
    );
    cx.simulate_click(chip.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));

    // The picker stays open, anchored where it was, with the device's
    // sessions between it and Add Device.
    assert_eq!(page(&view, cx), Some(crate::menu::Page::Devices));
    assert!(unfolded(&view, cx));
    let list = cx.debug_bounds("menu-panel").unwrap();
    assert_eq!(list.left(), picker.left());
    assert_eq!(list.bottom(), picker.bottom());
    let session = cx.debug_bounds("sessions-row-0").unwrap();
    let local = cx.debug_bounds(LOCAL_ROW).unwrap();
    let add = cx.debug_bounds(ADD_ROW).unwrap();
    assert!(session.top() >= local.bottom() && session.bottom() <= add.top());
    assert!(
        session.left() > local.left(),
        "sessions sit in from their device"
    );

    // The chip folds them away again, and the picker stays open.
    let chip = cx.debug_bounds("device-sessions-1").unwrap();
    cx.simulate_click(chip.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(page(&view, cx), Some(crate::menu::Page::Devices));
    assert!(!unfolded(&view, cx));
    assert!(cx.debug_bounds("sessions-row-0").is_none());
}

#[gpui::test]
fn right_unfolds_the_selected_device_and_left_folds_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    cx.simulate_keystrokes("down right");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(unfolded(&view, cx));
    assert!(cx.debug_bounds("sessions-row-0").is_some());
    // Down walks into the sessions; left from one folds back to its device.
    cx.simulate_keystrokes("down left");
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(!unfolded(&view, cx));
    // The device row is selected again: Enter lists it alone.
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), None);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.device_filter,
            crate::menu::DeviceScope::only(view.endpoints[0].id.clone())
        );
    });
}

#[gpui::test]
fn up_from_all_devices_wraps_to_add_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    // Add Device, which this development fixture cannot open, so the picker
    // stays as it was rather than choosing a device.
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), Some(crate::menu::Page::Devices));
    assert!(!unfolded(&view, cx));
    // Down from there wraps back to All Devices, which closes the picker.
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn three_rows_down_wraps_back_to_all_devices(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open_picker(cx);
    // All Devices closes the picker, so wrapping past Add Device lands there.
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    assert_eq!(page(&view, cx), None);
}
