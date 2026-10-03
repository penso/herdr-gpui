//! Settings > Plugins: what each host's plugins report, which of it the
//! sidebar shows, and each host's bound plugin actions, run on explicit clicks.
use super::*;
use crate::{
    config::SidebarScope,
    plugins::{HostState, PluginAction, PluginHost, SidebarValue},
    search_input::{Changed, SearchInput},
};

/// What Herdr's endpoint cannot do yet, so the pane never implies otherwise.
const LIMITS: &str = "Herdr does not yet let this app list, enable, or disable plugins, read \
their logs, or run actions that have no key binding; use `herdr plugin` on the host for those. \
Nothing is installed from here.";

const VALUES_NOTE: &str = "Plugins and hooks report these values for agents and workspaces. \
Showing one adds it as its own row to [ui.sidebar] in your Herdr config, so the terminal app \
shows it too.";

/// A run the host's connection accepted into its queue; the daemon may still refuse it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Sent {
    action: String,
    host: String,
}

pub(super) struct Plugins {
    search: Entity<SearchInput>,
    pub(super) status: Option<crate::Result<Sent>>,
    /// Records sidebar edits instead of writing the shared config.
    #[cfg(test)]
    edits: Option<Vec<Edit>>,
    _search_changed: Subscription,
}

impl Plugins {
    pub(super) fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search plugin values and actions...", cx)
        });
        let changed = cx.subscribe(&search, |this, _, _: &Changed, cx| {
            this.body_scroll.set_offset(Point::default());
            cx.notify();
        });
        Self {
            search,
            status: None,
            #[cfg(test)]
            edits: None,
            _search_changed: changed,
        }
    }

    pub(super) fn refresh_appearance(&self, config: &Config, theme: &Theme, cx: &mut App) {
        self.search.update(cx, |input, cx| {
            input.set_appearance(config.ui.clone(), theme.clone(), cx);
        });
    }
}

fn scope_label(scope: SidebarScope) -> &'static str {
    match scope {
        SidebarScope::Agents => "Agents",
        SidebarScope::Spaces => "Workspaces",
    }
}

impl SettingsWindow {
    pub(super) fn render_plugin_controls(&self, cx: &mut Context<Self>) -> Div {
        let query = self.plugins.search.read(cx).text().trim().to_lowercase();
        let hosts = self
            .source
            .upgrade()
            .map(|source| source.read(cx).plugin_hosts());
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .min_w_0()
            .child(self.plugins.search.clone());
        if let Some(status) = &self.plugins.status {
            let (text, color) = match status {
                Ok(sent) => (
                    format!("Sent \u{201c}{}\u{201d} to {}.", sent.action, sent.host),
                    self.theme.muted,
                ),
                Err(error) => (error.to_string(), self.theme.palette[1]),
            };
            body = body.child(
                div()
                    .debug_selector(|| "plugins-status".into())
                    .min_w_0()
                    .text_color(rgb(color))
                    .child(text),
            );
        }
        let Some(hosts) = hosts else {
            return body.child(self.control_note(
                "Open a session window to see plugin output and actions. Local preferences remain available.",
            ));
        };
        body = body.child(self.render_sidebar_values(&hosts, &query, cx));
        if hosts.is_empty() {
            body = body.child(self.control_note("No hosts are enabled."));
        }
        for host in hosts {
            body = body.child(self.render_plugin_host(host, &query, cx));
        }
        body.child(self.control_note(LIMITS))
    }

    /// The custom values hosts report and the agent status labels, each with a
    /// switch that edits the shared `[ui.sidebar]` rows.
    fn render_sidebar_values(
        &self,
        hosts: &[PluginHost],
        query: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let ready = self.controls_shared_ready();
        let layout = &self.config.sidebar_layout;
        let mut card = self
            .control_card("Sidebar values")
            .child(self.control_note(VALUES_NOTE));
        if !ready {
            card = card.child(self.control_note(if !cfg!(unix) {
                "Shared Herdr settings are read-only on this platform."
            } else if self.shared.is_none() {
                "Shared settings are unavailable. Reload from General to retry."
            } else {
                "Waiting for Settings to finish loading or saving."
            }));
        }
        let labelled: usize = hosts.iter().map(|host| host.output.labelled_agents).sum();
        let label_sample = hosts
            .iter()
            .find_map(|host| host.output.label_sample.as_deref());
        let status = "state_text";
        if query.is_empty() || "status labels".contains(query) {
            let detail = match label_sample {
                Some(sample) => format!(
                    "{labelled} agent{} name their own status, such as \u{201c}{sample}\u{201d}.",
                    if labelled == 1 { "" } else { "s" }
                ),
                None => "Idle, working, blocked, or done, or an integration's own words.".into(),
            };
            card = card.child(self.sidebar_value_switch(
                SidebarScope::Agents,
                status,
                "Agent status labels".into(),
                detail,
                layout.shows(SidebarScope::Agents, status),
                ready,
                cx,
            ));
        }
        let values: Vec<SidebarValue> = crate::plugins::sidebar_values(hosts, layout)
            .into_iter()
            .filter(|value| value.matches(query))
            .collect();
        if values.is_empty() {
            return card.child(self.control_note(if query.is_empty() {
                "No plugin or hook reports custom values on a connected host yet."
            } else {
                "No matching values."
            }));
        }
        for value in values {
            let detail = match &value.sample {
                Some(sample) => format!(
                    "{} \u{b7} \u{201c}{sample}\u{201d} on {}",
                    scope_label(value.scope),
                    value.hosts.join(", ")
                ),
                None => format!(
                    "{} \u{b7} not reported on a connected host",
                    scope_label(value.scope)
                ),
            };
            card = card.child(self.sidebar_value_switch(
                value.scope,
                &value.token(),
                value.token(),
                detail,
                value.shown,
                ready,
                cx,
            ));
        }
        card
    }

    #[allow(clippy::too_many_arguments)]
    fn sidebar_value_switch(
        &self,
        scope: SidebarScope,
        token: &str,
        title: String,
        detail: String,
        shown: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(self.control_note(detail));
        let id = format!("plugin-value-{}-{token}", scope.key());
        let switch = self.control_switch(id, label, shown, ready);
        if !ready {
            return switch;
        }
        let token = token.to_owned();
        switch.on_click(cx.listener(move |this, _, _, cx| {
            let edit = Edit::SidebarToken {
                scope,
                token: token.clone(),
                shown: !shown,
            };
            #[cfg(test)]
            if let Some(edits) = &mut this.plugins.edits {
                edits.push(edit);
                return;
            }
            this.save_shared(edit, cx);
        }))
    }

    fn render_plugin_host(&self, host: PluginHost, query: &str, cx: &mut Context<Self>) -> Div {
        let ready = host.state == HostState::Selected { ready: true };
        let mut card = self.control_card(host.label.clone());
        match host.state {
            HostState::Selected { ready: true } => {}
            HostState::Selected { ready: false } => {
                card = card.child(self.control_note("Waiting for this host to become ready."));
            }
            HostState::Other => {
                card = card.child(
                    self.control_note("Select this host in the sidebar to run its actions."),
                );
            }
            HostState::Disconnected => return card.child(self.control_note("Not connected.")),
        }
        if let Some(view) = &host.output.agent_view {
            card = card.child(self.control_note(format!(
                "The agent panel shows the \u{201c}{view}\u{201d} view a plugin chose."
            )));
        }
        if host.actions.is_empty() {
            return card.child(self.control_note("No plugin actions are bound on this host."));
        }
        let mut matched = host
            .actions
            .iter()
            .filter(|action| action.matches(query))
            .peekable();
        if matched.peek().is_none() {
            return card.child(self.control_note("No matching plugin actions."));
        }
        for action in matched {
            card = card.child(self.render_plugin_action(&host, action, ready, cx));
        }
        card
    }

    fn render_plugin_action(
        &self,
        host: &PluginHost,
        action: &PluginAction,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = SharedString::from(format!(
            "plugin-run-{}-{}",
            host.endpoint, action.command_id
        ));
        let selector = id.clone();
        let mut run = self
            .control_choice(id, "Run", false, ready)
            .debug_selector(move || selector.to_string())
            .flex_none()
            .px(px(10.))
            .py(px(3.));
        if ready {
            let (endpoint, boot, label) =
                (host.endpoint.clone(), host.boot.clone(), host.label.clone());
            let (command, name) = (action.command_id.clone(), action.label.clone());
            run = run.on_click(cx.listener(move |this, _, _, cx| {
                this.run_plugin_action(&endpoint, &boot, &command, &name, &label, cx);
            }));
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(action.label.clone()),
                    )
                    .when(!action.bindings.is_empty(), |column| {
                        column.child(self.control_note(action.bindings.join(", ")))
                    }),
            )
            .child(run)
    }

    fn run_plugin_action(
        &mut self,
        endpoint: &str,
        boot: &str,
        command: &str,
        action: &str,
        host: &str,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .source
            .update(cx, |source, cx| {
                let result = source.run_plugin_action(endpoint, boot, command);
                cx.notify();
                result
            })
            .unwrap_or(Err(crate::Error::NotConnected));
        self.plugins.status = Some(result.map(|()| Sent {
            action: action.to_owned(),
            host: host.to_owned(),
        }));
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use herdr_client::{
        ConnectTarget,
        protocol::{ClientShellCommand, ClientShellCommandAction, ClientShellSnapshot},
    };
    use std::sync::Arc;

    fn snapshot(
        boot: &str,
        commands: &[(&str, &str, ClientShellCommandAction)],
    ) -> Arc<ClientShellSnapshot> {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        snapshot.boot_id = boot.into();
        snapshot.commands = commands
            .iter()
            .map(|(id, description, action)| ClientShellCommand {
                command_id: (*id).into(),
                binding_label: String::new(),
                binding_labels: vec![format!("prefix+{id}")],
                action: *action,
                description: Some((*description).into()),
            })
            .collect();
        Arc::new(snapshot)
    }

    fn connected(endpoint: &mut crate::state::LiveState, snapshot: Arc<ClientShellSnapshot>) {
        endpoint.snapshot = Some(snapshot);
        endpoint.status = crate::state::ConnectionStatus::Connected;
    }

    /// A main window with a selected local host, a second connected host, a
    /// disconnected host, and a disabled one.
    fn hosts(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        connected(
            &mut view.live,
            snapshot(
                "boot-local",
                &[
                    (
                        "dash",
                        "Open dashboard",
                        ClientShellCommandAction::PluginAction,
                    ),
                    ("build", "Build", ClientShellCommandAction::Shell),
                ],
            ),
        );
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:remote".into(),
            "Remote".into(),
            ConnectTarget::Socket("/unused-plugins-remote.sock".into()),
            true,
        );
        connected(
            &mut remote.live,
            snapshot(
                "boot-remote",
                &[("sync", "Sync notes", ClientShellCommandAction::PluginAction)],
            ),
        );
        let offline = crate::endpoint::Endpoint::new(
            "ssh:offline".into(),
            "Offline".into(),
            ConnectTarget::Socket("/unused-plugins-offline.sock".into()),
            true,
        );
        let mut disabled = crate::endpoint::Endpoint::new(
            "ssh:disabled".into(),
            "Disabled".into(),
            ConnectTarget::Socket("/unused-plugins-disabled.sock".into()),
            false,
        );
        connected(
            &mut disabled.live,
            snapshot(
                "boot-disabled",
                &[("hidden", "Hidden", ClientShellCommandAction::PluginAction)],
            ),
        );
        view.endpoints.extend([remote, offline, disabled]);
        // Plugin output: a summary on both hosts' agents, a workspace value
        // and status labels locally, and an agent view on the remote.
        let report = |live: &mut crate::state::LiveState, workspace: bool| {
            let snapshot = Arc::make_mut(live.snapshot.as_mut().unwrap());
            for agent in &mut snapshot.agents {
                agent.tokens = vec![("summary".into(), "fix auth".into())];
                agent.state_labels = vec![("working".into(), "deep in the mines".into())];
            }
            for space in &mut snapshot.workspaces {
                space.tokens = if workspace {
                    vec![("ci".into(), "green".into())]
                } else {
                    Vec::new()
                };
            }
            snapshot.agent_view_label = (!workspace).then(|| "review".into());
        };
        report(&mut view.live, true);
        report(&mut view.endpoints[1].live, false);
        view.activation_deadline = Some(std::time::Instant::now());
        view
    }

    #[gpui::test]
    fn sidebar_values_toggle_shared_rows_and_follow_search(cx: &mut TestAppContext) {
        let main = cx.add_window(hosts);
        let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut view = SettingsWindow::new(weak, cx);
            view.section = Section::Plugins;
            view.config.sidebar_layout = crate::config::SidebarLayout::from_daemon_config(
                &"[ui.sidebar.agents]\nrows = [[\"agent\"], [\"$old\"]]\n"
                    .parse()
                    .unwrap(),
            )
            .unwrap();
            view.plugins.edits = Some(Vec::new());
            view
        });
        let draw = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        };
        let click = |cx: &mut VisualTestContext, id: &'static str| {
            draw(cx);
            let bounds = cx.debug_bounds(id).unwrap();
            cx.simulate_click(bounds.center(), Default::default());
            cx.run_until_parked();
        };
        // Without shared settings the switches are inert.
        click(cx, "plugin-value-agents-$summary");
        view.read_with(cx, |view, _| {
            assert_eq!(view.plugins.edits.as_ref().unwrap().len(), 0);
        });
        view.update(cx, |view, cx| {
            view.shared = Some(herdr_settings::Settings::parse_text("").unwrap());
            cx.notify();
        });
        for id in [
            "plugin-value-agents-$summary",
            "plugin-value-agents-$old",
            "plugin-value-agents-state_text",
            "plugin-value-spaces-$ci",
        ] {
            click(cx, id);
        }
        view.read_with(cx, |view, _| {
            let edits: Vec<_> = view
                .plugins
                .edits
                .iter()
                .flatten()
                .map(|edit| match edit {
                    Edit::SidebarToken {
                        scope,
                        token,
                        shown,
                    } => (*scope, token.as_str(), *shown),
                    other => panic!("unexpected edit {other:?}"),
                })
                .collect();
            assert_eq!(
                edits,
                [
                    (SidebarScope::Agents, "$summary", true),
                    (SidebarScope::Agents, "$old", false),
                    (SidebarScope::Agents, "state_text", true),
                    (SidebarScope::Spaces, "$ci", true),
                ]
            );
        });
        view.update(cx, |view, cx| {
            view.plugins.search.update(cx, |input, cx| {
                input.set_text_selected("GREEN", cx);
            });
        });
        draw(cx);
        assert!(cx.debug_bounds("plugin-value-spaces-$ci").is_some());
        assert!(cx.debug_bounds("plugin-value-agents-$summary").is_none());
        assert!(cx.debug_bounds("plugin-value-agents-state_text").is_none());
    }

    #[gpui::test]
    fn lists_each_enabled_host_and_runs_only_on_the_ready_selected_one(cx: &mut TestAppContext) {
        let main = cx.add_window(hosts);
        let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
        let projected = cx.update(|cx| main.update(cx, |view, _, _| view.plugin_hosts()).unwrap());
        assert_eq!(
            projected
                .iter()
                .map(|host| (host.endpoint.as_str(), host.state, host.actions.len()))
                .collect::<Vec<_>>(),
            [
                ("local", HostState::Selected { ready: false }, 1),
                ("ssh:remote", HostState::Other, 1),
                ("ssh:offline", HostState::Disconnected, 0),
            ]
        );
        assert_eq!(projected[0].boot, "boot-local");
        assert_eq!(projected[0].actions[0].label, "Open dashboard");

        let (view, cx) = cx.add_window_view(|_, cx| {
            let mut view = SettingsWindow::new(weak, cx);
            view.section = Section::Plugins;
            view
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        // Both connected hosts list their actions; neither can run yet.
        let local = cx.debug_bounds("plugin-run-local-dash").unwrap();
        assert!(cx.debug_bounds("plugin-run-ssh:remote-sync").is_some());
        assert!(cx.debug_bounds("plugin-run-local-build").is_none());
        assert!(cx.debug_bounds("plugin-run-ssh:disabled-hidden").is_none());
        cx.simulate_click(local.center(), Default::default());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.plugins.status.is_none()));

        // Another host's action is refused rather than run on the selected one.
        view.update(cx, |view, cx| {
            view.run_plugin_action(
                "ssh:remote",
                "boot-remote",
                "sync",
                "Sync notes",
                "Remote",
                cx,
            );
            assert!(matches!(
                view.plugins.status,
                Some(Err(crate::Error::PluginHostNotSelected))
            ));
            view.plugins.search.update(cx, |input, cx| {
                input.set_text_selected("PREFIX+SYNC", cx);
            });
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("plugins-status").is_some());
        assert!(cx.debug_bounds("plugin-run-local-dash").is_none());
        assert!(cx.debug_bounds("plugin-run-ssh:remote-sync").is_some());
    }
}
