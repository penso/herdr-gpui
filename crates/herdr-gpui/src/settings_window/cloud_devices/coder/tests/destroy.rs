use super::*;
use crate::{
    cloud::worker,
    coder::setup::Stage,
    progress::Progress,
    settings_window::cloud_devices::coder::destroy::{self, Destroying, Outcome, Update},
};

fn device(index: usize) -> SavedDevice {
    SavedDevice {
        provider: crate::cloud::CloudProvider::Coder,
        id: format!("w{index}"),
        label: format!("Box {index}"),
        account: "https://coder.example.com".into(),
        machine: format!("herdr-box-{index}"),
        session: "default".into(),
        enabled: true,
    }
}

fn signed_in(view: &Entity<SettingsWindow>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let card = view.coder_card.as_mut().unwrap();
            card.account = Account::Known(Session::SignedIn {
                user: "fixture-user".into(),
            });
            card.devices = (0..2).map(device).collect();
        })
    });
    // The narrowest window, tall enough that both rows stay in view.
    cx.simulate_resize(size(px(680.), px(1600.)));
    draw(cx);
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    draw(cx);
}

#[gpui::test]
fn each_device_offers_destroy_beside_remove_and_asks_before_destroying(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    signed_in(&view, cx);
    let devices = cx.debug_bounds("cloud-coder-devices").unwrap();
    for (destroy, remove) in [
        ("cloud-coder-destroy-0", "cloud-coder-remove-0"),
        ("cloud-coder-destroy-1", "cloud-coder-remove-1"),
    ] {
        let destroy = cx.debug_bounds(destroy).unwrap();
        let remove = cx.debug_bounds(remove).unwrap();
        assert!(
            destroy.right() <= remove.left(),
            "Destroy sits before Remove"
        );
        assert!(destroy.left() >= devices.left() && remove.right() <= devices.right());
    }

    click(cx, "cloud-coder-destroy-1");
    assert!(cx.debug_bounds("cloud-coder-destroy-confirm-1").is_some());
    assert!(cx.debug_bounds("cloud-coder-destroy-cancel-1").is_some());
    assert!(cx.debug_bounds("cloud-coder-destroy-confirm-0").is_none());
    assert!(
        cx.debug_bounds("cloud-coder-destroying").is_none(),
        "nothing runs before confirming"
    );
    view.read_with(cx, |view, _| {
        let card = view.coder_card.as_ref().unwrap();
        assert_eq!(card.confirm_destroy.as_deref(), Some("w1"));
        assert!(card.destroying.is_none());
    });

    click(cx, "cloud-coder-destroy-cancel-1");
    assert!(cx.debug_bounds("cloud-coder-destroy-1").is_some());
    view.read_with(cx, |view, _| {
        assert_eq!(view.coder_card.as_ref().unwrap().confirm_destroy, None);
    });
}

#[gpui::test]
fn destroying_needs_a_sign_in(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    signed_in(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.coder_card.as_mut().unwrap().account = Account::Known(Session::SignedOut);
        })
    });
    draw(cx);
    click(cx, "cloud-coder-destroy-0");
    view.read_with(cx, |view, _| {
        let card = view.coder_card.as_ref().unwrap();
        assert!(!card.can_destroy());
        assert_eq!(card.confirm_destroy, None, "a disabled button does nothing");
    });
}

#[gpui::test]
fn a_running_destroy_shows_its_progress_and_reports_when_the_workspace_is_gone(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    signed_in(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let job = worker::spawn("test", cx, |_, _: &dyn Fn(Update)| {}, |_, _, _| {}).unwrap();
            let card = view.coder_card.as_mut().unwrap();
            card.destroying = Some(Destroying {
                id: "w0".into(),
                label: "Box 0".into(),
                stage: None,
                _job: job,
            });
        })
    });
    draw(cx);
    let row = cx.debug_bounds("cloud-coder-remove-0").unwrap();
    let progress = cx.debug_bounds("cloud-coder-destroy-progress").unwrap();
    assert!(progress.top() >= row.bottom(), "the bar sits under its row");
    assert!(progress.right() <= px(680.));
    view.read_with(cx, |view, _| {
        let card = view.coder_card.as_ref().unwrap();
        assert!(!card.can_destroy(), "one destroy at a time");
        assert!(!view.coder_removable(&card.devices[0]));
        assert!(view.coder_removable(&card.devices[1]));
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_destroy(Update::Stage(Stage::Deleting), cx)
        })
    });
    view.read_with(cx, |view, _| {
        let job = view
            .coder_card
            .as_ref()
            .unwrap()
            .destroying
            .as_ref()
            .unwrap();
        assert_eq!(job.stage, Some(Stage::Deleting));
    });

    cx.update(|_, cx| view.update(cx, |view, cx| view.apply_destroy(Update::Done(Ok(())), cx)));
    draw(cx);
    assert!(cx.debug_bounds("cloud-coder-destroy-progress").is_none());
    assert!(cx.debug_bounds("cloud-coder-destroyed").is_some());
    view.read_with(cx, |view, _| {
        let card = view.coder_card.as_ref().unwrap();
        assert!(card.destroying.is_none());
        assert_eq!(card.outcome, Some(Outcome::Destroyed("Box 0".into())));
    });
}

#[gpui::test]
fn a_failed_destroy_keeps_the_device_and_says_why(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    signed_in(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let job = worker::spawn("test", cx, |_, _: &dyn Fn(Update)| {}, |_, _, _| {}).unwrap();
            view.coder_card.as_mut().unwrap().destroying = Some(Destroying {
                id: "w1".into(),
                label: "Box 1".into(),
                stage: Some(Stage::Queued),
                _job: job,
            });
            view.apply_destroy(Update::Done(Err(crate::coder::Error::Forbidden)), cx);
        })
    });
    // The refresh that follows reads the device back: it is still saved.
    signed_in(&view, cx);
    assert!(cx.debug_bounds("cloud-coder-destroy-failed").is_some());
    view.read_with(cx, |view, _| {
        let card = view.coder_card.as_ref().unwrap();
        assert!(
            matches!(&card.outcome, Some(Outcome::Failed(text)) if text.starts_with("Could not destroy \u{2068}Box 1\u{2069}"))
        );
        assert!(card.can_destroy(), "Destroy can run again");
    });
}

#[test]
fn each_stage_moves_the_bar_forward() {
    let fractions: Vec<f32> = [Stage::Waiting, Stage::Queued, Stage::Deleting]
        .into_iter()
        .map(|stage| match destroy::status(Some(stage)).1 {
            Progress::Working(fraction) => fraction,
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(fractions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(destroy::status(None).1, Progress::Busy);
}
