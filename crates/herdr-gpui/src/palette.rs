use crate::{
    Error, HerdrWindow, NavigationTarget, OwnedNavigationTarget, Result,
    controls::{COMMANDS, Command},
    menu::Page,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
use herdr_client::{
    Method,
    protocol::{AgentStatus, ClientShellCommandAction, ClientShellSnapshot},
};
use serde_json::{Value, json};

pub(crate) use agent_filter::AgentFilter;

mod agent_filter;
#[cfg(test)]
mod interaction_tests;
mod project_open;
mod projects;
mod render;
mod search;

#[derive(Clone, Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PaletteConfig {
    pub double_shift: bool,
    pub project_roots: Vec<String>,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self {
            double_shift: true,
            project_roots: Vec::new(),
        }
    }
}

impl PaletteConfig {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.project_roots.len() > 16
            || self.project_roots.iter().any(|root| {
                root.trim().is_empty() || root.len() > 8192 || root.chars().any(char::is_control)
            })
        {
            return Err(Error::PaletteProjectRoots);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Filter {
    #[default]
    All,
    Navigation,
    Commands,
    Projects,
}

impl Filter {
    const ALL: [Self; 4] = [Self::All, Self::Navigation, Self::Commands, Self::Projects];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Navigation => "Navigation",
            Self::Commands => "Commands",
            Self::Projects => "Projects",
        }
    }

    fn accepts(self, action: &Action) -> bool {
        self == Self::All
            || self
                == match action {
                    Action::Native(_) | Action::Configured(..) => Self::Commands,
                    Action::Go { .. } => Self::Navigation,
                    Action::Project(_) => Self::Projects,
                }
    }
}

#[derive(Clone)]
enum Action {
    Native(Command),
    /// A Go To destination, qualified by the host and daemon boot it was listed from.
    Go {
        endpoint: String,
        boot: String,
        target: OwnedNavigationTarget,
    },
    Configured(String, ClientShellCommandAction),
    Project(projects::Project),
}

struct Entry {
    label: String,
    detail: String,
    badge: SharedString,
    action: Action,
    /// Index of the row this one nests under, indented only while that row
    /// is visible so a search never leaves it hanging beneath nothing.
    parent: Option<usize>,
    /// The daemon-reported status of a Go To row's agent, if it has one.
    status: Option<AgentStatus>,
    fields: search::Fields,
}

impl Entry {
    fn new(
        label: String,
        detail: String,
        badge: impl Into<SharedString>,
        action: Action,
        parent: Option<usize>,
    ) -> Self {
        let badge = badge.into();
        let id = match &action {
            Action::Configured(id, _) => id.as_str(),
            _ => "",
        };
        let fields = search::Fields::new(&label, &format!("{detail} {badge} {id}"));
        Self {
            label,
            detail,
            badge,
            action,
            parent,
            status: None,
            fields,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identity {
    Native(Command),
    Go(String, OwnedNavigationTarget),
    Configured(String),
    Project(std::path::PathBuf),
}

impl Action {
    fn identity(&self) -> Identity {
        match self {
            Self::Native(command) => Identity::Native(*command),
            Self::Go {
                endpoint, target, ..
            } => Identity::Go(endpoint.clone(), target.clone()),
            Self::Configured(id, _) => Identity::Configured(id.clone()),
            Self::Project(project) => Identity::Project(project.path.clone()),
        }
    }
}

#[derive(Clone)]
struct Target {
    boot: String,
    workspace: Option<String>,
    tab: Option<String>,
    pane: Option<String>,
}

impl Target {
    fn capture(snapshot: &ClientShellSnapshot) -> Self {
        Self {
            boot: snapshot.boot_id.clone(),
            workspace: snapshot.focused_workspace_id.clone(),
            tab: snapshot.focused_tab_id.clone(),
            pane: snapshot.focused_pane_id.clone(),
        }
    }

    fn validate_boot(&self, snapshot: &ClientShellSnapshot) -> Result<()> {
        if self.boot.is_empty() || self.boot != snapshot.boot_id {
            return Err(Error::PaletteSessionChanged);
        }
        Ok(())
    }

    fn workspace_exists(&self, snapshot: &ClientShellSnapshot, id: &str) -> Result<()> {
        self.validate_boot(snapshot)?;
        if !snapshot.workspaces.iter().any(|w| w.workspace_id == id) {
            return Err(Error::PaletteWorkspaceRemoved);
        }
        Ok(())
    }

    fn invocation(
        &self,
        snapshot: &ClientShellSnapshot,
        id: &str,
        action: ClientShellCommandAction,
    ) -> Result<Value> {
        self.validate_boot(snapshot)?;
        if action == ClientShellCommandAction::Unknown {
            return Err(Error::UnsupportedCommand);
        }
        if !snapshot
            .commands
            .iter()
            .any(|c| c.command_id == id && c.action == action)
        {
            return Err(Error::PaletteCommandChanged);
        }
        if let Some(id) = &self.workspace {
            self.workspace_exists(snapshot, id)?;
        }
        if let Some(id) = &self.tab
            && !snapshot
                .tabs
                .iter()
                .any(|t| t.tab_id == *id && self.workspace.as_ref() == Some(&t.workspace_id))
        {
            return Err(Error::PaletteTabRemoved);
        }
        if let Some(id) = &self.pane
            && !snapshot.panes.iter().any(|p| {
                p.pane_id == *id
                    && self.workspace.as_ref() == Some(&p.workspace_id)
                    && self.tab.as_ref() == Some(&p.tab_id)
            })
        {
            return Err(Error::PalettePaneRemoved);
        }
        let mut params = json!({"command_id": id});
        for (key, value) in [
            ("workspace_id", &self.workspace),
            ("tab_id", &self.tab),
            ("pane_id", &self.pane),
        ] {
            if let Some(value) = value {
                params[key] = json!(value);
            }
        }
        Ok(params)
    }
}

/// Whether a Go To destination listed from `boot` still exists in `snapshot`.
fn destination_exists(
    snapshot: &ClientShellSnapshot,
    boot: &str,
    target: NavigationTarget<&str>,
) -> Result<()> {
    if boot.is_empty() || boot != snapshot.boot_id {
        return Err(Error::PaletteSessionChanged);
    }
    match target {
        NavigationTarget::Workspace(id) => snapshot
            .workspaces
            .iter()
            .any(|w| w.workspace_id == id)
            .then_some(())
            .ok_or(Error::PaletteWorkspaceRemoved),
        NavigationTarget::Tab(id) => snapshot
            .tabs
            .iter()
            .any(|t| t.tab_id == id)
            .then_some(())
            .ok_or(Error::PaletteTabRemoved),
        NavigationTarget::Pane(id) => snapshot
            .panes
            .iter()
            .any(|p| p.pane_id == id)
            .then_some(())
            .ok_or(Error::PaletteDestinationRemoved),
    }
}

fn status_badge(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Working => "working",
        AgentStatus::Idle => "idle",
        AgentStatus::Unknown => "",
    }
}

/// One host's Go To rows: each workspace, then every pane in it, one row per
/// agent or terminal so no split is hidden behind its tab.
fn go_to_entries(
    endpoint: &str,
    host: Option<&str>,
    snapshot: &ClientShellSnapshot,
    entries: &mut Vec<Entry>,
) {
    let go = |target| Action::Go {
        endpoint: endpoint.to_owned(),
        boot: snapshot.boot_id.clone(),
        target,
    };
    for workspace in &snapshot.workspaces {
        let detail = [
            host,
            Some(&*format!("#{}", workspace.number)),
            workspace.branch.as_deref(),
            projects::launch_root(snapshot, &workspace.workspace_id),
        ]
        .into_iter()
        .flatten()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("  ");
        let parent = entries.len();
        entries.push(Entry::new(
            workspace.label.clone(),
            detail,
            "Workspace",
            go(NavigationTarget::Workspace(workspace.workspace_id.clone())),
            None,
        ));
        let tabs: Vec<_> = snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id)
            .collect();
        let panes: Vec<_> = tabs
            .iter()
            .flat_map(|tab| {
                snapshot
                    .panes
                    .iter()
                    .filter(move |pane| {
                        pane.workspace_id == workspace.workspace_id && pane.tab_id == tab.tab_id
                    })
                    .map(move |pane| (*tab, pane))
            })
            .collect();
        for (tab, pane) in &panes {
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == pane.pane_id);
            let name = match agent {
                Some(agent) => crate::sidebar::agent_name(agent),
                None => pane
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .unwrap_or("Terminal"),
            };
            // As in the sidebar, the tab only earns its place when there is a choice.
            let tab = (tabs.len() > 1 || tab.custom_label).then_some(tab.label.as_str());
            let path = pane.foreground_cwd.as_deref().or(pane.cwd.as_deref());
            let detail = [host, Some(workspace.label.as_str()), tab, path]
                .into_iter()
                .flatten()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("  ");
            let mut entry = Entry::new(
                name.to_owned(),
                detail,
                agent.map_or(SharedString::new_static("Terminal"), |agent| {
                    crate::sidebar::state_label(agent, status_badge(agent.agent_status))
                        .into_owned()
                        .into()
                }),
                go(NavigationTarget::Pane(pane.pane_id.clone())),
                Some(parent),
            );
            entry.status = agent.map(|agent| agent.agent_status);
            entries.push(entry);
        }
    }
}

#[cfg(test)]
fn matches_query(text: &str, query: &str) -> bool {
    let query = query.to_lowercase();
    search::Fields::new(text, "")
        .score(&query.split_whitespace().collect::<Vec<_>>())
        .is_some()
}

/// Ranked results need not keep entry order.
fn is_nested(entry: &Entry, filtered: &[usize]) -> bool {
    entry
        .parent
        .is_some_and(|parent| filtered.contains(&parent))
}

pub(super) struct Palette {
    pub search: Entity<SearchInput>,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    selected: usize,
    scroll: UniformListScrollHandle,
    target: Option<Target>,
    filter: Filter,
    /// Narrows Navigation to agents in one status; other filters ignore it.
    agent_filter: AgentFilter,
    query: String,
    error: Option<String>,
    projects: projects::Collection,
    loading_projects: bool,
    project_task: Option<Task<()>>,
    _scan: projects::Cancellation,
    sources: Vec<Source>,
    local_target: Option<LocalTarget>,
    project_operation: ProjectOperation,
    configuration: PaletteConfig,
    keymap: crate::keymap::Keymap,
    supports_clear: bool,
    supports_edit_scrollback: bool,
    _subscription: Subscription,
}

/// Metadata snapshots, not surface updates, invalidate the prepared entries.
struct Source {
    endpoint: String,
    generation: u64,
    enabled: bool,
    snapshot: Option<std::sync::Arc<ClientShellSnapshot>>,
}

#[derive(Clone)]
struct LocalTarget {
    generation: u64,
    boot: String,
}

#[derive(Default)]
enum ProjectOperation {
    #[default]
    Idle,
    Validating {
        _task: Task<()>,
    },
    Awaiting(String),
}

impl Palette {
    fn filter(&mut self, query: &str) {
        self.query = query.to_owned();
        self.refilter(None);
    }

    fn selected_identity(&self) -> Option<Identity> {
        self.filtered
            .get(self.selected)
            .map(|index| self.entries[*index].action.identity())
    }

    /// The status filter in effect, which only Navigation applies.
    fn active_agent_filter(&self) -> AgentFilter {
        if self.filter == Filter::Navigation {
            self.agent_filter
        } else {
            AgentFilter::All
        }
    }

    fn set_agent_filter(&mut self, agent_filter: AgentFilter) {
        self.filter = Filter::Navigation;
        self.agent_filter = agent_filter;
        self.refilter(None);
    }

    fn refilter(&mut self, selected: Option<Identity>) {
        let query = self.query.to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        let agent_filter = self.active_agent_filter();
        let mut ranked: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.filter.accepts(&entry.action))
            .filter(|(_, entry)| agent_filter.accepts(entry.status))
            .filter_map(|(index, entry)| entry.fields.score(&terms).map(|score| (index, score)))
            .collect();
        if agent_filter != AgentFilter::All {
            // As in Herdr's navigator, a workspace stays as the heading of its
            // matching agents and ranks with the best of them.
            let mut parents = std::collections::BTreeMap::new();
            for (index, score) in &ranked {
                if let Some(parent) = self.entries[*index].parent {
                    let best = parents.entry(parent).or_insert(*score);
                    *best = (*best).max(*score);
                }
            }
            ranked.extend(parents.into_iter().map(|(parent, score)| {
                let own = self.entries[parent].fields.score(&terms);
                (parent, own.map_or(score, |own| own.max(score)))
            }));
        }
        // Ties keep list order, so a workspace still precedes its panes.
        ranked.sort_by(|(i, a), (j, b)| b.cmp(a).then(i.cmp(j)));
        self.filtered = ranked.into_iter().map(|(index, _)| index).collect();
        self.selected = selected
            .and_then(|selected| {
                self.filtered
                    .iter()
                    .position(|index| self.entries[*index].action.identity() == selected)
            })
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
    }

    fn busy(&self) -> bool {
        !matches!(self.project_operation, ProjectOperation::Idle)
    }
}

impl HerdrWindow {
    pub(super) fn open_palette(
        &mut self,
        filter: Filter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::Palette);
        let search = cx.new(SearchInput::new);
        let subscription = cx.subscribe(&search, |this, search, _: &Changed, cx| {
            if let Some(palette) = &mut this.menu.palette {
                palette.filter(search.read(cx).text());
                cx.notify();
            }
        });
        let target = self
            .live
            .snapshot
            .as_ref()
            .map(|snapshot| Target::capture(snapshot));
        let local_target = self
            .endpoints
            .iter()
            .position(|endpoint| endpoint.id == crate::endpoint::LOCAL)
            .and_then(|index| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &self.endpoints[index].live
                };
                live.snapshot.as_ref().map(|snapshot| LocalTarget {
                    generation: self.endpoints[index].generation,
                    boot: snapshot.boot_id.clone(),
                })
            });
        search.update(cx, |input, cx| {
            input.set_placeholder("Search workspaces, commands, agents, and projects...", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let mut palette = Palette {
            search,
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            target,
            filter,
            agent_filter: AgentFilter::All,
            query: String::new(),
            error: None,
            projects: projects::Collection::default(),
            loading_projects: !self.config.palette.project_roots.is_empty(),
            project_task: None,
            _scan: projects::Cancellation::default(),
            sources: Vec::new(),
            local_target,
            project_operation: ProjectOperation::Idle,
            configuration: self.config.palette.clone(),
            keymap: self.keymap().clone(),
            supports_clear: self.live.supports_pane_clear,
            supports_edit_scrollback: self.live.supports_edit_scrollback,
            _subscription: subscription,
        };
        self.prepare_palette_entries(&mut palette);
        self.menu.palette = Some(palette);
        self.load_palette_projects(window, cx);
        cx.notify();
    }

    fn palette_sources(&self) -> Vec<Source> {
        std::iter::once(self.selected_endpoint)
            .chain((0..self.endpoints.len()).filter(|index| *index != self.selected_endpoint))
            .map(|index| {
                let endpoint = &self.endpoints[index];
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                Source {
                    endpoint: endpoint.id.clone(),
                    generation: endpoint.generation,
                    enabled: endpoint.enabled && live.status.is_connected(),
                    snapshot: live.snapshot.clone(),
                }
            })
            .collect()
    }

    fn prepare_palette_entries(&self, palette: &mut Palette) {
        let selected = palette.selected_identity();
        let mut entries = Vec::new();
        let sources = self.palette_sources();
        for source in &sources {
            let Some(snapshot) = source.snapshot.as_ref().filter(|_| source.enabled) else {
                continue;
            };
            let host = self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.id == source.endpoint)
                .map(|endpoint| endpoint.label.as_str());
            go_to_entries(&source.endpoint, host, snapshot, &mut entries);
        }
        entries.extend(
            COMMANDS
                .iter()
                .filter(|info| info.command != Command::Palette)
                .filter(|info| match info.command {
                    Command::ClearPane => self.live.supports_pane_clear,
                    Command::EditScrollback => self.live.supports_edit_scrollback,
                    _ => true,
                })
                .map(|info| {
                    Entry::new(
                        info.label.into(),
                        self.keymap().primary(info.command).into(),
                        "GUI action",
                        Action::Native(info.command),
                        None,
                    )
                }),
        );
        if let Some(snapshot) = self
            .live
            .snapshot
            .as_ref()
            .filter(|_| self.live.status.is_connected())
        {
            entries.extend(snapshot.commands.iter().map(|command| {
                let bindings = self.keymap().custom_labels(command);
                let host = &self.endpoints[self.selected_endpoint].label;
                let workspace = palette
                    .target
                    .as_ref()
                    .and_then(|target| target.workspace.as_deref())
                    .and_then(|id| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.workspace_id == id)
                    })
                    .map(|workspace| workspace.label.as_str());
                let context = [Some(host.as_str()), workspace]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("  ");
                let detail = if bindings.is_empty() {
                    context
                } else {
                    format!("{context}  {}", bindings.join(", "))
                };
                Entry::new(
                    command
                        .description
                        .as_ref()
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or(&command.command_id)
                        .clone(),
                    detail,
                    "Herdr command",
                    Action::Configured(command.command_id.clone(), command.action),
                    None,
                )
            }));
        }
        entries.extend(palette.projects.projects.iter().map(|project| {
            Entry::new(
                project.label.clone(),
                format!("Local  {}", project.path.display()),
                "Project",
                Action::Project(project.clone()),
                None,
            )
        }));
        palette.entries = entries;
        palette.sources = sources;
        palette.keymap = self.keymap().clone();
        palette.supports_clear = self.live.supports_pane_clear;
        palette.supports_edit_scrollback = self.live.supports_edit_scrollback;
        palette.refilter(selected);
    }

    pub(crate) fn refresh_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.update_palette_project(window, cx);
        let Some(palette) = self.menu.palette.as_ref() else {
            return;
        };
        if palette.configuration != self.config.palette {
            if let Some(mut palette) = self.menu.palette.take() {
                palette.project_task = None;
                if matches!(
                    palette.project_operation,
                    ProjectOperation::Validating { .. }
                ) {
                    palette.project_operation = ProjectOperation::Idle;
                }
                palette._scan = projects::Cancellation::default();
                palette.configuration = self.config.palette.clone();
                palette.projects = projects::Collection::default();
                palette.loading_projects = !self.config.palette.project_roots.is_empty();
                self.prepare_palette_entries(&mut palette);
                self.menu.palette = Some(palette);
                self.load_palette_projects(window, cx);
                cx.notify();
            }
            return;
        }
        let sources = self.palette_sources();
        let changed = palette.keymap != *self.keymap()
            || palette.supports_clear != self.live.supports_pane_clear
            || palette.supports_edit_scrollback != self.live.supports_edit_scrollback
            || sources.len() != palette.sources.len()
            || sources.iter().zip(&palette.sources).any(|(a, b)| {
                a.endpoint != b.endpoint
                    || a.generation != b.generation
                    || a.enabled != b.enabled
                    || match (&a.snapshot, &b.snapshot) {
                        (Some(a), Some(b)) => !std::sync::Arc::ptr_eq(a, b),
                        (None, None) => false,
                        _ => true,
                    }
            });
        if changed && let Some(mut palette) = self.menu.palette.take() {
            self.prepare_palette_entries(&mut palette);
            self.menu.palette = Some(palette);
            cx.notify();
        }
    }

    fn activate_palette(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.palette.as_ref().is_none_or(Palette::busy) {
            return;
        }
        if !self.menu_target_current() {
            if let Some(palette) = &mut self.menu.palette {
                palette.error = Some("The selected connection changed. Reopen the palette.".into());
            }
            cx.notify();
            return;
        }
        if let Action::Native(command) = action {
            self.dismiss_menu(window, cx);
            self.command(command, window, cx);
            return;
        }
        if let Action::Project(project) = action {
            self.activate_project(project, window, cx);
            return;
        }
        if let Action::Go {
            endpoint,
            boot,
            target,
        } = &action
        {
            match self.go_to_ready(endpoint, boot, target.as_deref()) {
                Ok(selected) => {
                    self.dismiss_menu(window, cx);
                    if selected {
                        self.navigate(target.as_deref(), cx);
                    } else {
                        self.navigate_endpoint(endpoint, target.as_deref(), cx);
                    }
                }
                Err(error) => {
                    if let Some(palette) = &mut self.menu.palette {
                        palette.error = Some(error.to_string());
                    }
                    cx.notify();
                }
            }
            return;
        }
        let result = (|| {
            if !self.input_ready() {
                return Err(Error::PaletteConnectionNotReady);
            }
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            let target = self
                .menu
                .palette
                .as_ref()
                .and_then(|p| p.target.as_ref())
                .ok_or(Error::NoPaletteSession)?;
            match &action {
                Action::Configured(id, action) => target.invocation(snapshot, id, *action),
                Action::Native(_) | Action::Go { .. } | Action::Project(_) => unreachable!(),
            }
        })();
        match result {
            Ok(params) => {
                self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
                    handle.request(boot, Method::CommandInvoke, params)
                });
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(palette) = &mut self.menu.palette {
                    palette.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    /// Runs a daemon custom command on the focused workspace, tab, and pane,
    /// as choosing it in the palette does; a shortcut has no palette to
    /// report a refusal in, so it reads as a local error instead.
    pub(crate) fn invoke_custom_command(
        &mut self,
        id: &str,
        action: ClientShellCommandAction,
        cx: &mut Context<Self>,
    ) {
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return;
        }
        let result = (|| {
            if !self.input_ready() {
                return Err(Error::PaletteConnectionNotReady);
            }
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            Target::capture(snapshot).invocation(snapshot, id, action)
        })();
        match result {
            Ok(params) => {
                self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
                    handle.request(boot, Method::CommandInvoke, params)
                });
                self.marked.clear();
            }
            Err(error) => self.local_error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Checks a Go To destination against its host's current snapshot, and
    /// reports whether that host is the selected one. Another host is selected
    /// by the navigation itself, which waits for its surface when needed.
    fn go_to_ready(
        &self,
        endpoint: &str,
        boot: &str,
        target: NavigationTarget<&str>,
    ) -> Result<bool> {
        let index = self
            .endpoints
            .iter()
            .position(|e| e.id == endpoint && e.enabled)
            .ok_or(Error::PaletteHostUnavailable)?;
        let selected = index == self.selected_endpoint;
        let live = if selected {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        let snapshot = live
            .snapshot
            .as_ref()
            .ok_or(Error::PaletteHostUnavailable)?;
        destination_exists(snapshot, boot, target)?;
        if selected && !self.input_ready() {
            return Err(Error::PaletteConnectionNotReady);
        }
        Ok(selected)
    }

    pub(super) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Herdr's navigate-mode keys move a Go To list as its arrows do.
        let step = match event.keystroke.key.as_str() {
            "up" => Some(true),
            "down" => Some(false),
            _ => self
                .menu
                .palette
                .as_ref()
                .filter(|palette| palette.filter == Filter::Navigation)
                .and_then(|_| self.keymap().navigates_workspace(&event.keystroke)),
        };
        let Some(palette) = &mut self.menu.palette else {
            return;
        };
        if palette.search.read(cx).is_composing() {
            return;
        }
        // A Go To navigation binding keeps its key; otherwise Alt and a
        // navigator letter filter by agent status without typing the letter.
        if step.is_none()
            && let Some(agent_filter) = AgentFilter::from_keystroke(&event.keystroke)
        {
            cx.stop_propagation();
            window.prevent_default();
            palette.set_agent_filter(agent_filter);
            cx.notify();
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.dismiss_menu(window, cx);
            }
            _ if !palette.filtered.is_empty() && step.is_some() => {
                let up = step == Some(true);
                cx.stop_propagation();
                window.prevent_default();
                let count = palette.filtered.len();
                palette.selected = (palette.selected + if up { count - 1 } else { 1 }) % count;
                palette
                    .scroll
                    .scroll_to_item(palette.selected, ScrollStrategy::Center);
                cx.notify();
            }
            "tab" => {
                cx.stop_propagation();
                window.prevent_default();
                let index = Filter::ALL
                    .iter()
                    .position(|filter| *filter == palette.filter)
                    .unwrap_or(0);
                let step = if event.keystroke.modifiers.shift {
                    Filter::ALL.len() - 1
                } else {
                    1
                };
                palette.filter = Filter::ALL[(index + step) % Filter::ALL.len()];
                palette.refilter(None);
                cx.notify();
            }
            "enter" => {
                cx.stop_propagation();
                window.prevent_default();
                if let Some(index) = palette.filtered.get(palette.selected) {
                    let action = palette.entries[*index].action.clone();
                    self.activate_palette(action, window, cx);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
fn fixture_window(window: &mut Window, cx: &mut Context<HerdrWindow>) -> HerdrWindow {
    let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
    view.live.snapshot = Some(std::sync::Arc::new(
        serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap(),
    ));
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.live.supports_surface = true;
    view.endpoints[0].live = view.live.clone();
    view
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui::TestAppContext;
    use herdr_client::protocol::ClientShellCommand;

    fn snapshot() -> ClientShellSnapshot {
        serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap()
    }

    #[gpui::test]
    fn notification_command_is_searchable_and_targetless_activation_is_inert(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.show_toast_preview(
                    herdr_client::protocol::SemanticNotificationKind::Custom,
                    cx,
                );
                let selected = view.selected_endpoint;
                view.open_palette(Filter::All, window, cx);
                let palette = view.menu.palette.as_mut().unwrap();
                palette.filter("Open Notification Target");
                assert_eq!(palette.filtered.len(), 1);
                let action = palette.entries[palette.filtered[0]].action.clone();
                assert!(matches!(
                    action,
                    Action::Native(Command::OpenNotificationTarget)
                ));
                view.activate_palette(action, window, cx);
                assert!(view.menu.page.is_none());
                assert!(view.pending_navigation.is_none());
                assert_eq!(view.selected_endpoint, selected);
                assert_eq!(view.endpoints[selected].toasts.entries.len(), 1);
            })
        });
        cx.update(|window, cx| {
            crate::bind_keys(cx);
            window.focus(&view.read(cx).focus.clone(), cx);
            window.draw(cx).clear(cx);
            window.dispatch_keystroke(Keystroke::parse("cmd-alt-n").unwrap(), cx);
            assert!(view.read(cx).pending_navigation.is_none());
            assert_eq!(view.read(cx).endpoints[0].toasts.entries.len(), 1);
        });
    }

    #[gpui::test]
    fn command_badges_mark_daemon_commands_and_go_to_badges_mark_agent_status(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.commands = vec![ClientShellCommand {
                    command_id: "build".into(),
                    action: ClientShellCommandAction::Shell,
                    description: None,
                    binding_label: String::new(),
                    binding_labels: Vec::new(),
                }];
            })
        });
        for filter in [Filter::Navigation, Filter::Commands] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.open_palette(filter, window, cx))
            });
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                let palette = view.menu.palette.as_ref().unwrap();
                let entries: Vec<_> = palette
                    .filtered
                    .iter()
                    .map(|index| &palette.entries[*index])
                    .collect();
                assert!(!entries.is_empty());
                for entry in &entries {
                    let expected = match entry.action {
                        Action::Native(_) => "GUI action",
                        Action::Configured(..) => "Herdr command",
                        Action::Go {
                            target: NavigationTarget::Pane(_),
                            ..
                        } => "waiting",
                        Action::Go { .. } => "Workspace",
                        Action::Project(_) => "Project",
                    };
                    assert_eq!(entry.badge, expected, "{}", entry.label);
                }
                assert_eq!(
                    entries
                        .iter()
                        .any(|entry| matches!(entry.action, Action::Go { .. })),
                    filter == Filter::Navigation
                );
            });
            cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
        }
    }

    #[gpui::test]
    fn clear_pane_is_offered_and_sent_only_when_the_daemon_advertises_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        for supported in [false, true] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.live.supports_pane_clear = supported;
                    view.open_palette(Filter::All, window, cx);
                    let offered = view
                        .menu
                        .palette
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .any(|entry| matches!(entry.action, Action::Native(Command::ClearPane)));
                    assert_eq!(offered, supported);
                    view.dismiss_menu(window, cx);
                })
            });
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.supports_pane_clear = false;
                view.local_error = None;
                view.command(Command::ClearPane, window, cx);
                assert!(
                    view.local_error
                        .as_deref()
                        .is_some_and(|error| error.contains("newer Herdr"))
                );
                assert!(view.activation_deadline.is_none(), "nothing was sent");
            })
        });
    }

    #[gpui::test]
    fn go_to_rejects_a_disabled_or_missing_host_and_keeps_the_palette_open(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_palette(Filter::Navigation, window, cx);
                let boot = view.live.snapshot.as_ref().unwrap().boot_id.clone();
                for endpoint in ["missing-host", crate::endpoint::LOCAL] {
                    if endpoint == crate::endpoint::LOCAL {
                        view.endpoints[0].enabled = false;
                    }
                    view.activate_palette(
                        Action::Go {
                            endpoint: endpoint.into(),
                            boot: boot.clone(),
                            target: NavigationTarget::Pane("w1:p1".into()),
                        },
                        window,
                        cx,
                    );
                    let palette = view.menu.palette.as_ref().unwrap();
                    assert_eq!(
                        palette.error.as_deref(),
                        Some(Error::PaletteHostUnavailable.to_string().as_str())
                    );
                    assert!(view.pending_navigation.is_none());
                }
            })
        });
    }

    #[gpui::test]
    fn go_to_lists_the_selected_host_first_and_switches_host_for_a_remote_row(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:box".into(),
                "Box".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(std::sync::Arc::new(snapshot()));
            remote.live.status = crate::state::ConnectionStatus::Connected;
            view.endpoints.push(remote);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_palette(Filter::Navigation, window, cx);
                let palette = view.menu.palette.as_ref().unwrap();
                let entries: Vec<_> = palette
                    .filtered
                    .iter()
                    .map(|index| &palette.entries[*index])
                    .collect();
                let hosts: Vec<_> = entries
                    .iter()
                    .map(|entry| match &entry.action {
                        Action::Go { endpoint, .. } => endpoint.as_str(),
                        _ => panic!("Go To lists only destinations"),
                    })
                    .collect();
                let local = hosts
                    .iter()
                    .take_while(|host| **host == crate::endpoint::LOCAL)
                    .count();
                assert!(local > 0 && local < hosts.len());
                assert!(hosts[local..].iter().all(|host| *host == "ssh:box"));
                assert!(entries[..local].iter().all(|e| !e.detail.contains("Box")));
                assert!(entries[local..].iter().all(|e| e.detail.starts_with("Box")));
                let remote_pane = entries[local..]
                    .iter()
                    .find(|entry| {
                        matches!(
                            entry.action,
                            Action::Go {
                                target: NavigationTarget::Pane(_),
                                ..
                            }
                        )
                    })
                    .unwrap()
                    .action
                    .clone();
                let Action::Go { target, .. } = &remote_pane else {
                    unreachable!()
                };
                let target = target.clone();
                view.activate_palette(remote_pane, window, cx);
                assert!(view.menu.page.is_none(), "a valid row closes the picker");
                assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:box");
                // The remote host has no connection yet, so navigation waits for it.
                assert_eq!(view.pending_navigation, Some(target));
            })
        });
    }

    fn go_to_fixture() -> ClientShellSnapshot {
        let mut snapshot = snapshot();
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = "w1:t2".into();
        tab.label = "logs".into();
        snapshot.tabs.push(tab);
        let mut terminal = snapshot.panes[0].clone();
        terminal.pane_id = "w1:p2".into();
        terminal.tab_id = "w1:t2".into();
        terminal.label = Some("  ".into());
        terminal.foreground_cwd = None;
        terminal.cwd = Some("/repo/logs".into());
        snapshot.panes.push(terminal);
        let mut empty = snapshot.workspaces[0].clone();
        empty.workspace_id = "w2".into();
        empty.number = 2;
        empty.label = "empty".into();
        empty.branch = None;
        snapshot.workspaces.push(empty);
        snapshot
    }

    #[test]
    fn go_to_lists_every_pane_under_its_workspace() {
        let snapshot = go_to_fixture();
        let mut entries = Vec::new();
        go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
        let rows: Vec<_> = entries
            .iter()
            .map(|entry| {
                let Action::Go {
                    endpoint,
                    boot,
                    target,
                } = &entry.action
                else {
                    panic!("Go To lists only destinations");
                };
                assert_eq!(endpoint, crate::endpoint::LOCAL);
                assert_eq!(boot, "boot-v1");
                (
                    entry.label.as_str(),
                    entry.detail.as_str(),
                    entry.badge.as_ref(),
                    target.clone(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "repo",
                    "#1  main  /repo",
                    "Workspace",
                    NavigationTarget::Workspace("w1".into())
                ),
                // The fixture's agent labels its blocked state "waiting".
                (
                    "Claude",
                    "repo  main  /repo",
                    "waiting",
                    NavigationTarget::Pane("w1:p1".into())
                ),
                (
                    "Terminal",
                    "repo  logs  /repo/logs",
                    "Terminal",
                    NavigationTarget::Pane("w1:p2".into())
                ),
                (
                    "empty",
                    "#2",
                    "Workspace",
                    NavigationTarget::Workspace("w2".into())
                ),
            ]
        );
    }

    /// An integration's state label names the agent's status in Go To, as in
    /// the sidebar; a label for another status leaves the daemon's word.
    #[test]
    fn go_to_badges_use_the_agents_state_labels() {
        let badge =
            |labels: &[(&str, &str)]| {
                let mut snapshot = go_to_fixture();
                snapshot.agents[0].state_labels = labels
                    .iter()
                    .map(|(state, label)| ((*state).into(), (*label).into()))
                    .collect();
                let mut entries = Vec::new();
                go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
                entries
                .iter()
                .find(|entry| matches!(
                    &entry.action,
                    Action::Go { target: NavigationTarget::Pane(pane), .. } if pane == "w1:p1"
                ))
                .map(|entry| entry.badge.to_string())
            };
        assert_eq!(
            badge(&[("blocked", "needs you"), ("working", "busy")]).as_deref(),
            Some("needs you")
        );
        assert_eq!(badge(&[("working", "busy")]).as_deref(), Some("blocked"));
    }

    #[test]
    fn go_to_names_remote_hosts_and_hides_a_lone_default_tab() {
        let snapshot = snapshot();
        let mut entries = Vec::new();
        go_to_entries("ssh:box", Some("Box"), &snapshot, &mut entries);
        let details: Vec<_> = entries.iter().map(|entry| entry.detail.as_str()).collect();
        assert_eq!(details, ["Box  #1  main  /repo", "Box  repo  /repo"]);
        assert!(entries.iter().all(|entry| matches!(
            &entry.action,
            Action::Go { endpoint, .. } if endpoint == "ssh:box"
        )));
        let mut matching = entries
            .iter()
            .filter(|entry| {
                matches_query(
                    &format!("{} {} {}", entry.label, entry.detail, entry.badge),
                    "box claude",
                )
            })
            .map(|entry| entry.label.as_str());
        assert_eq!(matching.next(), Some("Claude"));
        assert_eq!(matching.next(), None);
    }

    #[test]
    fn go_to_panes_indent_only_beneath_a_visible_workspace() {
        let snapshot = go_to_fixture();
        let mut entries = Vec::new();
        go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
        let nested = |filtered: &[usize]| {
            filtered
                .iter()
                .map(|index| is_nested(&entries[*index], filtered))
                .collect::<Vec<_>>()
        };
        assert_eq!(nested(&[0, 1, 2, 3]), [false, true, true, false]);
        // A search that matches a pane but not its workspace leaves it flush.
        assert_eq!(nested(&[1, 3]), [false, false]);
    }

    #[test]
    fn go_to_destinations_are_revalidated_against_the_current_snapshot() {
        let snapshot = go_to_fixture();
        for target in [
            NavigationTarget::Workspace("w2"),
            NavigationTarget::Tab("w1:t2"),
            NavigationTarget::Pane("w1:p2"),
        ] {
            assert!(destination_exists(&snapshot, "boot-v1", target.clone()).is_ok());
            assert!(matches!(
                destination_exists(&snapshot, "boot-v2", target.clone()),
                Err(Error::PaletteSessionChanged)
            ));
            assert!(matches!(
                destination_exists(&snapshot, "", target),
                Err(Error::PaletteSessionChanged)
            ));
        }
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Workspace("gone")),
            Err(Error::PaletteWorkspaceRemoved)
        ));
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Tab("gone")),
            Err(Error::PaletteTabRemoved)
        ));
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Pane("gone")),
            Err(Error::PaletteDestinationRemoved)
        ));
    }

    #[test]
    fn filtering_matches_all_unicode_tokens_in_any_order() {
        assert!(matches_query("CAF\u{c9} branch 42", " 42\tCAF\u{e9} "));
        assert!(matches_query("\u{391}\u{392} workspace", "\u{3b1}\u{3b2}"));
        assert!(matches_query("anything", " \n "));
        assert!(!matches_query("CAF\u{c9} branch 42", "caf\u{e9} missing"));
    }

    #[test]
    fn invocation_uses_captured_ids_not_current_focus() {
        let mut snapshot = snapshot();
        snapshot.commands = vec![ClientShellCommand {
            command_id: "build".into(),
            action: ClientShellCommandAction::Shell,
            description: None,
            binding_label: String::new(),
            binding_labels: Vec::new(),
        }];
        let target = Target::capture(&snapshot);
        snapshot.focused_workspace_id = None;
        snapshot.focused_tab_id = None;
        snapshot.focused_pane_id = None;
        assert_eq!(
            target
                .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                .unwrap(),
            json!({
                "command_id": "build", "workspace_id": target.workspace,
                "tab_id": target.tab, "pane_id": target.pane,
            })
        );
        let empty_target = Target::capture(&snapshot);
        assert_eq!(
            empty_target
                .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                .unwrap(),
            json!({"command_id": "build"})
        );
    }

    #[test]
    fn invocation_rejects_stale_boot_command_action_and_membership() {
        let mut original = snapshot();
        original.commands = vec![ClientShellCommand {
            command_id: "build".into(),
            action: ClientShellCommandAction::Shell,
            description: None,
            binding_label: String::new(),
            binding_labels: Vec::new(),
        }];
        let target = Target::capture(&original);
        for change in 0..7 {
            let mut snapshot = original.clone();
            match change {
                0 => snapshot.boot_id.push_str("-new"),
                1 => snapshot.commands.clear(),
                2 => snapshot.commands[0].action = ClientShellCommandAction::Pane,
                3 => snapshot.workspaces.clear(),
                4 => snapshot.tabs.clear(),
                5 => snapshot.panes.clear(),
                _ => {
                    for pane in &mut snapshot.panes {
                        pane.tab_id = "foreign".into();
                    }
                }
            }
            assert!(
                target
                    .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                    .is_err()
            );
        }
        original.commands[0].action = ClientShellCommandAction::Unknown;
        assert!(
            target
                .invocation(&original, "build", ClientShellCommandAction::Unknown)
                .is_err()
        );
    }

    #[test]
    fn workspace_selection_rejects_removed_id_and_restarted_daemon() {
        let mut snapshot = snapshot();
        let target = Target::capture(&snapshot);
        let id = snapshot.workspaces[0].workspace_id.clone();
        assert!(target.workspace_exists(&snapshot, &id).is_ok());
        assert!(target.workspace_exists(&snapshot, "missing").is_err());
        snapshot.boot_id.push_str("-new");
        assert!(target.workspace_exists(&snapshot, &id).is_err());
    }
}
