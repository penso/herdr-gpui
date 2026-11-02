//! Folding a device's sessions: they close from however open they had come,
//! and unfolding again before they finish reopens them from there.
use super::*;
use crate::motion::{ENTER, LEAVE};

#[gpui::test]
fn sessions_close_from_where_they_are_and_reopen_from_there(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.unfold_device(0);
        let local = view.endpoints[0].id.clone();
        // Fully open: the fold starts from all of it.
        let now = crate::motion::now();
        view.menu.unfolded_since = now.checked_sub(ENTER * 2);
        view.fold_device(0);
        assert!(view.menu.unfolded_device.is_none());
        let folding = view.menu.folding.as_ref().unwrap();
        assert_eq!(folding.device, local);
        assert!((folding.from - 1.).abs() < 0.01, "{}", folding.from);

        // Unfolding again at once reopens from nearly open, not from nothing.
        view.unfold_device(0);
        assert!(view.menu.folding.is_none());
        assert!(view.menu.unfold_from > 0.9, "{}", view.menu.unfold_from);

        // A fold that has finished leaves nothing to reopen from.
        view.menu.unfolded_since = now.checked_sub(ENTER * 2);
        view.fold_device(0);
        if let Some(folding) = view.menu.folding.as_mut() {
            folding.since = now.checked_sub(LEAVE * 2).unwrap_or(now);
        }
        view.unfold_device(0);
        assert_eq!(view.menu.unfold_from, 0.);
    });
}

#[gpui::test]
fn unfolding_another_device_folds_the_first(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "ssh:build".into(),
            "Build".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "build.invalid".into(),
                session: "default".into(),
            },
            true,
        ));
        let local = view.endpoints[0].id.clone();
        view.unfold_device(0);
        view.unfold_device(1);
        assert_eq!(view.menu.unfolded_device.as_deref(), Some("ssh:build"));
        assert_eq!(
            view.menu.folding.as_ref().map(|f| f.device.as_str()),
            Some(local.as_str())
        );
        assert_eq!(view.menu.unfold_from, 0.);
    });
}

#[gpui::test]
fn right_on_an_open_device_leaves_it_open(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    view.update_in(cx, |view, window, cx| {
        if view.open_menu(window, cx) {
            view.menu.page = Some(Page::Devices);
            view.menu.selected = Some(1);
        }
    });
    cx.simulate_keystrokes("right");
    let since = view.read_with(cx, |view, _| view.menu.unfolded_since);
    assert!(since.is_some(), "right unfolds Local");
    cx.simulate_keystrokes("right");
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.unfolded_since, since, "it does not open again");
        assert!(view.menu.unfolded_device.is_some());
    });
}
