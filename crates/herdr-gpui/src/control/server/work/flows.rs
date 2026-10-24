//! The work flows that end in a dialog of the app's own: restoring a
//! checkpoint, running a worktree script, teleporting, and fanning out. Each
//! opens the flow as the caller's workspace menu would, already pointed at
//! the caller's checkout, and leaves the consequential step to the user:
//! restoring, moving, launching, and trusting a script all wait for them.
use super::{find, repository, selected_workspace};
use crate::{
    HerdrWindow,
    control::protocol::{
        Caller, ErrorCode, Response,
        work::{FanOutRequest, Script},
    },
    dialog_input::DialogInput,
    menu::Page,
    teleport::AgentKind,
    worktree_scripts::{Launch, ScriptKind},
};
use gpui::{App, Context, Point, Window};

impl From<Script> for ScriptKind {
    fn from(script: Script) -> Self {
        match script {
            Script::Setup => Self::Setup,
            Script::Run => Self::Run,
            Script::Archive => Self::Archive,
        }
    }
}

fn invalid(message: impl Into<String>) -> Response {
    Response::error(ErrorCode::InvalidRequest, message)
}

/// Brings the window forward, where the user confirms. Deferred: activating
/// a window updates it, which cannot happen inside this update.
pub(in crate::control::server) fn bring_forward(window: &Window, cx: &mut App) {
    let handle = window.window_handle();
    cx.activate(true);
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    });
}

/// Opens the caller's workspace menu, the way every flow here starts.
fn open_menu(
    view: &mut HerdrWindow,
    caller: &Caller,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Result<String, Response> {
    let workspace = selected_workspace(view, caller)?;
    view.open_workspace_menu(&workspace, Point::default(), window, cx);
    if view.menu.page != Some(Page::Workspace) {
        return Err(Response::error(
            ErrorCode::Failed,
            "The workspace's menu did not open",
        ));
    }
    Ok(workspace)
}

/// What the caller's workspace is checked out on, for messages.
fn branch(view: &HerdrWindow, caller: &Caller) -> String {
    find(view, caller)
        .ok()
        .and_then(|found| found.workspace.branch.clone())
        .unwrap_or_else(|| "the checkout".to_owned())
}

pub(super) fn restore(
    view: &mut HerdrWindow,
    caller: &Caller,
    id: Option<String>,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    if id
        .as_deref()
        .is_some_and(|id| !crate::checkpoint::valid_id(id))
    {
        return invalid("Checkpoint IDs are the digits `herdr-gpui checkpoint list` prints");
    }
    if let Err(response) = open_menu(view, caller, window, cx) {
        return response;
    }
    if view.checkpoint_checkout().is_none() {
        view.dismiss_menu(window, cx);
        return invalid(
            "Checkpoints need a Git checkout on a branch, on this machine or an SSH host",
        );
    }
    let branch = branch(view, caller);
    view.open_checkpoints(window, cx);
    bring_forward(window, cx);
    match id {
        Some(id) => {
            view.checkpoints.want(id.clone());
            Response::done(format!(
                "Opened the checkpoints of {branch}, asking to select {id}; confirm the restore in Herdr UI (the dialog says if {id} is not there)"
            ))
        }
        None => Response::done(format!(
            "Opened the checkpoints of {branch}; choose one and confirm the restore in Herdr UI"
        )),
    }
}

pub(super) fn script(
    view: &mut HerdrWindow,
    caller: &Caller,
    kind: Script,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let kind = ScriptKind::from(kind);
    if kind == ScriptKind::Archive {
        return Response::error(
            ErrorCode::Unsupported,
            "The archive script runs when a worktree is removed; remove it from its workspace menu in Herdr UI",
        );
    }
    let launch = match launch(view, caller, kind) {
        Ok(launch) => launch,
        Err(response) => return response,
    };
    let repo = launch.repo.clone();
    match view.start_worktree_script(launch, cx) {
        Ok(()) => Response::done(format!(
            "Starting the {} script of {repo} in a new tab; Herdr UI asks first about a script it has not seen",
            kind.name()
        )),
        Err(crate::Error::WorktreeScripts(crate::worktree_scripts::Error::Busy)) => {
            Response::error(ErrorCode::Busy, "Another worktree script is still starting")
        }
        Err(error) => Response::error(ErrorCode::Failed, crate::checkpoint::describe(&error)),
    }
}

/// The script job for the caller's workspace, built as its menu builds one.
fn launch(view: &HerdrWindow, caller: &Caller, kind: ScriptKind) -> Result<Launch, Response> {
    selected_workspace(view, caller)?;
    let found = find(view, caller)?;
    let Some(tree) = &found.workspace.worktree else {
        return Err(invalid(crate::worktree_scripts::Error::NotGit.to_string()));
    };
    // The menu offers the setup script on linked worktrees alone.
    if kind == ScriptKind::Setup && !tree.is_linked_worktree {
        return Err(invalid(
            "The setup script runs in linked worktrees, not the main checkout",
        ));
    }
    let endpoint = &view.endpoints[view.selected_endpoint];
    Ok(Launch {
        kind,
        endpoint: (view.selection_epoch, endpoint.generation),
        endpoint_id: endpoint.id.clone(),
        boot: found.snapshot.boot_id.clone(),
        workspace: found.workspace.workspace_id.clone(),
        repo: tree.label.clone(),
        repo_key: tree.key.clone(),
        checkout: None,
        force: false,
        requested: true,
    })
}

/// The endpoint `host` names among `hosts` (ID, label): an exact ID, else
/// one label, ignoring case.
pub(super) fn pick_host<'a>(
    hosts: impl IntoIterator<Item = (&'a str, &'a str)> + Clone,
    host: &str,
) -> Result<(&'a str, &'a str), Response> {
    if let Some(found) = hosts.clone().into_iter().find(|(id, _)| *id == host) {
        return Ok(found);
    }
    let mut labelled = hosts
        .clone()
        .into_iter()
        .filter(|(_, label)| label.eq_ignore_ascii_case(host));
    match (labelled.next(), labelled.next()) {
        (Some(found), None) => Ok(found),
        (Some(_), Some(_)) => Err(invalid(format!(
            "Several hosts are labelled {host}; name one by its ID"
        ))),
        (None, _) => {
            let known: Vec<_> = hosts
                .into_iter()
                .map(|(id, label)| format!("{label} ({id})"))
                .collect();
            Err(super::not_found(format!(
                "No host is named {host}; the window has {}",
                known.join(", ")
            )))
        }
    }
}

pub(super) fn teleport(
    view: &mut HerdrWindow,
    caller: &Caller,
    host: &str,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let hosts: Vec<(&str, &str)> = view
        .endpoints
        .iter()
        .map(|endpoint| (endpoint.id.as_str(), endpoint.label.as_str()))
        .collect();
    let (id, label) = match pick_host(hosts.iter().copied(), host) {
        Ok((id, label)) => (id.to_owned(), label.to_owned()),
        Err(response) => return response,
    };
    if id == view.endpoints[view.selected_endpoint].id {
        return invalid(format!("The worktree is already on {label}"));
    }
    if let Err(response) = open_menu(view, caller, window, cx) {
        return response;
    }
    if !view.can_teleport() {
        view.dismiss_menu(window, cx);
        return invalid(
            "Teleport moves a linked worktree, from Linux or macOS, off a host it can script, while no other move runs",
        );
    }
    let branch = branch(view, caller);
    view.open_teleport(window, cx);
    let Some(teleport) = view
        .teleport
        .as_mut()
        .filter(|teleport| teleport.offers(&id))
    else {
        view.dismiss_menu(window, cx);
        return invalid(format!(
            "Teleport cannot move work to {label}: it is disabled, the same machine, or not reachable by script"
        ));
    };
    teleport.review_host(&id);
    cx.notify();
    bring_forward(window, cx);
    Response::done(format!(
        "Reviewing the move of {branch} to {label}; press Enter in Herdr UI to teleport it"
    ))
}

/// An agent kind by the name Herdr reports, ignoring case.
pub(super) fn agent_kind(name: &str) -> Option<AgentKind> {
    let name = name.to_ascii_lowercase();
    AgentKind::parse(&name).or((name == "antigravity").then_some(AgentKind::Antigravity))
}

/// The agents asked for, merged by kind in the order first named.
pub(super) fn agents(asked: &[(String, u8)]) -> Result<Vec<(AgentKind, usize)>, Response> {
    let mut agents: Vec<(AgentKind, usize)> = Vec::new();
    for (name, count) in asked {
        let Some(kind) = agent_kind(name) else {
            let known: Vec<_> = AgentKind::ALL.iter().map(|kind| kind.name()).collect();
            return Err(invalid(format!(
                "No agent is named {name}; known agents: {}",
                known.join(", ")
            )));
        };
        match agents.iter_mut().find(|(picked, _)| *picked == kind) {
            Some((_, total)) => *total += usize::from(*count),
            None => agents.push((kind, usize::from(*count))),
        }
    }
    let total: usize = agents.iter().map(|(_, count)| count).sum();
    if total > crate::fan_out::MAX_LANES {
        return Err(invalid(format!(
            "A fan-out runs at most {} agents",
            crate::fan_out::MAX_LANES
        )));
    }
    Ok(agents)
}

/// The prompt as the dialog's one-line field holds it.
pub(super) fn prompt_text(prompt: &str) -> String {
    prompt
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|c| !crate::notifications::unsafe_char(*c))
        .collect::<String>()
        .trim()
        .to_owned()
}

pub(super) fn fan_out(
    view: &mut HerdrWindow,
    caller: &Caller,
    request: FanOutRequest,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> Response {
    let agents = match agents(&request.agents) {
        Ok(agents) => agents,
        Err(response) => return response,
    };
    let prompt = prompt_text(&request.prompt);
    if prompt.is_empty() {
        return invalid("Write a prompt");
    }
    if view
        .fan_out
        .as_ref()
        .is_some_and(|fan_out| !fan_out.composing())
    {
        return Response::error(
            ErrorCode::Busy,
            "This window's last fan-out is still open for comparison; keep a lane or close it from the workspace menu first",
        );
    }
    let workspace = match open_menu(view, caller, window, cx) {
        Ok(workspace) => workspace,
        Err(response) => return response,
    };
    if view.fan_out_item().is_none() {
        view.dismiss_menu(window, cx);
        return invalid(
            "Fan-out needs a Git workspace on this machine or an SSH host, from Linux or macOS",
        );
    }
    let repo = find(view, caller)
        .map(|found| repository(found.workspace).to_owned())
        .unwrap_or(workspace);
    view.open_fan_out(window, cx);
    let opened = view.menu.page == Some(Page::FanOut)
        && view
            .fan_out
            .as_ref()
            .is_some_and(|fan_out| fan_out.composing());
    if !opened {
        // As every other refusal here, leave no menu behind.
        view.dismiss_menu(window, cx);
        return Response::error(ErrorCode::Failed, "The fan-out dialog did not open");
    }
    let Some(fan_out) = view.fan_out.as_mut() else {
        return Response::error(ErrorCode::Failed, "The fan-out dialog did not open");
    };
    let picked: usize = agents.iter().map(|(_, count)| count).sum();
    fan_out.want(agents);
    let mut input = DialogInput::new(prompt);
    let end = input.text.len();
    input.selection = end..end;
    view.menu.input = Some(input);
    cx.notify();
    bring_forward(window, cx);
    Response::done(format!(
        "Opened fan-out for {repo} with {picked} agent(s) chosen; start it in Herdr UI"
    ))
}
