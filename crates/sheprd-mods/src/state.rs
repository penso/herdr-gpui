//! What a mod reads about the app: hosts, workspaces, agents, focus, usage,
//! and the focused branch's pull request. The app fills it from what its
//! window already shows; a mod sees only the topics it watches, read-only,
//! and runs again only when one of them changes.

use crate::names::named;
use mlua::{Lua, Table};

/// The most entries of any list a mod is shown.
const MAX_ITEMS: usize = 512;

named! {
    pub enum Topic {
        Agents = "agents",
        Focus = "focus",
        Hosts = "hosts",
        PullRequest = "pull_request",
        Usage = "usage",
        Workspaces = "workspaces",
    }
}

named! {
    pub enum AgentStatus {
        Idle = "idle",
        Working = "working",
        Blocked = "blocked",
        Done = "done",
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub hosts: Vec<Host>,
    pub workspaces: Vec<Workspace>,
    pub agents: Vec<Agent>,
    pub focus: Option<Focus>,
    pub usage: Vec<Usage>,
    pub pull_request: Option<PullRequest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub host: String,
    pub name: String,
    pub focused: bool,
    pub tabs: Vec<Tab>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub host: String,
    pub workspace: String,
    pub name: String,
    pub status: AgentStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub host: String,
    pub workspace: Option<String>,
    pub tab: Option<String>,
    /// The focused workspace's name.
    pub name: Option<String>,
    /// The focused workspace's checkout or folder, on its host.
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Usage {
    pub provider: String,
    pub label: String,
    /// From 0 to 1.
    pub used: f64,
    /// Unix seconds.
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// `open`, `draft`, `merged`, or `closed`.
    pub state: String,
    /// `pending`, `passing`, or `failing`, when checks ran.
    pub checks: Option<String>,
}

impl State {
    /// Whether `topic` differs between two states.
    pub fn changed(&self, other: &Self, topic: Topic) -> bool {
        match topic {
            Topic::Agents => self.agents != other.agents,
            Topic::Focus => self.focus != other.focus,
            Topic::Hosts => self.hosts != other.hosts,
            Topic::PullRequest => self.pull_request != other.pull_request,
            Topic::Usage => self.usage != other.usage,
            Topic::Workspaces => self.workspaces != other.workspaces,
        }
    }

    /// The read-only `app` table a view gets, with only `topics` filled.
    pub(crate) fn to_lua(&self, lua: &Lua, topics: &[Topic]) -> mlua::Result<Table> {
        let app = lua.create_table()?;
        for topic in topics {
            let value = match topic {
                Topic::Agents => {
                    let agents = lua.create_table()?;
                    agents.raw_set("all", list(lua, &self.agents, agent)?)?;
                    for status in AgentStatus::ALL {
                        let matching = self.agents.iter().filter(|a| a.status == *status);
                        agents.raw_set(status.name(), list(lua, matching, agent)?)?;
                    }
                    frozen(agents)
                }
                Topic::Focus => match &self.focus {
                    Some(focus) => {
                        let table = lua.create_table()?;
                        table.raw_set("host", focus.host.as_str())?;
                        table.raw_set("workspace", focus.workspace.as_deref())?;
                        table.raw_set("tab", focus.tab.as_deref())?;
                        table.raw_set("name", focus.name.as_deref())?;
                        table.raw_set("path", focus.path.as_deref())?;
                        frozen(table)
                    }
                    None => continue,
                },
                Topic::Hosts => list(lua, &self.hosts, |lua, host| {
                    let table = lua.create_table()?;
                    table.raw_set("id", host.id.as_str())?;
                    table.raw_set("name", host.name.as_str())?;
                    table.raw_set("connected", host.connected)?;
                    Ok(frozen(table))
                })?,
                Topic::PullRequest => match &self.pull_request {
                    Some(pr) => {
                        let table = lua.create_table()?;
                        table.raw_set("number", pr.number)?;
                        table.raw_set("title", pr.title.as_str())?;
                        table.raw_set("url", pr.url.as_str())?;
                        table.raw_set("state", pr.state.as_str())?;
                        table.raw_set("checks", pr.checks.as_deref())?;
                        frozen(table)
                    }
                    None => continue,
                },
                Topic::Usage => list(lua, &self.usage, |lua, usage| {
                    let table = lua.create_table()?;
                    table.raw_set("provider", usage.provider.as_str())?;
                    table.raw_set("label", usage.label.as_str())?;
                    table.raw_set("used", usage.used)?;
                    table.raw_set("resets_at", usage.resets_at)?;
                    Ok(frozen(table))
                })?,
                Topic::Workspaces => list(lua, &self.workspaces, |lua, workspace| {
                    let table = lua.create_table()?;
                    table.raw_set("id", workspace.id.as_str())?;
                    table.raw_set("host", workspace.host.as_str())?;
                    table.raw_set("name", workspace.name.as_str())?;
                    table.raw_set("focused", workspace.focused)?;
                    let tabs = list(lua, &workspace.tabs, |lua, tab| {
                        let table = lua.create_table()?;
                        table.raw_set("id", tab.id.as_str())?;
                        table.raw_set("name", tab.name.as_str())?;
                        table.raw_set("focused", tab.focused)?;
                        Ok(frozen(table))
                    })?;
                    table.raw_set("tabs", tabs)?;
                    Ok(frozen(table))
                })?,
            };
            app.raw_set(topic.name(), value)?;
        }
        Ok(frozen(app))
    }
}

fn agent(lua: &Lua, agent: &Agent) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("id", agent.id.as_str())?;
    table.raw_set("host", agent.host.as_str())?;
    table.raw_set("workspace", agent.workspace.as_str())?;
    table.raw_set("name", agent.name.as_str())?;
    table.raw_set("status", agent.status.name())?;
    Ok(frozen(table))
}

fn list<'a, T: 'a>(
    lua: &Lua,
    items: impl IntoIterator<Item = &'a T>,
    each: impl Fn(&Lua, &T) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for item in items.into_iter().take(MAX_ITEMS) {
        table.raw_push(each(lua, item)?)?;
    }
    Ok(frozen(table))
}

fn frozen(table: Table) -> Table {
    table.set_readonly(true);
    table
}
