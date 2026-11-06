#![allow(clippy::unwrap_used)]
use super::*;
use crate::{menu::Page, sidebar::layout_tests::fixture_window};
use gpui::TestAppContext;

fn op(provider: &str, count: u32) -> CreateOp {
    CreateOp {
        provider: provider.into(),
        count,
        template: None,
        preset: None,
        shape: Shape::Machine,
        install: true,
        setup: true,
    }
}

fn code(result: Result<Creation, Response>) -> Option<ErrorCode> {
    match result {
        Err(Response::Error { code, .. }) => Some(code),
        Err(response) => panic!("{response:?}"),
        Ok(_) => None,
    }
}

#[gpui::test]
fn a_request_names_a_provider_and_options_that_apply_to_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let registry = Registry::default();
        let first = CloudProvider::ALL[0];
        let unavailable = cloud::unavailable().is_some();
        let ok = |result| {
            if unavailable {
                assert_eq!(code(result), Some(ErrorCode::Unsupported));
            } else {
                assert_eq!(code(result), None);
            }
        };
        ok(creation(view, op(first.key(), 2), &registry));
        ok(creation(
            view,
            op(&cloud::name(first).to_uppercase(), 1),
            &registry,
        ));
        assert_eq!(
            code(creation(view, op("nimbus", 1), &registry)),
            Some(ErrorCode::NotFound)
        );
        if unavailable {
            return;
        }
        for count in [0, MAX_CREATE + 1] {
            assert_eq!(
                code(creation(view, op(first.key(), count), &registry)),
                Some(ErrorCode::InvalidRequest)
            );
        }
        for &provider in CloudProvider::ALL {
            let mut templated = op(provider.key(), 1);
            templated.template = Some("dev".into());
            let mut sandboxed = op(provider.key(), 1);
            sandboxed.shape = Shape::Sandbox;
            #[cfg(feature = "coder")]
            let coder = provider == CloudProvider::Coder;
            #[cfg(not(feature = "coder"))]
            let coder = false;
            #[cfg(feature = "azure")]
            let azure = provider == CloudProvider::Azure;
            #[cfg(not(feature = "azure"))]
            let azure = false;
            let expected = |fits: bool| (!fits).then_some(ErrorCode::InvalidRequest);
            assert_eq!(
                code(creation(view, templated, &registry)),
                expected(coder),
                "{provider:?}"
            );
            assert_eq!(
                code(creation(view, sandboxed, &registry)),
                expected(azure),
                "{provider:?}"
            );
        }
    });
}

#[gpui::test]
fn machines_being_made_count_toward_the_device_list(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    if cloud::unavailable().is_some() {
        return;
    }
    cx.update(|_, cx| {
        let view = view.read(cx);
        let registry = Registry::default();
        let provider = CloudProvider::ALL[0];
        let pending = registry.open(
            crate::control::batch::BatchKind::Create,
            creation(view, op(provider.key(), MAX_CREATE), &registry)
                .unwrap()
                .devices(),
            (),
        );
        assert_eq!(registry.creating(|_| false), MAX_CREATE as usize);
        let room = MAX_DEVICES - MAX_CREATE as usize;
        let mut kept = Vec::new();
        for _ in 0..room / MAX_CREATE as usize {
            kept.push(
                registry.open(
                    crate::control::batch::BatchKind::Create,
                    creation(view, op(provider.key(), MAX_CREATE), &registry)
                        .unwrap()
                        .devices(),
                    (),
                ),
            );
        }
        assert_eq!(
            code(creation(view, op(provider.key(), 1), &registry)),
            Some(ErrorCode::InvalidRequest)
        );
        drop(pending);
        assert!(registry.creating(|_| false) < MAX_DEVICES);
    });
}

#[gpui::test]
fn asking_opens_the_question_and_creates_nothing(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    if cloud::unavailable().is_some() {
        return;
    }
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let response = ask(view, op(CloudProvider::ALL[0].key(), 2), window, cx);
            let Response::Cloud {
                reply: crate::control::protocol::cloud::Reply::Batch { batch },
            } = response
            else {
                panic!("{response:?}");
            };
            assert_eq!(batch.devices.len(), 2);
            assert_eq!(view.menu.page, Some(Page::CreateCloud));
        })
    });
}
