//! The `en-XA` leak test (`crate::i18n::leak_tests`) for fan-out: an
//! interrupted fan-out restored for comparison, with its notice.
use super::left_behind;
use crate::{
    i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa},
    menu::Page,
};

#[test]
fn a_restored_fan_out_shows_only_messages_under_en_xa() {
    under_en_xa("fan-out en-XA leak", |cx, recorder| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fan-outs.json");
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        draw(cx);
        recorder.take();
        let flash = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
                left_behind(&path, &view.endpoints[0].id).install(cx);
                view.poll_fan_out(window, cx);
                view.menu.page = Some(Page::FanOut);
                window.focus(&view.menu.focus, cx);
                view.flash.as_ref().unwrap().0.text.clone()
            })
        });
        assert!(herdr_i18n::pseudo::is_marked(&flash), "{flash}");
        cx.run_until_parked();
        draw(cx);
        // The repository, the host, and the lane's agent and branch are names.
        let data: Vec<String> = view.read_with(cx, |view, _| {
            let fan_out = view.fan_out.as_ref().unwrap();
            let mut data = vec![
                fan_out.origin.repo_label.clone(),
                fan_out.origin.endpoint_label.clone(),
            ];
            for lane in &fan_out.lanes {
                data.push(lane.lane.agent.clone());
                data.push(lane.lane.branch.clone());
            }
            data
        });
        assert_translated("restored fan-out", &uncut(recorder.take()), &data);
    });
}
