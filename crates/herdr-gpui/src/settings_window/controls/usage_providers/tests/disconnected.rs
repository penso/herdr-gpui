use super::*;
use crate::{endpoint::Endpoint, state::ConnectionStatus};
use core::prelude::v1::test;
use herdr_client::ConnectTarget;

fn mask(id: &str) -> [bool; usage::PROVIDER_COUNT] {
    let mut mask = [false; usage::PROVIDER_COUNT];
    let index = usage::providers()
        .position(|provider| provider.id() == id)
        .unwrap();
    mask[index] = true;
    mask
}

#[gpui::test]
fn disconnected_host_switch_discovers_services_with_usage_off_without_reopening_settings(
    cx: &mut TestAppContext,
) {
    let (view, cx) = fixture(cx);
    view.update(cx, |view, _| {
        view.config.usage.show = false;
    });
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    let reply = source.update(cx, |source, cx| {
        source.config.usage.show = false;
        source.endpoints[0].live.status = ConnectionStatus::Connecting;
        let target = source.endpoints[0].connection.target.clone();
        let reply = source
            .usage
            .installation_discovery_peer_for_test(target, mask("codex"));
        cx.notify();
        reply
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    view.read_with(cx, |view, _| assert!(view.usage_provider_scope.is_none()));

    source.update(cx, |source, cx| {
        let mut endpoint = Endpoint::new(
            "next".into(),
            "Next disconnected host".into(),
            ConnectTarget::Wsl {
                distro: "isolated-next-host".into(),
                session: "test".into(),
            },
            true,
        );
        endpoint.live.status = ConnectionStatus::Connecting;
        source.endpoints.push(endpoint);
        source.selected_endpoint = 1;
        source.selection_epoch += 1;
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_none());
    reply(mask("gemini"));
    source.update(cx, |source, cx| {
        source.usage.poll(
            None,
            &source.config.usage,
            &Default::default(),
            0,
            false,
            std::time::Instant::now(),
        );
        assert!(source.usage.current().is_none());
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-gemini").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_none());
    view.read_with(cx, |view, _| {
        assert!(!view.config.usage.show);
        assert!(view.config.usage.hide_providers.is_empty());
    });
}
