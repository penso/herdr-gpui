use super::*;
use crate::{endpoint::Endpoint, state::ConnectionStatus};
use core::prelude::v1::test;
use herdr_client::ConnectTarget;
use serde_json::{Value, json};

fn available(claude: bool, codex: bool) -> Value {
    json!({"result":{"type":"integration_list","integrations":[
        {"target":"claude","label":"Claude Code","command":"claude","available":claude,"state":"not_installed"},
        {"target":"codex","label":"Codex","command":"codex","available":codex,"state":"not_installed"},
        {"target":"cursor","label":"Cursor","command":"cursor","available":false,"state":"current"},
        {"target":"unknown","label":"Unknown","command":"not-a-usage-service","available":true,"state":"current"}
    ]}})
}

fn metadata(cx: &mut VisualTestContext, view: &Entity<SettingsWindow>, response: Value) {
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    let scope = source.update(cx, |source, _| {
        let endpoint = &mut source.endpoints[source.selected_endpoint];
        endpoint.live.status = ConnectionStatus::Connected;
        endpoint.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(1)));
        source.queue_integration_response_for_test(response);
        source.integration_scope().unwrap()
    });
    view.update(cx, |view, _| view.usage_provider_scope = Some(scope));
    source.update(cx, |source, cx| source.poll_integrations(cx));
    cx.run_until_parked();
}

fn selected(ids: &[&str]) -> [bool; usage::PROVIDER_COUNT] {
    let mut selected = [false; usage::PROVIDER_COUNT];
    for (index, provider) in usage::providers().enumerate() {
        selected[index] = ids.contains(&provider.id());
    }
    selected
}

#[gpui::test]
fn fresh_disabled_clis_and_saved_choices_are_visible_and_bulk_enable_only_listed_services(
    cx: &mut TestAppContext,
) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    view.update(cx, |view, _| {
        view.config.usage.hide_providers = vec!["claude".into(), "codex".into(), "cursor".into()];
    });
    metadata(cx, &view, available(true, true));
    draw(cx);
    view.read_with(cx, |view, cx| {
        assert!(
            view.source
                .upgrade()
                .unwrap()
                .read(cx)
                .usage
                .current()
                .is_none()
        );
        assert_eq!(
            view.config.usage.hide_providers,
            ["claude", "codex", "cursor"]
        );
    });
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-cursor").is_some());
    assert!(cx.debug_bounds("settings-usage-providers-empty").is_none());
    assert_eq!(
        click(cx, &edits, "settings-usage-providers-all"),
        Some(Preference::UsageProviders(ProviderEdit::EnableMany(
            selected(&["codex", "claude", "cursor"])
        )))
    );
}

#[gpui::test]
fn installed_disabled_service_can_be_reenabled_individually(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    view.update(cx, |view, _| {
        view.config.usage.hide_providers = vec!["codex".into()]
    });
    metadata(cx, &view, available(false, true));
    assert_eq!(
        click(cx, &edits, "settings-usage-provider-codex"),
        Some(Preference::UsageProviders(ProviderEdit::Enable("codex")))
    );
}

#[gpui::test]
fn offline_installation_metadata_adds_codex_missing_from_daemon_path_and_saved_choices(
    cx: &mut TestAppContext,
) {
    let (view, cx) = fixture(cx);
    let edits = recording(cx, &view);
    view.update(cx, |view, _| {
        view.config.usage.hide_providers = vec!["claude".into()];
    });
    metadata(cx, &view, available(true, false));
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_none());
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    source.update(cx, |source, cx| {
        let target = source.endpoints[source.selected_endpoint]
            .connection
            .target
            .clone();
        source
            .usage
            .queue_installation_discovery_for_test(target, selected(&["codex"]));
        assert!(source.usage.poll(
            None,
            &source.config.usage,
            &Default::default(),
            0,
            false,
            std::time::Instant::now(),
        ));
        assert!(source.usage.current().is_none());
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_some());
    assert_eq!(
        click(cx, &edits, "settings-usage-provider-codex"),
        Some(Preference::UsageProviders(ProviderEdit::Disable("codex")))
    );
}

#[gpui::test]
fn unchanged_terminal_notifications_do_not_invalidate_status_bar_settings(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    metadata(cx, &view, available(true, true));
    let notifications = cx.update(|_, cx| {
        cx.new(|cx| {
            cx.observe(&view, |count: &mut usize, _, _| *count += 1)
                .detach();
            0_usize
        })
    });
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    for _ in 0..8 {
        source.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }
    assert_eq!(notifications.read_with(cx, |count, _| *count), 0);
    metadata(cx, &view, available(false, true));
    assert_eq!(notifications.read_with(cx, |count, _| *count), 1);
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
}

#[gpui::test]
fn selected_host_change_refreshes_once_and_excludes_previous_host_sources(cx: &mut TestAppContext) {
    let (view, cx) = fixture(cx);
    metadata(cx, &view, available(true, false));
    install(cx, &view, cached(&["openai"]));
    let source = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    source.update(cx, |source, cx| {
        let mut next = Endpoint::new(
            "next".into(),
            "Next host".into(),
            ConnectTarget::Wsl {
                distro: "isolated-test-host".into(),
                session: "test".into(),
            },
            true,
        );
        next.live.status = ConnectionStatus::Connected;
        next.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(1)));
        source.endpoints.push(next);
        source.selected_endpoint = 1;
        source.selection_epoch += 1;
        source.integrations.error = Some("old-host-error".into());
        cx.notify();
    });
    cx.run_until_parked();
    source.read_with(cx, |source, _| {
        assert_ne!(source.integrations.error.as_deref(), Some("old-host-error"))
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
    assert!(cx.debug_bounds("settings-usage-provider-openai").is_none());
    source.update(cx, |source, cx| {
        source.integrations.error = Some("same-scope-error".into());
        cx.notify();
    });
    cx.run_until_parked();
    source.read_with(cx, |source, _| {
        assert_eq!(
            source.integrations.error.as_deref(),
            Some("same-scope-error")
        )
    });
    metadata(cx, &view, available(false, true));
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
    assert!(cx.debug_bounds("settings-usage-provider-openai").is_none());
}

#[gpui::test]
fn retargeting_open_status_bar_tracks_only_the_new_source(cx: &mut TestAppContext) {
    let other = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| {
        other
            .update(cx, |source, _, cx| {
                source.endpoints[0].connection.target = ConnectTarget::Wsl {
                    distro: "isolated-next-source".into(),
                    session: "test".into(),
                };
                source.endpoints[0].live.status = ConnectionStatus::Connected;
                source.endpoints[0].live.snapshot =
                    Some(Arc::new(crate::sidebar::layout_tests::snapshot(1)));
                cx.weak_entity()
            })
            .unwrap()
    });
    let (view, cx) = fixture(cx);
    metadata(cx, &view, available(true, false));
    let previous = view.read_with(cx, |view, _| view.source.upgrade().unwrap());
    view.update(cx, |view, cx| view.retarget_source(weak.clone(), cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.source.entity_id(), weak.entity_id())
    });
    metadata(cx, &view, available(false, true));
    previous.update(cx, |source, cx| {
        source
            .usage
            .set_current_for_test(usage::Host::Local, cached(&["openai"]));
        cx.notify();
    });
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("settings-usage-provider-codex").is_some());
    assert!(cx.debug_bounds("settings-usage-provider-claude").is_none());
    assert!(cx.debug_bounds("settings-usage-provider-openai").is_none());
}
