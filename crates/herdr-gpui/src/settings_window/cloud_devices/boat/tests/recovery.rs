use super::*;
use crate::{
    boat::setup::{Key, Overview},
    cloud::SavedDevice,
    settings_window::cloud_devices::boat::Account,
};

#[gpui::test]
fn saved_boat_devices_offer_finish_setup_after_a_failed_account_poll(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.simulate_resize(size(px(800.), px(1000.)));
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.boat.api_url = Some(crate::boat::DEFAULT_API_URL.into());
            let card = view.boat_card.as_mut().unwrap();
            card.account = Account::Known(Box::new(Overview {
                pending: Ok(None),
                key: Ok(Key::Saved),
                sandboxes: Some(Err(crate::boat::Error::StartTimeout)),
                devices: vec![SavedDevice {
                    provider: CloudProvider::Boat,
                    id: "bx_23456789".into(),
                    label: "Saved interrupted setup".into(),
                    account: crate::boat::DEFAULT_API_URL.into(),
                    machine: "herdr-box".into(),
                    session: "default".into(),
                    enabled: true,
                }],
            }));
            cx.notify();
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("cloud-boat-finish-0").is_some());
    assert!(cx.debug_bounds("cloud-boat-remove-0").is_some());
    view.read_with(cx, |view, cx| {
        let source = view.source.upgrade().unwrap();
        assert!(
            source.read(cx).cloud_jobs.is_empty(),
            "rendering must not launch recovery"
        );
    });
}

#[gpui::test]
fn setup_in_another_main_window_blocks_overlapping_boat_jobs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let other = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let (release, wait) = std::sync::mpsc::channel();
    let result = cx.update(|_, cx| {
        other
            .update(cx, |main, _, cx| {
                main.start_cloud_job(
                    CloudProvider::Boat,
                    "fixture sandbox".into(),
                    None,
                    move |_, _| {
                        let _ = wait.recv_timeout(std::time::Duration::from_secs(10));
                        Err(crate::Error::from(crate::cloud::Error::Cancelled))
                    },
                    cx,
                )
            })
            .unwrap()
    });
    view.read_with(cx, |_, cx| {
        if crate::cloud::unavailable().is_some() {
            assert!(matches!(
                result,
                Err(crate::Error::Remote(herdr_remote::Error::Cloud(
                    crate::cloud::Error::Unavailable(_)
                )))
            ));
            assert!(!super::super::setup_running(cx));
        } else {
            result.unwrap();
            assert!(super::super::setup_running(cx));
        }
    });
    let _ = release.send(());
}
