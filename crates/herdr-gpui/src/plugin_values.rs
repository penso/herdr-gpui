//! What plugins and hooks report for the sidebar, from each host's snapshot.
//!
//! Herdr offers endpoint clients no `plugin.*` method, but a snapshot carries
//! plugin output: custom `$name` values reported for agents and workspaces
//! (`pane.report_metadata`, `workspace.report_metadata`) and the status labels
//! integrations choose. The sidebar draws them only where Herdr's
//! `[ui.sidebar]` rows list them, so Settings > Plugins lists what is reported
//! and previews the rows. Everything here is pure and bounded.
use crate::{
    HerdrWindow,
    config::{
        AgentLayout, AgentToken, SidebarLayout, SidebarScope, SpaceLayout, SpaceToken, TokenStyle,
        sidebar::ConfiguredToken,
    },
};
use herdr_client::protocol::ClientShellSnapshot;
use std::collections::{BTreeMap, btree_map::Entry};

#[cfg(test)]
mod tests;

/// Distinct custom values listed per host; reporters choose the keys.
const MAX_REPORTED: usize = 64;
/// Characters of a reported value or label shown as an example.
const SAMPLE_LIMIT: usize = 48;

/// A custom `$name` value reported on one host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reported {
    pub scope: SidebarScope,
    /// The reported key, without `$`.
    pub name: String,
    /// One current value, as safe bounded display text.
    pub sample: String,
}

/// What one host's plugins and hooks currently report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HostReport {
    pub host: String,
    /// Sorted by scope then name, at most [`MAX_REPORTED`].
    pub reported: Vec<Reported>,
    /// Agents whose integration names their statuses itself.
    pub labelled_agents: usize,
    pub label_sample: Option<String>,
}

fn sample(text: &str) -> Option<String> {
    let text = crate::notifications::safe_text(text, SAMPLE_LIMIT);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Plugin output in `snapshot`, bounded and safe to display.
pub(crate) fn host_report(host: &str, snapshot: &ClientShellSnapshot) -> HostReport {
    let mut reported = BTreeMap::<(SidebarScope, &str), String>::new();
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
            let full = reported.len() >= MAX_REPORTED;
            if let (Entry::Vacant(new), Some(value)) =
                (reported.entry((scope, name.as_str())), sample(value))
                && !full
            {
                new.insert(value);
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
    HostReport {
        host: host.to_owned(),
        reported: reported
            .into_iter()
            .map(|((scope, name), sample)| Reported {
                scope,
                name: name.to_owned(),
                sample,
            })
            .collect(),
        labelled_agents: usize::from(label_sample.is_some()) + labels.count(),
        label_sample,
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
    /// The token as `[ui.sidebar]` rows spell it.
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
pub(crate) fn sidebar_values(reports: &[HostReport], layout: &SidebarLayout) -> Vec<SidebarValue> {
    let mut values = BTreeMap::<(SidebarScope, &str), SidebarValue>::new();
    for report in reports {
        for reported in &report.reported {
            values
                .entry((reported.scope, reported.name.as_str()))
                .or_insert_with(|| SidebarValue {
                    scope: reported.scope,
                    name: reported.name.clone(),
                    sample: Some(reported.sample.clone()),
                    hosts: Vec::new(),
                    shown: layout.shows(reported.scope, &format!("${}", reported.name)),
                })
                .hosts
                .push(report.host.clone());
        }
    }
    let agents = layout
        .agents
        .rows
        .iter()
        .flatten()
        .filter_map(|token| match &token.token {
            AgentToken::Custom(name) => Some((SidebarScope::Agents, name.as_str())),
            _ => None,
        });
    let spaces = layout
        .spaces
        .rows
        .iter()
        .flatten()
        .filter_map(|token| match &token.token {
            SpaceToken::Custom(name) => Some((SidebarScope::Spaces, name.as_str())),
            _ => None,
        });
    for (scope, name) in agents.chain(spaces) {
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

/// One piece of a previewed sidebar row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewPart {
    StateIcon,
    /// Text with the style its configured token and matching rule give it.
    Text(String, PreviewRole, TokenStyle),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewRole {
    Name,
    Detail,
    /// A value a plugin or integration reported.
    Plugin,
}

/// Example data the preview fills built-in tokens with, and what the hosts report.
pub(crate) struct PreviewSource<'a> {
    pub values: &'a [SidebarValue],
    pub label_sample: Option<&'a str>,
    /// Shown for `machine` only when more than one host is listed, as in Herdr.
    pub machine: Option<&'a str>,
}

impl PreviewSource<'_> {
    fn custom(&self, scope: SidebarScope, name: &str) -> Option<PreviewPart> {
        self.values
            .iter()
            .find(|value| value.scope == scope && value.name == name)
            .and_then(|value| value.sample.clone())
            .map(|sample| PreviewPart::Text(sample, PreviewRole::Plugin, TokenStyle::default()))
    }
}

fn text(value: &str, role: PreviewRole) -> Option<PreviewPart> {
    Some(PreviewPart::Text(
        value.to_owned(),
        role,
        TokenStyle::default(),
    ))
}

/// Each token takes its configured style, or the first matching rule's; a
/// matching `hide` rule drops it, as the sidebar's `style_for` does. Rows
/// without a value disappear, as in Herdr's sidebar.
fn rows<T>(
    rows: &[Vec<ConfiguredToken<T>>],
    mut part: impl FnMut(&T) -> Option<PreviewPart>,
) -> Vec<Vec<PreviewPart>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .filter_map(|configured| match part(&configured.token)? {
                    PreviewPart::Text(text, role, _) => {
                        let style = configured.style_for(&text)?;
                        Some(PreviewPart::Text(text, role, style))
                    }
                    PreviewPart::StateIcon => Some(PreviewPart::StateIcon),
                })
                .collect::<Vec<_>>()
        })
        .filter(|row| !row.is_empty())
        .collect()
}

/// An example agent drawn with the default agent `rows`.
pub(crate) fn preview_agent(
    layout: &AgentLayout,
    source: &PreviewSource<'_>,
) -> Vec<Vec<PreviewPart>> {
    use PreviewRole::{Detail, Name, Plugin};
    rows(&layout.rows, |token| match token {
        AgentToken::StateIcon => Some(PreviewPart::StateIcon),
        AgentToken::StateText => match source.label_sample {
            Some(label) => text(label, Plugin),
            None => text("working", Detail),
        },
        AgentToken::Machine => source.machine.and_then(|machine| text(machine, Detail)),
        AgentToken::Workspace => text("herdr-gpui", Name),
        AgentToken::Tab => text("settings", Detail),
        AgentToken::Pane => text("server", Detail),
        AgentToken::Agent => text("Claude Code", Detail),
        AgentToken::TerminalTitle | AgentToken::TerminalTitleStripped => text("just ci", Detail),
        AgentToken::Custom(name) => source.custom(SidebarScope::Agents, name),
    })
}

/// An example workspace drawn with the space `rows`.
pub(crate) fn preview_space(
    layout: &SpaceLayout,
    source: &PreviewSource<'_>,
) -> Vec<Vec<PreviewPart>> {
    use PreviewRole::{Detail, Name};
    rows(&layout.rows, |token| match token {
        SpaceToken::StateIcon => Some(PreviewPart::StateIcon),
        SpaceToken::StateText => text("working", Detail),
        SpaceToken::Workspace => text("herdr-gpui", Name),
        SpaceToken::Branch => text("feat/plugin-values", Detail),
        SpaceToken::GitStatus => text("\u{2191}2", Detail),
        SpaceToken::Custom(name) => source.custom(SidebarScope::Spaces, name),
    })
}

impl HerdrWindow {
    /// What every enabled, connected host's plugins report, in sidebar order.
    pub(crate) fn plugin_reports(&self) -> Vec<HostReport> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(_, endpoint)| endpoint.enabled)
            .filter_map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let snapshot = live.snapshot.as_deref()?;
                live.status
                    .is_connected()
                    .then(|| host_report(&endpoint.label, snapshot))
            })
            .collect()
    }
}
