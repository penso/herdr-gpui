use super::*;
use crate::control::protocol::work::{MuteInfo, Reply};
use crate::notification_mutes::Mutes;
use gpui::TestAppContext;

fn info(response: Response) -> MuteInfo {
    match response {
        Response::Work {
            reply: Reply::Mute { mute },
        } => mute,
        response => panic!("{response:?}"),
    }
}

#[gpui::test]
fn the_callers_worktree_mutes_and_reports_its_hosts_mute(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        connected(&mut view);
        view
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let here = caller(Some("w4"));
            assert!(!info(mute::status(view, &here, cx)).muted);

            assert!(info(mute::set(view, &here, true, cx)).muted);
            assert!(info(mute::set(view, &here, true, cx)).muted, "asked twice");
            assert!(!info(mute::status(view, &caller(Some("w5")), cx)).muted);

            let id = view.endpoints[0].id.clone();
            Mutes::update(cx, |mutes| mutes.set_host(&id, true));
            let mute = info(mute::set(view, &here, false, cx));
            assert!(!mute.muted && mute.host_muted);

            let missing = mute::status(view, &caller(Some("gone")), cx);
            assert_eq!(error_code(&missing), Some(ErrorCode::WorkspaceNotFound));
        });
    });
}
