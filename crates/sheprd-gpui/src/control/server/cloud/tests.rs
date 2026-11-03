#![cfg(feature = "cloud")]
#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    cloud::CloudProvider, control::protocol::cloud::Phase, menu::Page,
    sidebar::layout_tests::fixture_window,
};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    cx.add_window_view(fixture_window)
}

fn add_cloud(view: &mut SheprdWindow, id: &str, label: &str) {
    let provider = CloudProvider::ALL[0];
    view.endpoints.push(crate::endpoint::Endpoint::new(
        format!("{}:{id}", provider.key()),
        label.into(),
        ConnectTarget::Cloud {
            provider,
            account: "https://cloud.example.com".into(),
            id: id.into(),
            machine: format!("herdr-{id}"),
            session: "default".into(),
        },
        true,
    ));
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn endpoints(targets: &[Target]) -> Vec<&str> {
    targets.iter().map(|target| target.id.as_str()).collect()
}

fn error_code(response: &Response) -> Option<ErrorCode> {
    match response {
        Response::Error { code, .. } => Some(*code),
        _ => None,
    }
}

#[gpui::test]
fn devices_are_named_by_endpoint_machine_or_one_label(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            add_cloud(view, "w1", "Build box");
            add_cloud(view, "w2", "Twin");
            add_cloud(view, "w3", "twin");
            let registry = Registry::default();
            let endpoint = format!("{}:w2", CloudProvider::ALL[0].key());
            let found = targets(
                view,
                &names(&["build box", &endpoint, "w1"]),
                None,
                &registry,
            )
            .unwrap();
            assert_eq!(endpoints(&found), ["w1", "w2"], "each device once");
            for (name, code) in [
                ("twin", ErrorCode::InvalidRequest),
                ("nothing", ErrorCode::NotFound),
                (crate::endpoint::LOCAL, ErrorCode::NotFound),
            ] {
                let error = targets(view, &names(&[name]), None, &registry).unwrap_err();
                assert_eq!(error_code(&error), Some(code), "{name}");
            }
            let none = targets(view, &[], None, &registry).unwrap_err();
            assert_eq!(error_code(&none), Some(ErrorCode::InvalidRequest));
        })
    });
}

#[gpui::test]
fn a_batch_names_the_devices_still_saved(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            add_cloud(view, "w1", "One");
            let registry = Registry::default();
            let provider = CloudProvider::ALL[0];
            let target = |id: &str| Target {
                provider,
                id: id.into(),
                endpoint: format!("{}:{id}", provider.key()),
                label: id.into(),
            };
            let id = registry.open_destroy(&[target("w1"), target("gone")]).id();
            let found = targets(view, &[], Some(id), &registry).unwrap();
            assert_eq!(endpoints(&found), ["w1"]);
            let unknown = targets(view, &[], Some(id + 1), &registry).unwrap_err();
            assert_eq!(error_code(&unknown), Some(ErrorCode::NotFound));
            let only_gone = registry.open_destroy(&[target("gone")]).id();
            let empty = targets(view, &[], Some(only_gone), &registry).unwrap_err();
            assert_eq!(error_code(&empty), Some(ErrorCode::NotFound));
        })
    });
}

#[gpui::test]
fn destroying_only_asks_in_the_window(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            add_cloud(view, "w1", "One");
            let response = destroy(view, &names(&["w1"]), None, window, cx);
            let Response::Cloud {
                reply: Reply::Batch { batch },
            } = response
            else {
                panic!("{response:?}");
            };
            assert_eq!(batch.phase, Phase::Confirming);
            assert_eq!(batch.devices.len(), 1);
            assert_eq!(view.menu.page, Some(Page::DestroyCloud));
            assert!(
                view.endpoints
                    .iter()
                    .any(|endpoint| endpoint.id.ends_with(":w1"))
            );
        })
    });
}

#[test]
fn only_the_newest_batches_that_fit_are_listed() {
    let device = crate::control::protocol::cloud::DeviceInfo {
        provider: "coder".into(),
        id: "id".into(),
        endpoint: "coder:id".into(),
        label: "label".into(),
        state: crate::control::protocol::cloud::DeviceState::Failed,
        step: None,
        error: Some("\"".repeat(500)),
    };
    let batches: Vec<BatchInfo> = (1..=32)
        .map(|id| BatchInfo {
            id,
            kind: crate::control::protocol::cloud::BatchKind::Destroy,
            phase: Phase::Done,
            devices: vec![device.clone(); MAX_DEVICES],
        })
        .collect();
    let (kept, omitted) = newest(batches.clone());
    assert!(omitted > 0);
    assert_eq!(kept.len() + omitted, batches.len());
    assert_eq!(kept.last().map(|batch| batch.id), Some(32));
    assert!(serde_json::to_vec(&kept).unwrap().len() <= crate::control::protocol::MAX_MESSAGE / 2);
    assert_eq!(newest(batches[..1].to_vec()), (batches[..1].to_vec(), 0));
}
