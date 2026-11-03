use super::*;
use gpui::AppContext;
use std::sync::mpsc;
use std::time::Duration;

#[gpui::test]
fn another_windows_setup_blocks_removal_and_duplicate_setup_after_retargeting(
    cx: &mut TestAppContext,
) {
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let ((settings, cx), first) = open(cx);
    let second = cx.update(|_, cx| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx))
        })
        .unwrap()
    });
    // Keep the setup in flight until the assertions finish; dropping this
    // sender releases the worker even if an assertion fails.
    let (_finish, wait) = mpsc::channel::<()>();
    cx.update(|_, cx| {
        first
            .update(cx, |main, _, cx| {
                main.start_cloud_job(
                    CloudProvider::Railway,
                    "still-setting-up".into(),
                    None,
                    move |stop, _| {
                        while !stop() {
                            match wait.recv_timeout(Duration::from_millis(20)) {
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                _ => break,
                            }
                        }
                        Err(crate::Error::from(crate::cloud::Error::Cancelled))
                    },
                    cx,
                )
                .unwrap();
            })
            .unwrap();
        let source = second.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        settings.update(cx, |view, cx| {
            // Opening Settings from window two takes this same path.
            view.retarget_source(source, cx);
            // Exercise Remove's guard without ever touching the real catalog.
            view.change_railway(cx, |_, _| panic!("removal must not start"));
            let card = view.railway_card.as_mut().unwrap();
            assert!(!card.changing && card.job.is_none());
            assert!(card.message.as_deref().unwrap().contains("Wait"));
            card.source = KeySource::Existing;
            card.path
                .update(cx, |input, cx| input.set_text_selected("/test/key", cx));
            card.message = None;
            view.setup_railway(cx);
            assert!(
                view.railway_card
                    .as_ref()
                    .unwrap()
                    .message
                    .as_deref()
                    .unwrap()
                    .contains("Wait")
            );
        });
        second
            .update(cx, |main, _, _| assert!(main.cloud_jobs.is_empty()))
            .unwrap();

        // Once the owning window's job is gone, operations become available
        // again. The original worker is cancelled by dropping its handle.
        first
            .update(cx, |main, _, _| main.cloud_jobs = Default::default())
            .unwrap();
        settings.update(cx, |view, cx| {
            view.change_railway(cx, |_, send| send(Update::Done(Ok(()))));
            assert!(view.railway_card.as_ref().unwrap().changing);
        });
    });
}

#[gpui::test]
fn setup_waits_for_a_settings_change_to_finish(cx: &mut TestAppContext) {
    let ((settings, cx), main) = open(cx);
    cx.update(|_, cx| {
        settings.update(cx, |view, cx| {
            let card = view.railway_card.as_mut().unwrap();
            card.changing = true;
            card.source = KeySource::Existing;
            card.path
                .update(cx, |input, cx| input.set_text_selected("/test/key", cx));
            view.setup_railway(cx);
            assert!(
                view.railway_card
                    .as_ref()
                    .unwrap()
                    .message
                    .as_deref()
                    .unwrap()
                    .contains("Wait")
            );
        });
        main.update(cx, |main, _, _| assert!(main.cloud_jobs.is_empty()))
            .unwrap();
    });
}
