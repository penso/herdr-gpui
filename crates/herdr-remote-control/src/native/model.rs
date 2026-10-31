//! What the native app reads: the same layout, chat summaries, and pull
//! requests the web app's lists draw, as typed JSON instead of markup.
//! Chat types are `herdr_chat_protocol`'s own; nothing here mirrors them.

use crate::{
    ansi::{self, Color, Style},
    control::pull_requests::{PullRequests, Tone, WorkspacePullRequest},
    hub::{self, ChatRow, Layout, Summary},
    load::Load,
};
use herdr_chat_protocol::{Cursor, Permission, Status, Transcript};
use herdr_client::protocol::{AgentStatus, ClientShellWorktree};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Every workspace of this computer, with its chats and terminal tabs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Home {
    pub(crate) computer: String,
    pub(crate) fingerprint: String,
    /// Whether the Herdr daemon is reachable now.
    pub(crate) connected: bool,
    /// Permission requests waiting across every chat: the app's badge.
    pub(crate) waiting: usize,
    /// This computer's CPU and memory; absent until first read, and from
    /// older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) load: Option<Load>,
    pub(crate) workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Workspace {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) branch: Option<String>,
    pub(crate) cwd: String,
    /// The repository checkout it is, as Herdr reports it: a phone lists a
    /// repository's linked worktrees under it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) worktree: Option<ClientShellWorktree>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pull_request: Option<PullRequest>,
    pub(crate) chats: Vec<Chat>,
    pub(crate) terminals: Vec<Terminal>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct PullRequest {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) tone: Tone,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Chat {
    pub(crate) pane: String,
    pub(crate) tab: String,
    pub(crate) title: String,
    pub(crate) agent: Option<String>,
    pub(crate) status: AgentStatus,
    /// Absent until the daemon's connection to the chat is up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) summary: Option<ChatSummary>,
}

/// What a list row shows of a chat's transcript.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChatSummary {
    pub(crate) connected: bool,
    pub(crate) status: Status,
    /// The oldest permission request waiting.
    pub(crate) question: Option<Permission>,
    pub(crate) questions: usize,
    pub(crate) last_text: Option<String>,
    pub(crate) turn_started_ms: Option<u64>,
    /// When the latest turn ended; absent from older chat programs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_turn_ended_ms: Option<u64>,
    pub(crate) usage_percent: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Terminal {
    pub(crate) tab: String,
    pub(crate) label: String,
    pub(crate) cwd: Option<String>,
    pub(crate) zoomed: bool,
    pub(crate) panes: Vec<Pane>,
    /// The agent Herdr detects in the tab, the most pressing first.
    pub(crate) agent: Option<TerminalAgent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pane {
    pub(crate) pane: String,
    pub(crate) label: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) focused: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct TerminalAgent {
    pub(crate) name: Option<String>,
    pub(crate) status: AgentStatus,
    /// The pane it runs in; absent from older servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pane: Option<String>,
    /// What it works on, from its terminal title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
}

/// A chat's whole transcript and the stream position it includes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChatSnapshot {
    pub(crate) pane: String,
    /// Absent until the daemon has heard from the chat once.
    pub(crate) cursor: Option<Cursor>,
    pub(crate) connected: bool,
    pub(crate) transcript: Transcript,
}

/// One run of equally styled terminal text. Only the attributes that are
/// set are written.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Run {
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) bg: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) dim: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) inverse: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) strike: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) hidden: bool,
}

/// Whether a flag is off, so it is left out; serde passes a reference.
fn is_false(value: &bool) -> bool {
    !*value
}

/// The 16 palette colors by name, normal then bright.
const COLOR_NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

fn color(color: Color) -> Option<String> {
    match color {
        Color::Default => None,
        Color::Named(index) => {
            let name = COLOR_NAMES[usize::from(index % 8)];
            Some(if index >= 8 {
                format!("bright_{name}")
            } else {
                name.to_owned()
            })
        }
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    }
}

impl Run {
    fn new(style: &Style, text: String) -> Self {
        Self {
            text,
            fg: color(style.fg),
            bg: color(style.bg),
            bold: style.bold,
            dim: style.dim,
            italic: style.italic,
            underline: style.underline,
            inverse: style.reverse,
            strike: style.strike,
            hidden: style.hidden,
        }
    }
}

/// A screen read from Herdr as lines of runs, and its widest line.
pub(crate) fn screen(text: &str) -> (usize, Vec<Vec<Run>>) {
    let ansi::Screen { lines, columns } = ansi::parse(text);
    let rows = lines
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|(style, text)| Run::new(&style, text))
                .collect()
        })
        .collect();
    (columns, rows)
}

/// The pull request Herdr UI found for `workspace` in the same boot.
fn pull_request<'a>(
    pull_requests: &'a PullRequests,
    boot: &str,
    workspace: &hub::Workspace,
) -> Option<&'a WorkspacePullRequest> {
    (pull_requests.boot_id == boot).then_some(())?;
    pull_requests.entries.iter().find(|entry| {
        entry.workspace_id == workspace.id
            && entry.cwd == workspace.cwd
            && Some(entry.branch.as_str()) == workspace.branch.as_deref()
    })
}

impl From<Summary> for ChatSummary {
    fn from(summary: Summary) -> Self {
        Self {
            connected: summary.connected,
            status: summary.status,
            question: summary.question,
            questions: summary.questions,
            last_text: summary.last_text,
            turn_started_ms: summary.turn_started_ms,
            last_turn_ended_ms: summary.last_turn_ended_ms,
            usage_percent: summary.usage_percent,
        }
    }
}

fn chat(row: &ChatRow, summaries: &mut HashMap<String, Summary>) -> Chat {
    let summary = summaries.remove(&row.pane);
    // Herdr may know an ACP chat only as `acp`, without the display name
    // `herdr-acp` reported; the chat's own transcript names its agent.
    let agent = summary
        .as_ref()
        .map(|summary| summary.agent.trim())
        .filter(|agent| !agent.is_empty())
        .map(str::to_owned)
        .or_else(|| row.agent.clone());
    Chat {
        pane: row.pane.clone(),
        tab: row.tab.clone(),
        title: row.title.clone(),
        agent,
        status: row.status,
        summary: summary.map(ChatSummary::from),
    }
}

fn terminal(terminal: &hub::Terminal) -> Terminal {
    Terminal {
        tab: terminal.tab.clone(),
        label: terminal.label.clone(),
        cwd: terminal.cwd.clone(),
        zoomed: terminal.zoomed,
        panes: terminal
            .panes
            .iter()
            .map(|pane| Pane {
                pane: pane.pane.clone(),
                label: pane.label.clone(),
                cwd: pane.cwd.clone(),
                focused: pane.focused,
            })
            .collect(),
        agent: terminal.agent.as_ref().map(|agent| TerminalAgent {
            name: agent.name.clone(),
            status: agent.status,
            pane: Some(agent.pane.clone()),
            title: agent.title.clone(),
        }),
    }
}

/// Where a home is drawn from; `summaries` holds the chats whose
/// connection is up, by pane.
pub(crate) struct Sources<'a> {
    pub(crate) computer: &'a str,
    pub(crate) fingerprint: &'a str,
    pub(crate) layout: &'a Layout,
    pub(crate) load: Option<Load>,
    pub(crate) summaries: HashMap<String, Summary>,
    pub(crate) pull_requests: &'a PullRequests,
}

impl Home {
    pub(crate) fn project(sources: Sources<'_>) -> Self {
        let Sources {
            computer,
            fingerprint,
            layout,
            load,
            mut summaries,
            pull_requests,
        } = sources;
        let waiting = summaries.values().map(|summary| summary.questions).sum();
        let workspaces = layout
            .workspaces
            .iter()
            .map(|workspace| Workspace {
                id: workspace.id.clone(),
                label: workspace.label.clone(),
                branch: workspace.branch.clone(),
                cwd: workspace.cwd.clone(),
                worktree: workspace.worktree.clone(),
                pull_request: pull_request(pull_requests, &layout.boot, workspace).map(|pr| {
                    PullRequest {
                        number: pr.number,
                        url: pr.url.clone(),
                        tone: pr.tone,
                    }
                }),
                chats: workspace
                    .chats
                    .iter()
                    .map(|row| chat(row, &mut summaries))
                    .collect(),
                terminals: workspace.terminals.iter().map(terminal).collect(),
            })
            .collect();
        Self {
            computer: computer.to_owned(),
            fingerprint: fingerprint.to_owned(),
            connected: layout.connected,
            waiting,
            load,
            workspaces,
        }
    }
}
