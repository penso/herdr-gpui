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
            entries.push(Entry::new(
                name.to_owned(),
                detail,
                agent.map_or(SharedString::new_static("Terminal"), |agent| {
                    crate::sidebar::state_label(agent, status_badge(agent.agent_status))
                        .into_owned()
                        .into()
                }),
                go(NavigationTarget::Pane(pane.pane_id.clone())),
                Some(parent),
            ));
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

    fn refilter(&mut self, selected: Option<Identity>) {
        let query = self.query.to_lowercase();
        let terms: Vec<_> = query.split_whitespace().collect();
        let mut ranked: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.filter.accepts(&entry.action))
            .filter_map(|(index, entry)| entry.fields.score(&terms).map(|score| (index, score)))
            .collect();
        ranked.sort_by(|(_, a), (_, b)| b.cmp(a));
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
mod tests;
