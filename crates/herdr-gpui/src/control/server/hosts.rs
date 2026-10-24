//! `pr`, `host`, `cloud add`, `ports`, `usage`, `system`, `security`, and
//! `automation approve`: what the
//! window gathers about its hosts, read from the state it already keeps.
//! Nothing here starts a lookup, a scan, or a sample of its own; a request
//! answers from the latest one, and says why when there is none.
mod automation;
mod ports;
mod pr;
mod security;

#[cfg(test)]
mod tests;

use super::{Watches, in_free_window, in_window, socket, target, window_for};
use crate::{
    HerdrWindow,
    browser::PageOpened,
    control::protocol::{
        Call, Caller, ErrorCode, Response,
        hosts::{HostOp, LoadInfo, Reply, UsageInfo, UsageWindow},
    },
};
use gpui::App;
use herdr_client::protocol::{ClientShellSnapshot, ClientShellWorkspace};
use std::time::SystemTime;

pub(super) fn serve(
    call: Call<HostOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let caller = &call.caller;
    let response = match call.op {
        HostOp::List => in_window(caller, cx, |view, _, _| Response::Host {
            reply: Reply::Hosts {
                hosts: view.control_hosts(),
            },
        }),
        HostOp::Select { id } => in_free_window(caller, cx, |view, _, cx| {
            select(view, &id, |view, id| view.select_endpoint(id, cx))
        }),
        HostOp::Pr => in_window(caller, cx, |view, _, _| pr::status(view, caller)),
        HostOp::PrComments => return pr::comments(call.caller, incoming, watches, cx),
        HostOp::PrOpen => in_window(caller, cx, |view, window, cx| {
            pr::open(view, caller, window, cx)
        }),
        HostOp::Ports => in_window(caller, cx, |view, _, _| ports::list(view, caller)),
        HostOp::Forward { port } => in_window(caller, cx, |view, _, cx| {
            ports::forward(view, caller, port, cx)
        }),
        HostOp::Unforward { port } => in_window(caller, cx, |view, _, cx| {
            ports::unforward(view, caller, port, cx)
        }),
        HostOp::OpenPort { port } => return ports::open(call.caller, port, incoming, watches, cx),
        HostOp::Usage => in_window(caller, cx, |view, _, _| usage(view, SystemTime::now())),
        HostOp::System => in_window(caller, cx, |view, _, _| system(view)),
        HostOp::Security => in_window(caller, cx, |view, _, _| security::alerts(view)),
        HostOp::SecurityOpen { repository, item } => in_window(caller, cx, |view, window, cx| {
            security::open(view, &repository, &item, window, cx)
        }),
        HostOp::AutomationApprove { path, host } => in_window(caller, cx, |view, window, cx| {
            automation::approve(view, caller, &path, host.as_deref(), window, cx)
        }),
        HostOp::Cloud => cloud(caller, cx),
        HostOp::CloudSetup { id } => {
            in_free_window(caller, cx, |view, _, cx| cloud_setup(view, &id, cx))
        }
        HostOp::CloudImages { provider } => in_window(caller, cx, |view, _, cx| {
            cloud_images(view, caller, provider.as_deref(), cx)
        }),
    };
    incoming.respond(response);
}

/// The endpoint and workspace a request is about: the caller's own workspace
/// when this window shows it, else the window's focused workspace on its
/// selected host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Place {
    pub index: usize,
    pub endpoint: String,
    pub label: String,
    pub workspace: String,
}

impl Place {
    pub(super) fn of(view: &HerdrWindow, caller: &Caller) -> Result<Self, Response> {
        let target = target(caller);
        let (index, workspace) = target
            .strictness()
            .iter()
            .find_map(|strict| view.control_target(&target, *strict))
            .ok_or_else(|| {
                Response::error(
                    ErrorCode::WorkspaceNotFound,
                    match &caller.workspace_id {
                        Some(id) => format!("This window does not show workspace {id}"),
                        None => "The window shows no workspace".into(),
                    },
                )
            })?;
        let endpoint = &view.endpoints[index];
        Ok(Self {
            index,
            endpoint: endpoint.id.clone(),
            label: endpoint.label.clone(),
            workspace,
        })
    }

    pub(super) fn selected(&self, view: &HerdrWindow) -> bool {
        self.index == view.selected_endpoint
    }

    fn snapshot<'a>(&self, view: &'a HerdrWindow) -> Option<&'a ClientShellSnapshot> {
        let live = if self.selected(view) {
            &view.live
        } else {
            &view.endpoints.get(self.index)?.live
        };
        live.snapshot.as_deref()
    }

    pub(super) fn workspace<'a>(&self, view: &'a HerdrWindow) -> Option<&'a ClientShellWorkspace> {
        self.snapshot(view)?
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == self.workspace)
    }

    pub(super) fn focused(&self, view: &HerdrWindow) -> bool {
        self.selected(view)
            && self
                .snapshot(view)
                .and_then(|snapshot| snapshot.focused_workspace_id.as_deref())
                == Some(self.workspace.as_str())
    }
}

/// Shows the host named by ID or label. `choose` is the window's own
/// selection, as clicking the host in the sidebar does.
fn select(
    view: &mut HerdrWindow,
    name: &str,
    choose: impl FnOnce(&mut HerdrWindow, &str) -> bool,
) -> Response {
    let found = view
        .endpoints
        .iter()
        .find(|endpoint| endpoint.id == name)
        .or_else(|| {
            view.endpoints
                .iter()
                .find(|endpoint| endpoint.label.eq_ignore_ascii_case(name))
        });
    let Some(endpoint) = found else {
        return Response::error(
            ErrorCode::NotFound,
            format!("No host is named {name}; `herdr-gpui host list` lists them"),
        );
    };
    let (id, label) = (endpoint.id.clone(), endpoint.label.clone());
    if !endpoint.enabled {
        return Response::error(
            ErrorCode::InvalidRequest,
            format!("Host {label} is turned off; turn it on in the sidebar first"),
        );
    }
    let before = view.local_error.clone();
    if choose(view, &id) {
        return Response::done(format!("Showing {label}"));
    }
    let reason = view
        .local_error
        .clone()
        .filter(|error| Some(error) != before.as_ref())
        .unwrap_or_else(|| format!("Herdr UI could not switch to {label}"));
    Response::error(ErrorCode::Failed, reason)
}

fn selected_label(view: &HerdrWindow) -> String {
    view.endpoints
        .get(view.selected_endpoint)
        .map(|endpoint| endpoint.label.clone())
        .unwrap_or_default()
}

/// Plan usage on the selected host, as the status bar shows it.
fn usage(view: &HerdrWindow, now: SystemTime) -> Response {
    let host = selected_label(view);
    let entry = view.usage.current();
    let usage: Vec<UsageInfo> = entry
        .map(|entry| {
            entry
                .readings
                .iter()
                .map(|reading| usage_info(reading, now))
                .collect()
        })
        .unwrap_or_default();
    let note = if !view.config.usage.show {
        Some("Usage is turned off ([usage] show in the settings)".to_owned())
    } else if let Some(error) = entry.and_then(|entry| entry.error.clone()) {
        Some(error)
    } else if entry.is_none() {
        Some(format!("Herdr UI has not read usage on {host} yet"))
    } else if usage.is_empty() {
        Some(format!("No AI plan is signed in on {host}"))
    } else {
        None
    };
    Response::Host {
        reply: Reply::Usage { host, usage, note },
    }
}

pub(super) fn usage_info(reading: &crate::usage::Reading, now: SystemTime) -> UsageInfo {
    let report = reading.report.as_ref();
    UsageInfo {
        provider: reading.provider.name().to_owned(),
        account: report.and_then(|report| report.account.email.clone()),
        plan: report.and_then(|report| report.account.plan.clone()),
        windows: report
            .map(|report| {
                report
                    .windows
                    .iter()
                    .map(|window| UsageWindow {
                        label: window.kind.title().to_owned(),
                        used_percent: window.used,
                        resets_in_seconds: window.resets_in(now).map(|left| left.as_secs()),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        error: reading.error.clone(),
    }
}

/// The selected host's latest load sample.
fn system(view: &HerdrWindow) -> Response {
    let label = selected_label(view);
    let host = view
        .endpoints
        .get(view.selected_endpoint)
        .and_then(|endpoint| crate::usage::Host::load_of(&endpoint.connection.target));
    let Some(host) = host else {
        return Response::Host {
            reply: Reply::System {
                load: None,
                note: Some(format!("Load is not sampled on {label}, a cloud machine")),
            },
        };
    };
    let reading = view.system_load.get(&host);
    let load = reading
        .and_then(crate::system_load::Reading::latest)
        .map(|sample| Box::new(load_info(&label, sample)));
    let note = match (&load, reading.and_then(crate::system_load::Reading::error)) {
        (_, Some(error)) => Some(error.to_owned()),
        (Some(_), None) => None,
        (None, None) => Some(
            "Herdr UI samples load only while it shows it: in the status bar \
             (show_system_load), the Devices overview, or a host picker"
                .to_owned(),
        ),
    };
    Response::Host {
        reply: Reply::System { load, note },
    }
}

pub(super) fn load_info(host: &str, sample: &crate::system_load::Sample) -> LoadInfo {
    LoadInfo {
        host: host.to_owned(),
        cpu_percent: sample.cpu,
        memory_used: sample.memory.map(|memory| memory.used),
        memory_total: sample.memory.map(|memory| memory.total),
        cores: sample.cores,
        load: sample.load,
        uptime_seconds: sample.uptime,
        disk_available: sample.disk.map(|disk| disk.available),
        disk_total: sample.disk.map(|disk| disk.total),
        gpu_count: sample.gpu.map(|gpu| gpu.count),
        gpu_percent: sample.gpu.and_then(|gpu| gpu.busy),
        gpu_memory_used: sample
            .gpu
            .and_then(|gpu| gpu.memory.map(|memory| memory.used)),
        gpu_memory_total: sample
            .gpu
            .and_then(|gpu| gpu.memory.map(|memory| memory.total)),
    }
}

/// Opens the settings that add a cloud machine. Creating one costs money,
/// so the user chooses and confirms it there; nothing is created here.
#[cfg(feature = "cloud")]
fn cloud(caller: &Caller, cx: &mut App) -> Response {
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => return response,
    };
    let Ok(source) = handle.entity(cx).map(|entity| entity.downgrade()) else {
        return Response::error(ErrorCode::NoWindow, "The window closed");
    };
    crate::settings_window::open_cloud(source, None, cx);
    Response::done("Opened Settings at Cloud devices; add a machine there")
}

#[cfg(not(feature = "cloud"))]
fn cloud(caller: &Caller, cx: &mut App) -> Response {
    let _ = (caller, cx);
    Response::error(ErrorCode::Unsupported, "This build has no cloud providers")
}

/// Runs a cloud device's setup script again. The device is named by its
/// endpoint ID, the machine ID `cloud list` prints, or its label.
#[cfg(feature = "cloud")]
fn cloud_setup(
    view: &mut HerdrWindow,
    name: &str,
    cx: &mut gpui::Context<HerdrWindow>,
) -> Response {
    use crate::cloud_setup::Error;
    let cloud: Vec<_> = view
        .endpoints
        .iter()
        .filter(|endpoint| {
            matches!(
                endpoint.connection.target,
                herdr_client::ConnectTarget::Cloud { .. }
            )
        })
        .collect();
    let found = cloud
        .iter()
        .find(|endpoint| endpoint.id == name)
        .or_else(|| {
            cloud.iter().find(|endpoint| {
                endpoint
                    .id
                    .split_once(':')
                    .is_some_and(|(_, id)| id == name)
            })
        })
        .or_else(|| {
            cloud
                .iter()
                .find(|endpoint| endpoint.label.eq_ignore_ascii_case(name))
        })
        .map(|endpoint| (endpoint.id.clone(), endpoint.label.clone()));
    let Some((id, label)) = found else {
        return Response::error(
            ErrorCode::NotFound,
            format!("No cloud device is named {name}; `herdr-gpui cloud list` lists them"),
        );
    };
    match view.rerun_cloud_setup(&id, cx) {
        Ok(()) => Response::done(format!(
            "Showing {label}; its setup script runs in a new tab once it connects"
        )),
        Err(crate::Error::CloudSetup(Error::NoDevice(_))) => Response::error(
            ErrorCode::InvalidRequest,
            format!("Cloud device {label} is turned off; turn it on in the sidebar first"),
        ),
        Err(error @ crate::Error::CloudSetup(Error::NoScript(_))) => {
            Response::error(ErrorCode::InvalidRequest, error.to_string())
        }
        Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
    }
}

#[cfg(not(feature = "cloud"))]
fn cloud_setup(
    view: &mut HerdrWindow,
    name: &str,
    cx: &mut gpui::Context<HerdrWindow>,
) -> Response {
    let _ = (view, name, cx);
    Response::error(ErrorCode::Unsupported, "This build has no cloud providers")
}

/// Builds the sandbox images of the caller's workspace's repository, as
/// **Build sandbox images** in its menu does.
#[cfg(feature = "cloud")]
fn cloud_images(
    view: &mut HerdrWindow,
    caller: &Caller,
    provider: Option<&str>,
    cx: &mut gpui::Context<HerdrWindow>,
) -> Response {
    let place = match Place::of(view, caller) {
        Ok(place) => place,
        Err(response) => return response,
    };
    let Some(tree) = place
        .workspace(view)
        .and_then(|workspace| workspace.worktree.clone())
    else {
        return Response::error(ErrorCode::NotFound, "The workspace is not a Git checkout");
    };
    let Some(origin) = view.dispatch_origin(&place.workspace, &tree.key, &tree.label) else {
        return Response::error(
            ErrorCode::Unsupported,
            format!("Herdr UI cannot run scripts on {}", place.label),
        );
    };
    let Some(path) = crate::cloud::image::checkout_of(&origin.repo_key).map(str::to_owned) else {
        return Response::error(
            ErrorCode::NotFound,
            "The repository has no checkout to build from",
        );
    };
    let providers: Vec<_> = view
        .sandbox_image_providers()
        .into_iter()
        .filter(|&each| {
            provider.is_none_or(|name| {
                crate::cloud::name(each).eq_ignore_ascii_case(name)
                    || each.key().eq_ignore_ascii_case(name)
            })
        })
        .collect();
    if providers.is_empty() {
        return Response::error(
            ErrorCode::NotFound,
            "No provider here holds sandbox images: add a container or Daytona device first",
        );
    }
    let mut names = Vec::new();
    for each in providers {
        if let Err(error) = view.build_sandbox_image(
            each,
            &origin.place.endpoint_id,
            path.clone(),
            tree.label.clone(),
            cx,
        ) {
            return Response::error(ErrorCode::Failed, error.to_string());
        }
        names.push(crate::cloud::name(each));
    }
    Response::done(format!(
        "Building {}'s images for {}; `herdr-gpui cloud images` shows them once built",
        tree.label,
        names.join(" and ")
    ))
}

#[cfg(not(feature = "cloud"))]
fn cloud_images(
    view: &mut HerdrWindow,
    caller: &Caller,
    provider: Option<&str>,
    cx: &mut gpui::Context<HerdrWindow>,
) -> Response {
    let _ = (view, caller, provider, cx);
    Response::error(ErrorCode::Unsupported, "This build has no cloud providers")
}

/// The answer to a request that opened a page: done when a tab (or, on a
/// build without tabs, the system browser) shows it, refused otherwise.
/// `None` is a page that opens later, through a tunnel still connecting.
pub(super) fn page_answer(how: Option<PageOpened>, done: impl FnOnce() -> String) -> Response {
    match how {
        Some(PageOpened::Tab | PageOpened::SystemBrowser) => Response::done(done()),
        None => Response::done(format!("{}, once its tunnel connects", done())),
        Some(PageOpened::Full) => {
            Response::error(ErrorCode::TabLimit, "Too many browser tabs are open")
        }
        Some(PageOpened::Unreachable) => Response::error(
            ErrorCode::Failed,
            "The window could not go to that workspace",
        ),
    }
}
