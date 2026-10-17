//! Settings follows what the main window does with Azure machines: a machine
//! being created shows its progress in the Azure devices box, and a device the
//! sidebar removes leaves the box without a reload.
#![allow(clippy::unwrap_used)]
use super::super::super::super::{Section, SettingsWindow};
use crate::{
    HerdrWindow,
    cloud::{CloudProvider, Step},
};
use gpui::{Entity, TestAppContext, VisualTestContext, WindowHandle, px, size};

fn open(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<HerdrWindow>,
    Entity<SettingsWindow>,
    &mut VisualTestContext,
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
            view.select_section(Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Azure;
        })
    });
    cx.simulate_resize(size(px(680.), px(3600.)));
    (main, view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn a_machine_being_created_shows_its_progress_in_the_devices_box(cx: &mut TestAppContext) {
    let (main, view, cx) = open(cx);
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let started = cx.update(|_, cx| {
        main.update(cx, |main, _, cx| {
            main.start_cloud_job(
                CloudProvider::Azure,
                "herdr-new-box".into(),
                None,
                move |_, report| {
                    report(Step::InstallingHerdr);
                    let _ = wait.recv_timeout(std::time::Duration::from_secs(10));
                    Err(crate::cloud::Error::Cancelled.into())
                },
                cx,
            )
        })
        .unwrap()
    });
    if crate::cloud::unavailable().is_some() {
        assert!(started.is_err());
        return;
    }
    started.unwrap();
    // The step reaches the window on a later tick, and Settings follows it.
    for _ in 0..50 {
        cx.run_until_parked();
        let stepped = cx.update(|_, cx| {
            main.read(cx)
                .unwrap()
                .cloud_jobs
                .iter()
                .any(|job| job.progress.is_some())
        });
        if stepped {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    draw(cx);
    assert!(cx.debug_bounds("cloud-azure-creating-0").is_some());
    view.read_with(cx, |view, cx| {
        let rows = view.azure_job_rows(cx);
        assert_eq!(rows.len(), 1);
    });
    release.send(()).unwrap();
}

#[gpui::test]
fn a_device_removed_in_the_main_window_is_read_again(cx: &mut TestAppContext) {
    let (main, view, cx) = open(cx);
    let before = view.read_with(cx, |view, _| view.cloud_devices_seen);
    cx.update(|_, cx| {
        main.update(cx, |main, _, cx| {
            main.endpoints.push(crate::endpoint::Endpoint::new(
                "azure:herdr-vm".into(),
                "herdr-vm".into(),
                herdr_client::ConnectTarget::Cloud {
                    provider: CloudProvider::Azure,
                    account: "00000000-0000-0000-0000-000000000000".into(),
                    id: "herdr-vm".into(),
                    machine: "herdr-vm".into(),
                    session: "default".into(),
                },
                true,
            ));
            cx.notify();
        })
        .unwrap()
    });
    cx.run_until_parked();
    let added = view.read_with(cx, |view, _| view.cloud_devices_seen);
    assert_ne!(added, before, "a new cloud device is noticed");
    cx.update(|_, cx| {
        main.update(cx, |main, _, cx| {
            main.endpoints
                .retain(|endpoint| endpoint.id != "azure:herdr-vm");
            cx.notify();
        })
        .unwrap()
    });
    cx.run_until_parked();
    let removed = view.read_with(cx, |view, _| view.cloud_devices_seen);
    assert_ne!(removed, added, "and so is its removal");
}
