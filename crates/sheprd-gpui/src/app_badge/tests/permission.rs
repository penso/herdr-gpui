use super::*;
use crate::actions::SetBadgePreview;

fn set_preview(enabled: bool, cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.dispatch_action(&SetBadgePreview { enabled }));
    cx.run_until_parked();
}

#[gpui::test]
fn badge_permission_is_requested_once_on_the_first_nonzero_count(cx: &mut gpui::TestAppContext) {
    PERMISSION_REQUESTS.set(0);
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            install(cx);
            sync(window.window_handle().window_id(), &view.endpoints, cx);
        })
        .unwrap();
    cx.read_global::<Badge, _>(|badge, _| assert_eq!(badge.published, Some(0)));
    assert_eq!(PERMISSION_REQUESTS.get(), 0, "an empty badge never prompts");

    set_preview(true, cx);
    cx.read_global::<Badge, _>(|badge, _| assert_eq!(badge.published, Some(2)));
    assert_eq!(PERMISSION_REQUESTS.get(), 1);

    set_preview(false, cx);
    set_preview(true, cx);
    cx.read_global::<Badge, _>(|badge, _| assert_eq!(badge.published, Some(2)));
    assert_eq!(
        PERMISSION_REQUESTS.get(),
        1,
        "a badge shown again asks only once"
    );
}
