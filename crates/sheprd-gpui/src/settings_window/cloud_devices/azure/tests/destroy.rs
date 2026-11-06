#![allow(clippy::unwrap_used)]
use super::super::{
    super::super::{Section, SettingsWindow},
    Account, AzureConfig,
    destroy::{Destroying, Outcome, Update},
};
use crate::{
    azure::{
        Settings,
        destroy::Stage,
        machine::Machine,
        setup::{CredentialStatus, Overview},
    },
    cloud::{CloudProvider, SavedDevice, worker},
    progress::Progress,
};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
use std::collections::HashMap;

const SUBSCRIPTION: &str = "11111111-1111-1111-1111-111111111111";

fn device(id: String) -> SavedDevice {
    SavedDevice {
        provider: CloudProvider::Azure,
        label: format!("Box {id}"),
        machine: id.clone(),
        id,
        account: SUBSCRIPTION.into(),
        session: "default".into(),
        enabled: true,
    }
}

/// Settings open on an Azure account holding a VM (row 0) and a sandbox
/// (row 1), in the narrowest window, tall enough that both rows show.
fn open(cx: &mut TestAppContext) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    let config = AzureConfig {
        subscription_id: Some(SUBSCRIPTION.into()),
        ..AzureConfig::default()
    };
    let settings = Settings::resolve(&config, |_| None).unwrap().unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.azure = AzureConfig::default();
            view.select_section(Section::CloudDevices, window, cx);
            view.cloud_tab = CloudProvider::Azure;
            let card = view.azure_card.as_mut().unwrap();
            // The fixture stands in for the account the worker would read.
            card.job = None;
            card.account = Account::Known {
                settings: Box::new(settings),
                overview: Overview {
                    credential: Ok(CredentialStatus::AzureCli),
                    devices: vec![
                        device("herdr-box".into()),
                        device(Machine::sandbox_id("sb-1")),
                    ],
                },
                catalog: None,
                sizes: HashMap::new(),
            };
        })
    });
    cx.simulate_resize(size(px(680.), px(3600.)));
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    draw(cx);
}

#[gpui::test]
fn removing_a_device_asks_with_delete_as_the_default(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let devices = cx.debug_bounds("cloud-azure-devices").unwrap();
    for remove in ["cloud-azure-destroy-0", "cloud-azure-destroy-1"] {
        let remove = cx.debug_bounds(remove).unwrap();
        assert!(remove.left() >= devices.left() && remove.right() <= devices.right());
    }
    // One Remove… per row; removing from Herdr only waits for the question.
    assert!(cx.debug_bounds("cloud-azure-remove-0").is_none());

    click(cx, "cloud-azure-destroy-0");
    let only = cx.debug_bounds("cloud-azure-remove-0").unwrap();
    let cancel = cx.debug_bounds("cloud-azure-destroy-cancel-0").unwrap();
    let delete = cx.debug_bounds("cloud-azure-destroy-confirm-0").unwrap();
    assert!(
        only.right() <= cancel.left() && cancel.right() <= delete.left(),
        "Delete is the last, default answer"
    );
    assert!(delete.right() <= devices.right());
    assert!(cx.debug_bounds("cloud-azure-destroy-confirm-1").is_none());
    assert!(
        cx.debug_bounds("cloud-azure-destroying").is_none(),
        "nothing runs before confirming"
    );
    view.read_with(cx, |view, _| {
        let card = view.azure_card.as_ref().unwrap();
        assert_eq!(card.confirm_destroy.as_deref(), Some("herdr-box"));
        assert!(card.destroying.is_none());
    });

    click(cx, "cloud-azure-destroy-cancel-0");
    assert!(cx.debug_bounds("cloud-azure-destroy-0").is_some());
    assert!(cx.debug_bounds("cloud-azure-remove-0").is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.azure_card.as_ref().unwrap().confirm_destroy, None);
    });
}

#[gpui::test]
fn destroying_needs_a_resolved_account(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.azure_card.as_mut().unwrap().account = Account::Checking;
        })
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(!view.azure_card.as_ref().unwrap().can_destroy());
    });
}

#[gpui::test]
fn a_running_destroy_shows_its_progress_and_reports_when_the_machine_is_gone(
    cx: &mut TestAppContext,
) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let job = worker::spawn("test", cx, |_, _: &dyn Fn(Update)| {}, |_, _, _| {}).unwrap();
            view.azure_card.as_mut().unwrap().destroying = Some(Destroying {
                id: "herdr-box".into(),
                label: "Box herdr-box".into(),
                stage: None,
                _job: job,
            });
        })
    });
    draw(cx);
    let row = cx.debug_bounds("cloud-azure-destroy-0").unwrap();
    let progress = cx.debug_bounds("cloud-azure-destroy-progress").unwrap();
    assert!(progress.top() >= row.bottom(), "the bar sits under its row");
    assert!(progress.right() <= px(680.));
    view.read_with(cx, |view, _| {
        let card = view.azure_card.as_ref().unwrap();
        assert!(!card.can_destroy(), "one destroy at a time");
        assert!(!view.azure_removable(&card.devices()[0]));
        assert!(view.azure_removable(&card.devices()[1]));
    });

    for stage in [Stage::Vm, Stage::Leftovers] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.apply_azure_destroy(Update::Stage(stage), cx)
            })
        });
        view.read_with(cx, |view, _| {
            let job = view
                .azure_card
                .as_ref()
                .unwrap()
                .destroying
                .as_ref()
                .unwrap();
            assert_eq!(job.stage, Some(stage));
            assert!(matches!(
                super::super::destroy::status(job.stage).1,
                Progress::Working(_)
            ));
        });
    }

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.apply_azure_destroy(Update::Done(Ok(())), cx)
        })
    });
    view.read_with(cx, |view, _| {
        let card = view.azure_card.as_ref().unwrap();
        assert!(card.destroying.is_none());
        assert_eq!(
            card.outcome,
            Some(Outcome::Destroyed("Box herdr-box".into()))
        );
    });
}

#[gpui::test]
fn a_failed_destroy_keeps_the_device_and_says_why(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let job = worker::spawn("test", cx, |_, _: &dyn Fn(Update)| {}, |_, _, _| {}).unwrap();
            view.azure_card.as_mut().unwrap().destroying = Some(Destroying {
                id: Machine::sandbox_id("sb-1"),
                label: "Box sb-1".into(),
                stage: Some(Stage::Sandbox),
                _job: job,
            });
            view.apply_azure_destroy(Update::Done(Err(crate::azure::Error::DeleteTimeout)), cx);
        })
    });
    view.read_with(cx, |view, _| {
        let card = view.azure_card.as_ref().unwrap();
        let Some(Outcome::Failed(message)) = &card.outcome else {
            panic!("the failure is reported");
        };
        assert!(message.starts_with("Could not delete \u{2068}Box sb-1\u{2069}: "));
        assert!(card.can_destroy(), "Destroy can run again");
    });
}
