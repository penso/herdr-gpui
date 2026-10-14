use super::*;
use crate::state::ConnectionStatus;
use core::prelude::v1::test;

impl HerdrWindow {
    pub(crate) fn queue_integration_response_for_test(&mut self, response: Value) {
        self.integrations.clear_pending();
        let scope = self.integration_scope().unwrap();
        let id = "installed-provider-test".to_owned();
        scope.inbox.lock().unwrap().pending = Some((id.clone(), Some(Ok(response))));
        self.integrations.scope = Some(scope);
        self.integrations.pending = Some((id, Operation::List));
        self.integrations.busy = true;
    }
}

fn connected(source: &mut HerdrWindow) {
    let endpoint = &mut source.endpoints[source.selected_endpoint];
    endpoint.live.status = ConnectionStatus::Connected;
    endpoint.live.snapshot = Some(Arc::new(crate::sidebar::layout_tests::snapshot(1)));
}

#[gpui::test]
fn available_commands_project_registry_services_without_requiring_installed_hooks(
    cx: &mut TestAppContext,
) {
    let window = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        window.update(cx, |source, _, cx| {
            connected(source);
            source.queue_integration_response_for_test(json!({"result":{
                "type":"integration_list", "integrations":[
                    {"target":"claude","label":"Claude Code","command":"claude","available":true,"state":"not_installed"},
                    {"target":"codex","label":"Codex","command":"codex","available":true,"state":"outdated"},
                    {"target":"droid","label":"Droid","command":"droid","available":true,"state":"current"},
                    {"target":"cursor","label":"Cursor","command":"cursor","available":false,"state":"current"},
                    {"target":"unknown","label":"Unknown","command":"unknown","available":true,"state":"current"}
                ]
            }}));
            source.poll_integrations(cx);
            let selected = source.installed_usage_providers();
            let ids = usage::providers().zip(selected)
                .filter_map(|(provider, shown)| shown.then_some(provider.id()))
                .collect::<Vec<_>>();
            assert_eq!(ids, ["codex", "claude", "factory"]);
            assert!(source.usage.current().is_none());
        }).unwrap();
    });
}

#[gpui::test]
fn foreign_or_retired_connection_rows_and_late_responses_never_project(cx: &mut TestAppContext) {
    let window = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        window.update(cx, |source, _, cx| {
            connected(source);
            let response = json!({"result":{"type":"integration_list","integrations":[
                {"target":"claude","label":"Claude","command":"claude","available":true,"state":"current"}
            ]}});
            source.queue_integration_response_for_test(response.clone());
            let old_inbox = source.integrations.scope.as_ref().unwrap().inbox.clone();
            source.endpoints[0].generation += 1;
            assert!(!source.installed_usage_providers().into_iter().any(|shown| shown));
            source.poll_integrations(cx);
            assert!(source.integrations.rows.is_empty());
            assert!(old_inbox.lock().unwrap().pending.is_none());

            source.queue_integration_response_for_test(response.clone());
            source.poll_integrations(cx);
            assert!(source.installed_usage_providers().into_iter().any(|shown| shown));
            Arc::make_mut(source.endpoints[0].live.snapshot.as_mut().unwrap()).boot_id = "new-boot".into();
            assert!(!source.installed_usage_providers().into_iter().any(|shown| shown));
            source.poll_integrations(cx);
            assert!(source.integrations.rows.is_empty());

            source.queue_integration_response_for_test(response);
            source.selection_epoch += 1;
            source.poll_integrations(cx);
            assert!(source.integrations.rows.is_empty());
        }).unwrap();
    });
}
