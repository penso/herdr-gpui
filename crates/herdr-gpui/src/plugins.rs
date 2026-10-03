//! What each connected daemon's plugins expose to endpoint clients.
//!
//! Herdr's endpoint offers no `plugin.*` method, so a GUI client cannot list,
//! enable, disable, or read the logs of plugins. What the snapshot does carry
//! is plugin output: actions a host binds with a `[[keys.command]]` of
//! `type = "plugin_action"` (runnable through `command.invoke`), custom `$name`
//! values reported for agents and workspaces, status labels, and the active
//! agent view. This module projects those per host and runs bound actions; it
//! never installs anything.
use crate::{
    Error, HerdrWindow, Result,
    config::{AgentToken, SidebarLayout, SidebarScope, SpaceToken},
    palette::Target,
};
use std::collections::{BTreeMap, btree_map::Entry};

/// Distinct custom values listed per host; reporters choose the keys.
const MAX_REPORTED: usize = 64;
/// Characters of a reported value or label shown as an example.
const SAMPLE_LIMIT: usize = 48;
use herdr_client::{
    Method,
    protocol::{ClientShellCommand, ClientShellCommandAction, ClientShellSnapshot},
};

/// A bound plugin action, as listed by one daemon boot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginAction {
    pub command_id: String,
    pub label: String,
    pub bindings: Vec<String>,
}

impl PluginAction {
    fn new(command: &ClientShellCommand) -> Self {
        let mut bindings = command.binding_labels.clone();
        if !command.binding_label.is_empty() && !bindings.contains(&command.binding_label) {
            bindings.push(command.binding_label.clone());
        }
        bindings.retain(|binding| !binding.trim().is_empty());
        Self {
            command_id: command.command_id.clone(),
            label: command
                .description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .unwrap_or("Unnamed plugin action")
                .to_owned(),
            bindings,
        }
    }

    /// Case-insensitive match on the label or any binding; `query` is already lowercase.
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.label.to_lowercase().contains(query)
            || self
                .bindings
                .iter()
                .any(|binding| binding.to_lowercase().contains(query))
    }
}

/// The plugin actions in `snapshot`'s command manifest, in manifest order.
pub(crate) fn plugin_actions(snapshot: &ClientShellSnapshot) -> Vec<PluginAction> {
    snapshot
        .commands
        .iter()
        .filter(|command| command.action == ClientShellCommandAction::PluginAction)
        .map(PluginAction::new)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostState {
    /// The window's selected host; actions run on its focused pane.
    Selected {
        ready: bool,
    },
    /// Connected, but actions only run on the selected host.
    Other,
    Disconnected,
}

/// A custom `$name` value reported on one host, for agents or workspaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reported {
    pub scope: SidebarScope,
    /// The reported key, without `$`.
    pub name: String,
    /// One current value, as safe bounded display text.
    pub sample: String,
    /// How many agents or workspaces report it.
    pub count: usize,
}

impl Reported {
    /// The token as `[ui.sidebar]` rows spell it.
    pub fn token(&self) -> String {
        format!("${}", self.name)
    }
}

/// What one host's plugins and hooks currently report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PluginOutput {
    /// Sorted by scope then name, at most [`MAX_REPORTED`].
    pub reported: Vec<Reported>,
    /// Agents whose integration names their statuses itself.
    pub labelled_agents: usize,
    pub label_sample: Option<String>,
    /// The agent view a plugin selected for the agent panel.
    pub agent_view: Option<String>,
}

fn sample(text: &str) -> Option<String> {
    let text = crate::notifications::safe_text(text, SAMPLE_LIMIT);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Plugin output in `snapshot`, bounded and safe to display.
pub(crate) fn plugin_output(snapshot: &ClientShellSnapshot) -> PluginOutput {
    let mut reported = BTreeMap::<(SidebarScope, &str), (String, usize)>::new();
    let agents = snapshot
        .agents
        .iter()
        .map(|agent| (SidebarScope::Agents, &agent.tokens));
    let spaces = snapshot
        .workspaces
        .iter()
        .map(|workspace| (SidebarScope::Spaces, &workspace.tokens));
    for (scope, tokens) in agents.chain(spaces) {
        for (name, value) in tokens {
            let Some(value) = sample(value) else { continue };
            let full = reported.len() >= MAX_REPORTED;
            match reported.entry((scope, name.as_str())) {
                Entry::Occupied(mut seen) => seen.get_mut().1 += 1,
                Entry::Vacant(new) if !full => {
                    new.insert((value, 1));
                }
                Entry::Vacant(_) => {}
            }
        }
    }
    let mut labels = snapshot.agents.iter().filter_map(|agent| {
        agent
            .state_labels
            .iter()
            .find_map(|(_, label)| sample(label))
    });
    let label_sample = labels.next();
    PluginOutput {
        reported: reported
            .into_iter()
            .map(|((scope, name), (sample, count))| Reported {
                scope,
                name: name.to_owned(),
                sample,
                count,
            })
            .collect(),
        labelled_agents: usize::from(label_sample.is_some()) + labels.count(),
        label_sample,
        agent_view: snapshot.agent_view_label.as_deref().and_then(sample),
    }
}

/// A custom sidebar value across hosts: reported now, configured, or both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SidebarValue {
    pub scope: SidebarScope,
    pub name: String,
    /// The first reporting host's example, if any host reports it now.
    pub sample: Option<String>,
    /// Labels of the hosts reporting it, in host order.
    pub hosts: Vec<String>,
    /// Whether `[ui.sidebar]` rows already show it.
    pub shown: bool,
}

impl SidebarValue {
    pub fn token(&self) -> String {
        format!("${}", self.name)
    }

    /// Case-insensitive match on the name or example; `query` is already lowercase.
    pub fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_lowercase().contains(query)
            || self
                .sample
                .as_ref()
                .is_some_and(|sample| sample.to_lowercase().contains(query))
    }
}

/// Every custom value the hosts report, plus those the layout already shows
/// so a stale one can still be hidden, sorted by scope then name.
pub(crate) fn sidebar_values(hosts: &[PluginHost], layout: &SidebarLayout) -> Vec<SidebarValue> {
    let mut values = BTreeMap::<(SidebarScope, &str), SidebarValue>::new();
    for host in hosts {
        for reported in &host.output.reported {
            let value = values
                .entry((reported.scope, reported.name.as_str()))
                .or_insert_with(|| SidebarValue {
                    scope: reported.scope,
                    name: reported.name.clone(),
                    sample: Some(reported.sample.clone()),
                    hosts: Vec::new(),
                    shown: layout.shows(reported.scope, &reported.token()),
                });
            value.hosts.push(host.label.clone());
        }
    }
    let configured = layout
        .agents
        .rows
        .iter()
        .flatten()
        .filter_map(|token| match &token.token {
            AgentToken::Custom(name) => Some((SidebarScope::Agents, name.as_str())),
            _ => None,
        })
        .chain(
            layout
                .spaces
                .rows
                .iter()
                .flatten()
                .filter_map(|token| match &token.token {
                    SpaceToken::Custom(name) => Some((SidebarScope::Spaces, name.as_str())),
                    _ => None,
                }),
        );
    for (scope, name) in configured {
        values.entry((scope, name)).or_insert_with(|| SidebarValue {
            scope,
            name: name.to_owned(),
            sample: None,
            hosts: Vec::new(),
            shown: true,
        });
    }
    values.into_values().collect()
}

/// One enabled host, the plugin actions its current boot lists, and what its
/// plugins report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginHost {
    pub endpoint: String,
    pub label: String,
    pub boot: String,
    pub state: HostState,
    pub actions: Vec<PluginAction>,
    pub output: PluginOutput,
}

impl HerdrWindow {
    /// Every enabled host, in sidebar order, with its bound plugin actions.
    pub(crate) fn plugin_hosts(&self) -> Vec<PluginHost> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| endpoint.enabled)
            .map(|(index, endpoint)| {
                let selected = index == self.selected_endpoint;
                let live = if selected { &self.live } else { &endpoint.live };
                let snapshot = live
                    .snapshot
                    .as_deref()
                    .filter(|_| live.status.is_connected());
                let state = match snapshot {
                    None => HostState::Disconnected,
                    Some(_) if selected => HostState::Selected {
                        ready: self.plugin_action_ready(),
                    },
                    Some(_) => HostState::Other,
                };
                PluginHost {
                    endpoint: endpoint.id.clone(),
                    label: endpoint.label.clone(),
                    boot: snapshot.map(|s| s.boot_id.clone()).unwrap_or_default(),
                    state,
                    actions: snapshot.map(plugin_actions).unwrap_or_default(),
                    output: snapshot.map(plugin_output).unwrap_or_default(),
                }
            })
            .collect()
    }

    fn plugin_action_ready(&self) -> bool {
        self.activation_deadline.is_none()
            && self
                .endpoints
                .get(self.selected_endpoint)
                .is_some_and(|endpoint| endpoint.surface_requested())
            && self.input_ready()
    }

    /// Runs a plugin action listed by `endpoint`'s `boot` on that host's focused
    /// workspace, tab, and pane, as the palette runs a configured command.
    /// Success means the request was queued, not that the daemon accepted it.
    pub(crate) fn run_plugin_action(
        &mut self,
        endpoint: &str,
        boot: &str,
        command_id: &str,
    ) -> Result<()> {
        if self
            .endpoints
            .get(self.selected_endpoint)
            .is_none_or(|selected| selected.id != endpoint)
        {
            return Err(Error::PluginHostNotSelected);
        }
        if !self.plugin_action_ready() {
            return Err(Error::PluginHostNotReady);
        }
        let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        if snapshot.boot_id != boot
            || !snapshot.commands.iter().any(|command| {
                command.command_id == command_id
                    && command.action == ClientShellCommandAction::PluginAction
            })
        {
            return Err(Error::PluginActionChanged);
        }
        let params = Target::capture(snapshot).invocation(
            snapshot,
            command_id,
            ClientShellCommandAction::PluginAction,
        )?;
        if !self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
            handle.request(boot, Method::CommandInvoke, params)
        }) {
            return Err(Error::NotConnected);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn command(id: &str, action: ClientShellCommandAction) -> ClientShellCommand {
        ClientShellCommand {
            command_id: id.into(),
            binding_label: "prefix+p".into(),
            binding_labels: vec!["prefix+p".into(), "ctrl+alt+p".into()],
            action,
            description: Some(" Open dashboard ".into()),
        }
    }

    fn snapshot(commands: Vec<ClientShellCommand>) -> ClientShellSnapshot {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        snapshot.commands = commands;
        snapshot
    }

    #[test]
    fn lists_only_plugin_actions_in_manifest_order() {
        let snapshot = snapshot(vec![
            command("shell", ClientShellCommandAction::Shell),
            command("b", ClientShellCommandAction::PluginAction),
            command("popup", ClientShellCommandAction::Popup),
            command("a", ClientShellCommandAction::PluginAction),
            command("future", ClientShellCommandAction::Unknown),
        ]);
        let actions = plugin_actions(&snapshot);
        assert_eq!(
            actions
                .iter()
                .map(|action| action.command_id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(actions[0].label, "Open dashboard");
        assert_eq!(actions[0].bindings, ["prefix+p", "ctrl+alt+p"]);
    }

    #[test]
    fn labels_and_bindings_fall_back_without_duplicates() {
        let mut command = command("x", ClientShellCommandAction::PluginAction);
        command.description = Some("   ".into());
        command.binding_labels = vec![" ".into()];
        command.binding_label = "prefix+y".into();
        let action = PluginAction::new(&command);
        assert_eq!(action.label, "Unnamed plugin action");
        assert_eq!(action.bindings, ["prefix+y"]);
        command.description = None;
        command.binding_label = String::new();
        command.binding_labels.clear();
        let action = PluginAction::new(&command);
        assert_eq!(action.label, "Unnamed plugin action");
        assert!(action.bindings.is_empty());
    }

    #[test]
    fn output_counts_reported_values_per_scope_safely_and_bounded() {
        let mut snapshot = snapshot(Vec::new());
        let mut agent = snapshot.agents[0].clone();
        agent.state_labels.clear();
        snapshot.agents.clear();
        for workspace in &mut snapshot.workspaces {
            workspace.tokens.clear();
        }
        snapshot.agent_view_label = None;
        assert_eq!(plugin_output(&snapshot), PluginOutput::default());
        agent.tokens = vec![
            ("summary".into(), "fix \u{202e}auth".into()),
            ("model".into(), "   ".into()),
        ];
        let mut second = agent.clone();
        second.tokens = vec![("summary".into(), "other".into())];
        second.state_labels = vec![("working".into(), " deep in the mines ".into())];
        snapshot.agents = vec![agent.clone(), second, agent];
        snapshot.workspaces[0].tokens = vec![("summary".into(), "ci green".into())];
        snapshot.agent_view_label = Some("review\u{7}".into());
        let output = plugin_output(&snapshot);
        assert_eq!(
            output.reported,
            [
                Reported {
                    scope: SidebarScope::Agents,
                    name: "summary".into(),
                    sample: "fix auth".into(),
                    count: 3,
                },
                Reported {
                    scope: SidebarScope::Spaces,
                    name: "summary".into(),
                    sample: "ci green".into(),
                    count: 1,
                },
            ]
        );
        assert_eq!(output.reported[0].token(), "$summary");
        assert_eq!(output.labelled_agents, 1);
        assert_eq!(output.label_sample.as_deref(), Some("deep in the mines"));
        assert_eq!(output.agent_view.as_deref(), Some("review"));

        snapshot.workspaces[0].tokens = (0..MAX_REPORTED + 8)
            .map(|index| (format!("k{index:03}"), "x".repeat(SAMPLE_LIMIT * 2)))
            .collect();
        let output = plugin_output(&snapshot);
        assert_eq!(output.reported.len(), MAX_REPORTED);
        assert!(
            output
                .reported
                .iter()
                .all(|reported| reported.sample.chars().count() <= SAMPLE_LIMIT)
        );
    }

    #[test]
    fn sidebar_values_merge_hosts_and_keep_configured_ones_hideable() {
        let host = |label: &str, reported: &[(SidebarScope, &str, &str)]| PluginHost {
            endpoint: label.to_lowercase(),
            label: label.into(),
            boot: String::new(),
            state: HostState::Other,
            actions: Vec::new(),
            output: PluginOutput {
                reported: reported
                    .iter()
                    .map(|(scope, name, sample)| Reported {
                        scope: *scope,
                        name: (*name).into(),
                        sample: (*sample).into(),
                        count: 1,
                    })
                    .collect(),
                ..PluginOutput::default()
            },
        };
        let hosts = [
            host(
                "Local",
                &[
                    (SidebarScope::Agents, "summary", "fix auth"),
                    (SidebarScope::Spaces, "ci", "green"),
                ],
            ),
            host("Remote", &[(SidebarScope::Agents, "summary", "deploy")]),
        ];
        let layout = SidebarLayout::from_daemon_config(
            &"[ui.sidebar.agents]\nrows = [[\"agent\", \"$summary\"], [\"$old\"]]\n"
                .parse()
                .unwrap(),
        )
        .unwrap();
        let values = sidebar_values(&hosts, &layout);
        let summary = |values: &[SidebarValue]| {
            values
                .iter()
                .map(|value| {
                    (
                        value.scope,
                        value.name.clone(),
                        value.sample.clone(),
                        value.hosts.join(","),
                        value.shown,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            summary(&values),
            [
                (
                    SidebarScope::Agents,
                    "old".into(),
                    None,
                    String::new(),
                    true
                ),
                (
                    SidebarScope::Agents,
                    "summary".into(),
                    Some("fix auth".into()),
                    "Local,Remote".into(),
                    true
                ),
                (
                    SidebarScope::Spaces,
                    "ci".into(),
                    Some("green".into()),
                    "Local".into(),
                    false
                ),
            ]
        );
        assert_eq!(values[1].token(), "$summary");
        assert!(values[1].matches("fix"));
        assert!(values[1].matches("summ"));
        assert!(!values[0].matches("fix"));
    }

    #[test]
    fn search_matches_label_or_binding_case_insensitively() {
        let action = PluginAction::new(&command("x", ClientShellCommandAction::PluginAction));
        assert!(action.matches(""));
        assert!(action.matches("dashboard"));
        assert!(action.matches("ctrl+alt"));
        assert!(!action.matches("missing"));
        // Callers pass a lowercased query; an uppercase one never matches.
        assert!(!action.matches("DASHBOARD"));
    }
}
