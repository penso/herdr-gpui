#![allow(clippy::unwrap_used)]
use super::{
    super::super::{Section, SettingsWindow},
    CloudProvider, KeySource, Overview, Update, setup,
};
use crate::{cloud::SavedDevice, railway::KeyDirectory};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
use std::path::PathBuf;

mod cross_window_jobs;
mod messages;

fn open(
    cx: &mut TestAppContext,
) -> (
    (Entity<SettingsWindow>, &mut VisualTestContext),
    gpui::WindowHandle<crate::HerdrWindow>,
) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.cloud_tab = CloudProvider::Railway;
            view.select_section(Section::CloudDevices, window, cx);
        })
    });
    ((view, cx), main)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn device(index: usize) -> setup::Device {
    setup::Device {
        saved: SavedDevice {
            provider: CloudProvider::Railway,
            id: format!("key-{index}"),
            label: format!("A long Railway device label number {index} that must truncate"),
            account: format!("/home/u/.ssh/key-{index}"),
            machine: format!("railway-box-{index}"),
            session: "default".into(),
            enabled: true,
        },
        key_present: true,
    }
}

fn show(view: &Entity<SettingsWindow>, devices: Vec<setup::Device>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.railway_card.as_mut().unwrap().overview = Some(Overview {
                devices,
                new_key_directory: KeyDirectory {
                    path: PathBuf::from("/home/u/.ssh"),
                    user: true,
                },
            });
        })
    });
}

#[gpui::test]
fn the_key_choices_install_consent_and_devices_fit_the_narrowest_width(cx: &mut TestAppContext) {
    let ((view, cx), _main) = open(cx);
    show(&view, vec![device(0), device(1)], cx);
    cx.simulate_resize(size(px(680.), px(2000.)));
    draw(cx);
    for selector in [
        "cloud-railway-card",
        "cloud-railway-generate",
        "cloud-railway-source-existing",
        "cloud-railway-install",
        "cloud-railway-create",
        "cloud-railway-devices",
        "cloud-railway-remove-0",
        "cloud-railway-remove-1",
    ] {
        let bounds = cx.debug_bounds(selector);
        assert!(bounds.is_some_and(|b| b.right() <= px(680.)), "{selector}");
    }
    assert!(cx.debug_bounds("cloud-railway-browse").is_none());
    let existing = cx.debug_bounds("cloud-railway-source-existing").unwrap();
    cx.simulate_click(existing.center(), gpui::Modifiers::none());
    draw(cx);
    view.read_with(cx, |view, _| {
        let card = view.railway_card.as_ref().unwrap();
        assert_eq!(card.source, KeySource::Existing);
        assert!(
            card.install,
            "installing needs the user's switch, on by default"
        );
    });
    assert!(cx.debug_bounds("cloud-railway-browse").is_some());
    assert!(cx.debug_bounds("cloud-railway-generate").is_none());
}

#[gpui::test]
fn setup_without_a_usable_key_starts_nothing(cx: &mut TestAppContext) {
    let ((view, cx), main) = open(cx);
    show(&view, Vec::new(), cx);
    let message = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.railway_card.as_ref().unwrap().message.clone().unwrap()
        })
    };
    cx.update(|_, cx| view.update(cx, |view, cx| view.setup_railway(cx)));
    assert!(message(cx).contains("Generate a key"));
    // A path left in the hidden field is not used while "Generate" is chosen.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let card = view.railway_card.as_mut().unwrap();
            card.path
                .update(cx, |input, cx| input.set_text_selected("/old/key", cx));
            card.message = None;
            view.setup_railway(cx);
        })
    });
    assert!(message(cx).contains("Generate a key"));
    // A name that is not one safe file name is explained, not generated.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let card = view.railway_card.as_mut().unwrap();
            card.name
                .update(cx, |input, cx| input.set_text_selected("../escape", cx));
            view.generate_railway_key(cx);
        })
    });
    assert_eq!(
        message(cx),
        crate::i18n::fl!("settings-cloud-railway-name-rule")
    );
    view.read_with(cx, |view, _| {
        assert!(view.railway_card.as_ref().unwrap().job.is_none());
    });
    cx.update(|_, cx| {
        main.update(cx, |main, _, _| assert!(main.cloud_jobs.is_empty()))
            .unwrap()
    });
}

#[gpui::test]
fn a_generated_key_fills_the_path_and_a_refresh_keeps_a_change(cx: &mut TestAppContext) {
    let ((view, cx), _main) = open(cx);
    show(&view, vec![device(0)], cx);
    // A generated key becomes the key to set up with.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let key = PathBuf::from("/home/u/.ssh/herdr-railway");
            view.apply_railway(Update::Generated(Ok(key)), cx);
            let card = view.railway_card.as_ref().unwrap();
            assert_eq!(card.source, KeySource::Existing);
            assert_eq!(card.path.read(cx).text(), "/home/u/.ssh/herdr-railway");
        })
    });
    // A change in progress is not replaced, which would cancel it, by a
    // config-driven refresh.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let job = crate::cloud::worker::spawn::<SettingsWindow, ()>(
                "railway-test-change",
                cx,
                |stop, _| {
                    while !stop() {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                },
                |_, (), _| {},
            )
            .unwrap();
            let card = view.railway_card.as_mut().unwrap();
            (card.job, card.changing, card.reads_disk) = (Some(job), true, true);
            view.railway_config_changed(cx);
            let card = view.railway_card.as_ref().unwrap();
            assert!(card.job.is_some() && card.changing);
        })
    });
}

// Cloud jobs are unavailable on Windows, so there is no setup to wait for.
#[cfg(unix)]
#[gpui::test]
fn removal_waits_for_a_running_setup(cx: &mut TestAppContext) {
    let ((view, cx), main) = open(cx);
    show(&view, vec![device(0)], cx);
    // With a setup running, a removal is refused and starts nothing.
    cx.update(|_, cx| {
        let started = main
            .update(cx, |main, _, cx| {
                main.start_cloud_job(
                    CloudProvider::Railway,
                    "box".into(),
                    None,
                    |stop, _| {
                        while !stop() {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                        Err(crate::Error::from(crate::cloud::Error::Cancelled))
                    },
                    cx,
                )
            })
            .unwrap();
        started.unwrap();
        view.update(cx, |view, cx| {
            view.forget_railway_device("key-0".into(), cx)
        });
    });
    view.read_with(cx, |view, _| {
        let card = view.railway_card.as_ref().unwrap();
        assert!(card.job.is_none() && !card.changing);
        assert!(card.message.as_deref().unwrap().contains("Wait"));
    });
}
