use super::*;
use crate::{update_panel::download_caption, updater::State};

fn caption(received: u64, total: u64) -> Option<(String, Option<String>)> {
    download_caption(&State::Downloading { received, total })
}

#[test]
fn download_sizes_read_in_megabytes() {
    assert_eq!(
        caption(42_318_544, 96_104_448),
        Some(("42.3 MB of 96.1 MB".into(), Some("44%".into())))
    );
    assert_eq!(
        caption(0, 100_000_000),
        Some(("0.0 MB of 100.0 MB".into(), Some("0%".into())))
    );
    assert_eq!(
        caption(42_318_544, 0),
        Some(("42.3 MB downloaded".into(), None)),
        "an unknown size shows what arrived and no percentage"
    );
}

#[test]
fn verification_and_other_states_show_no_sizes() {
    assert_eq!(caption(100, 100), None);
    assert_eq!(caption(u64::MAX, 100), None);
    assert_eq!(download_caption(&State::Checking), None);
    assert_eq!(download_caption(&State::Installing), None);
}

// `debug_bounds` keeps the last frame that drew an element, so the state that
// draws no caption goes first.
#[gpui::test]
fn the_download_caption_sits_under_the_bar(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(320.), px(480.)));

    draw_update_state(
        cx,
        &view,
        &State::Downloading {
            received: 100,
            total: 100,
        },
    );
    assert!(cx.debug_bounds("app-update-download-size").is_none());

    draw_update_state(
        cx,
        &view,
        &State::Downloading {
            received: 42_000_000,
            total: 0,
        },
    );
    assert!(cx.debug_bounds("app-update-download-size").is_some());
    assert!(
        cx.debug_bounds("app-update-download-percent").is_none(),
        "no percentage without a size"
    );

    let (panel, _) = draw_update_state(
        cx,
        &view,
        &State::Downloading {
            received: 42_000_000,
            total: 96_000_000,
        },
    );
    let bar = cx.debug_bounds("app-update-progress").unwrap();
    let sizes = cx.debug_bounds("app-update-download-size").unwrap();
    let percent = cx.debug_bounds("app-update-download-percent").unwrap();
    assert!(sizes.top() >= bar.bottom(), "the sizes sit under the bar");
    assert!(sizes.left() >= panel.left() && sizes.right() <= panel.right());
    assert!(
        (percent.right() - bar.right()).abs() < px(1.),
        "the percentage lines up with the bar's end"
    );
}
