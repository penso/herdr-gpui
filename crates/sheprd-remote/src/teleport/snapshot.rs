//! Typed views of `herdr` CLI output. The GUI connection's snapshot carries no
//! split tree, launch argv, or agent session, so Teleport reads the socket API
//! snapshot through the CLI on each host. Only the fields used are decoded;
//! unknown fields are ignored so newer daemons stay readable.

use serde::Deserialize;

/// `{"id": .., "result": T}`, as every `herdr` API subcommand prints it.
#[derive(Debug, Deserialize)]
pub struct Envelope<T> {
    pub result: T,
}

#[derive(Debug, Deserialize)]
pub struct SnapshotResult {
    pub snapshot: HostSnapshot,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HostSnapshot {
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub panes: Vec<Pane>,
    #[serde(default)]
    pub layouts: Vec<Layout>,
}

impl HostSnapshot {
    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.workspace_id == id)
    }

    /// The workspace's tabs in their sidebar order.
    pub fn tabs_of(&self, workspace_id: &str) -> Vec<&Tab> {
        let mut tabs: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace_id)
            .collect();
        tabs.sort_by_key(|tab| tab.number);
        tabs
    }

    pub fn pane(&self, id: &str) -> Option<&Pane> {
        self.panes.iter().find(|pane| pane.pane_id == id)
    }

    pub fn layout(&self, tab_id: &str) -> Option<&Layout> {
        self.layouts.iter().find(|layout| layout.tab_id == tab_id)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    pub workspace_id: String,
    pub worktree: Option<Worktree>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Worktree {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub checkout_path: String,
    pub is_linked_worktree: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tab {
    pub tab_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub number: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pane {
    pub pane_id: String,
    pub tab_id: String,
    pub cwd: Option<String>,
    pub foreground_cwd: Option<String>,
    pub agent: Option<String>,
    pub agent_session: Option<AgentSession>,
    /// The daemon reports the pane's agent as working. Statuses this client
    /// does not know read as not working, so a new one cannot break parsing.
    #[serde(default, rename = "agent_status", deserialize_with = "is_working")]
    pub working: bool,
}

fn is_working<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<String>::deserialize(d)?.as_deref() == Some("working"))
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentSession {
    pub agent: String,
    pub kind: SessionKind,
    pub source: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Id,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Layout {
    pub tab_id: String,
    pub panes: Vec<LayoutPane>,
    pub splits: Vec<LayoutSplit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutPane {
    pub pane_id: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Right,
    Down,
}

impl SplitDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Down => "down",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayoutSplit {
    /// `split_<n>_root` or `split_<n>_<path>`, where the path spells the
    /// branch from the root: `0` is the first child, `1` the second.
    pub id: String,
    pub direction: SplitDirection,
    pub ratio: f32,
    pub rect: Rect,
}

#[derive(Debug, Deserialize)]
pub struct ProcessInfoResult {
    pub process_info: ProcessInfo,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProcessInfo {
    pub foreground_process_group_id: Option<u32>,
    pub shell_pid: Option<u32>,
    #[serde(default)]
    pub foreground_processes: Vec<Process>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Process {
    pub pid: u32,
    #[serde(default)]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
}

impl ProcessInfo {
    /// Every process in the foreground job; none while the shell itself is in
    /// the foreground (an idle prompt).
    pub fn foreground_pids(&self) -> Vec<u32> {
        if self.foreground_process_group_id.is_none()
            || self.foreground_process_group_id == self.shell_pid
        {
            return Vec::new();
        }
        self.foreground_processes.iter().map(|p| p.pid).collect()
    }

    /// The foreground job's leader, or `None` while the shell itself is in
    /// the foreground (an idle prompt).
    pub fn foreground_job(&self) -> Option<&Process> {
        let group = self.foreground_process_group_id?;
        if Some(group) == self.shell_pid {
            return None;
        }
        self.foreground_processes
            .iter()
            .find(|process| process.pid == group)
            .or_else(|| self.foreground_processes.iter().min_by_key(|p| p.pid))
            .filter(|process| !process.argv.is_empty())
    }
}

/// `worktree_created`: the new workspace and the pane its first tab holds.
#[derive(Debug, Deserialize)]
pub struct WorktreeCreated {
    pub workspace: CreatedWorkspace,
    pub tab: CreatedTab,
    pub root_pane: CreatedPane,
    pub worktree: CreatedWorktree,
}

#[derive(Debug, Deserialize)]
pub struct CreatedWorkspace {
    pub workspace_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatedTab {
    pub tab_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatedPane {
    pub pane_id: String,
    /// Stable identity for callers that must not retry against a replaced pane.
    pub terminal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatedWorktree {
    pub path: String,
}

/// `workspace_created`.
#[derive(Debug, Deserialize)]
pub struct WorkspaceCreated {
    pub workspace: CreatedWorkspace,
}

/// `tab_list`.
#[derive(Debug, Deserialize)]
pub struct TabList {
    pub tabs: Vec<CreatedTab>,
}

/// `tab_created`.
#[derive(Debug, Deserialize)]
pub struct TabCreated {
    pub tab: CreatedTab,
    pub root_pane: CreatedPane,
}

/// `pane_info`, as `pane split` prints the new pane.
#[derive(Debug, Deserialize)]
pub struct PaneInfoResult {
    pub pane: CreatedPane,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_cli_snapshot_envelope_and_ignores_unknown_fields() {
        let json = r#"{"id":"cli:api:snapshot","result":{"type":"session_snapshot","snapshot":{
            "version":"0.9.1","protocol":22,"future":true,
            "workspaces":[{"workspace_id":"w1","label":"feat","worktree":{"checkout_path":"/w/feat",
              "is_linked_worktree":true,"repo_key":"/r/.git","repo_name":"r","repo_root":"/r"}}],
            "tabs":[{"tab_id":"w1:t2","workspace_id":"w1","number":2,"label":"b"},
                    {"tab_id":"w1:t1","workspace_id":"w1","number":1}],
            "panes":[{"pane_id":"w1:p1","tab_id":"w1:t1","cwd":"/w/feat","agent":"claude",
              "agent_session":{"agent":"claude","kind":"id","source":"herdr:claude","value":"abc"}}],
            "layouts":[{"tab_id":"w1:t1","zoomed":false,"focused_pane_id":"w1:p1",
              "area":{"x":0,"y":0,"width":10,"height":5},
              "panes":[{"pane_id":"w1:p1","focused":true,"rect":{"x":0,"y":0,"width":10,"height":5}}],
              "splits":[]}],
            "agents":[]}}}"#;
        let snapshot = serde_json::from_str::<Envelope<SnapshotResult>>(json)
            .unwrap()
            .result
            .snapshot;
        let tabs: Vec<_> = snapshot
            .tabs_of("w1")
            .iter()
            .map(|t| t.tab_id.as_str())
            .collect();
        assert_eq!(tabs, ["w1:t1", "w1:t2"]);
        let pane = snapshot.pane("w1:p1").unwrap();
        assert_eq!(pane.agent_session.as_ref().unwrap().kind, SessionKind::Id);
        assert!(
            snapshot
                .workspace("w1")
                .unwrap()
                .worktree
                .as_ref()
                .unwrap()
                .is_linked_worktree
        );
        assert_eq!(snapshot.layout("w1:t1").unwrap().panes[0].pane_id, "w1:p1");
    }

    #[test]
    fn an_idle_shell_has_no_foreground_job() {
        let info: ProcessInfo = serde_json::from_str(
            r#"{"foreground_process_group_id":7,"shell_pid":7,
                "foreground_processes":[{"pid":7,"argv":["-zsh"],"cwd":"/w"}]}"#,
        )
        .unwrap();
        assert!(info.foreground_job().is_none());
        assert!(info.foreground_pids().is_empty());
        let info: ProcessInfo = serde_json::from_str(
            r#"{"foreground_process_group_id":9,"shell_pid":7,
                "foreground_processes":[{"pid":12,"argv":["node","x"]},{"pid":9,"argv":["npm","run","dev"]}]}"#,
        )
        .unwrap();
        assert_eq!(info.foreground_job().unwrap().argv, ["npm", "run", "dev"]);
        assert_eq!(info.foreground_pids(), [12, 9]);
        assert!(ProcessInfo::default().foreground_pids().is_empty());
    }
}
