use super::*;
use crate::container::{Engine, lifecycle};

#[gpui::test]
fn stopped_containers_do_not_retry_in_any_window(cx: &mut gpui::TestAppContext) {
    let target = ConnectTarget::Cloud {
        provider: sheprd_client::CloudProvider::Container,
        account: "docker".into(),
        id: "herdr-test-intentional-stop".into(),
        machine: "herdr-test-intentional-stop".into(),
        session: "default".into(),
    };
    // Exercises the same stop boundary as Settings and idle, without an engine.
    lifecycle::stop(Engine::Docker, "herdr-test-intentional-stop", || Ok(())).unwrap();
    let views: Vec<_> = (0..2)
        .map(|_| {
            cx.add_window_view(|window, cx| {
                let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
                view.endpoints.push(Endpoint::new(
                    "container:test".into(),
                    "Stopped".into(),
                    target.clone(),
                    true,
                ));
                view
            })
            .0
        })
        .collect();
    for view in &views {
        view.update(cx, |view, cx| {
            for _ in 0..3 {
                view.poll_endpoints(cx);
            }
            let endpoint = &mut view.endpoints[1];
            assert_eq!(endpoint.status(), "stopped");
            assert_eq!(endpoint.attempts, 0);
            assert!(endpoint.connection.handle.is_none());
            // Other automatic callers of connect cannot bypass the intent either.
            endpoint.connect(view.options, false);
            assert_eq!(endpoint.attempts, 0);
        });
    }
    let mut other_engine = target.clone();
    if let ConnectTarget::Cloud { account, .. } = &mut other_engine {
        *account = "podman".into();
    }
    assert!(!lifecycle::suspended(&other_engine));
    lifecycle::resume(&target);
    for view in views {
        view.read_with(cx, |view, _| assert!(!view.endpoints[1].suspended()));
    }
}
