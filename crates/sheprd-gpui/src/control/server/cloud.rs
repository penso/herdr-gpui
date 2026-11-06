//! `cloud create`, `cloud exec`, `cloud destroy`, `cloud wait`, and `cloud
//! batches`. A command runs on a saved cloud machine at once, as the agent's
//! own work on the user's machine; creating and destroying ask in the
//! window, which the user answers once for the whole batch, so nothing is
//! created or deleted from here.
use super::{Watches, socket};
use crate::control::protocol::{Call, ErrorCode, Response, cloud::CloudOp};
use gpui::App;

#[cfg(feature = "cloud")]
use {
    super::in_free_window,
    crate::{
        SheprdWindow,
        cloud::batch::{Registry, Target},
        control::protocol::{
            Caller, MAX_WAIT_SECONDS,
            cloud::{BatchInfo, MAX_DEVICES, Reply},
        },
    },
    gpui::{Context, Window},
    sheprd_client::ConnectTarget,
    std::time::{Duration, Instant},
};

#[cfg(feature = "cloud")]
mod create;
#[cfg(feature = "cloud")]
mod exec;
#[cfg(test)]
mod tests;

#[cfg(feature = "cloud")]
pub(super) fn serve(
    call: Call<CloudOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let caller = &call.caller;
    let response = match call.op {
        CloudOp::Create(op) => in_free_window(caller, cx, |view, window, cx| {
            create::ask(view, op, window, cx)
        }),
        CloudOp::Destroy { devices, batch } => in_free_window(caller, cx, |view, window, cx| {
            destroy(view, &devices, batch, window, cx)
        }),
        CloudOp::Wait {
            batch,
            wait_seconds,
        } => return wait(batch, wait_seconds, incoming, watches, cx),
        CloudOp::Batches => {
            let (batches, omitted) = newest(cx.default_global::<Registry>().all());
            Response::Cloud {
                reply: Reply::Batches { batches, omitted },
            }
        }
        CloudOp::Exec {
            device,
            command,
            cwd,
            wait_seconds,
        } => {
            let request = exec::Request {
                device,
                command,
                cwd,
                wait_seconds,
            };
            return exec::serve(caller, request, incoming, watches, cx);
        }
    };
    incoming.respond(response);
}

#[cfg(not(feature = "cloud"))]
pub(super) fn serve(_: Call<CloudOp>, incoming: socket::Incoming, _: &mut Watches, _: &mut App) {
    incoming.respond(Response::error(
        ErrorCode::Unsupported,
        "This build has no cloud providers",
    ));
}

/// The newest of `batches`, oldest first, whose JSON fits in half a
/// message, and how many older ones were left out. Each device keeps a
/// bounded step and error, but 32 batches of 64 devices could still pass
/// the socket bound, which would lose the whole list.
#[cfg(feature = "cloud")]
fn newest(mut batches: Vec<BatchInfo>) -> (Vec<BatchInfo>, usize) {
    let mut room = crate::control::protocol::MAX_MESSAGE / 2;
    let kept = batches
        .iter()
        .rev()
        .take_while(|batch| {
            let size = serde_json::to_vec(batch).map_or(usize::MAX, |json| json.len());
            room.checked_sub(size)
                .inspect(|left| room = *left)
                .is_some()
        })
        .count();
    let omitted = batches.len() - kept;
    batches.drain(..omitted);
    (batches, omitted)
}

#[cfg(feature = "cloud")]
fn answer(batch: BatchInfo) -> Response {
    Response::Cloud {
        reply: Reply::Batch { batch },
    }
}

#[cfg(feature = "cloud")]
fn invalid(message: impl Into<String>) -> Response {
    Response::error(ErrorCode::InvalidRequest, message)
}

/// A saved cloud device the window shows.
#[cfg(feature = "cloud")]
#[derive(Clone, Debug)]
struct Device {
    target: Target,
    /// The provider account it was saved under.
    account: String,
}

#[cfg(feature = "cloud")]
fn devices(view: &SheprdWindow) -> Vec<Device> {
    view.endpoints
        .iter()
        .filter_map(|endpoint| match &endpoint.connection.target {
            ConnectTarget::Cloud {
                provider,
                account,
                id,
                ..
            } => Some(Device {
                target: Target {
                    provider: *provider,
                    id: id.clone(),
                    endpoint: endpoint.id.clone(),
                    label: endpoint.label.clone(),
                },
                account: account.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// The device `name` names: its endpoint ID, its machine ID, or a label no
/// other device has. A shared label is refused rather than guessed.
#[cfg(feature = "cloud")]
fn resolve<'a>(cloud: &'a [Device], name: &str) -> Result<&'a Device, Response> {
    if let Some(device) = cloud
        .iter()
        .find(|device| device.target.endpoint == name)
        .or_else(|| cloud.iter().find(|device| device.target.id == name))
    {
        return Ok(device);
    }
    let mut labelled = cloud
        .iter()
        .filter(|device| device.target.label.eq_ignore_ascii_case(name));
    match (labelled.next(), labelled.next()) {
        (Some(device), None) => Ok(device),
        (Some(_), Some(_)) => Err(invalid(format!(
            "More than one cloud device is named {name}; name it by the ID `sheprd cloud list` prints"
        ))),
        (None, _) => Err(Response::error(
            ErrorCode::NotFound,
            format!("No cloud device is named {name}; `sheprd cloud list` lists them"),
        )),
    }
}

/// Asks in the window to destroy the devices `names` and batch `from` name.
#[cfg(feature = "cloud")]
fn destroy(
    view: &mut SheprdWindow,
    names: &[String],
    from: Option<u64>,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> Response {
    let registry = cx.default_global::<Registry>().clone();
    let targets = match targets(view, names, from, &registry) {
        Ok(targets) => targets,
        Err(response) => return response,
    };
    let Some(id) = view.ask_to_destroy_cloud(&targets, window, cx) else {
        return Response::error(
            ErrorCode::WindowBusy,
            "A dialog is open in the window; close it first",
        );
    };
    super::work::bring_forward(window, cx);
    match registry.get(id) {
        Some(batch) => answer(batch),
        None => Response::error(ErrorCode::Failed, "The batch was not kept"),
    }
}

/// The window's saved cloud devices `names` and batch `from` name, each
/// once, in the order named.
#[cfg(feature = "cloud")]
fn targets(
    view: &SheprdWindow,
    names: &[String],
    from: Option<u64>,
    registry: &Registry,
) -> Result<Vec<Target>, Response> {
    if names.is_empty() && from.is_none() {
        return Err(invalid(
            "Name the devices to destroy, or a batch with --batch",
        ));
    }
    let cloud = devices(view);
    let mut found: Vec<Target> = Vec::new();
    for name in names {
        found.push(resolve(&cloud, name)?.target.clone());
    }
    if let Some(from) = from {
        let Some(batch) = registry.get(from) else {
            return Err(Response::error(
                ErrorCode::NotFound,
                format!("Sheprd keeps no batch {from}; `sheprd cloud batches` lists them"),
            ));
        };
        found.extend(batch.devices.iter().filter_map(|device| {
            cloud
                .iter()
                .find(|saved| saved.target.endpoint == device.endpoint)
                .map(|saved| saved.target.clone())
        }));
    }
    let mut targets: Vec<Target> = Vec::with_capacity(found.len());
    for target in found {
        if !targets.iter().any(|kept| kept.endpoint == target.endpoint) {
            targets.push(target);
        }
    }
    if targets.is_empty() {
        return Err(Response::error(
            ErrorCode::NotFound,
            "None of the batch's devices is saved any more",
        ));
    }
    if targets.len() > MAX_DEVICES {
        return Err(invalid(format!(
            "At most {MAX_DEVICES} devices are destroyed at once"
        )));
    }
    Ok(targets)
}

/// Answers once batch `id` is settled, or with how far it got after
/// `wait_seconds`.
#[cfg(feature = "cloud")]
fn wait(
    id: u64,
    wait_seconds: u64,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let registry = cx.default_global::<Registry>().clone();
    let gone = move || {
        Response::error(
            ErrorCode::NotFound,
            format!("Sheprd keeps no batch {id}; `sheprd cloud batches` lists them"),
        )
    };
    let wait = Duration::from_secs(wait_seconds.min(MAX_WAIT_SECONDS));
    let until = Instant::now() + wait;
    watches.add(
        incoming,
        // The check answers at `until` with the batch as it is; the watch's
        // own expiry, a little later, only covers a check that never ran.
        wait + Duration::from_secs(1),
        move |_| match registry.get(id) {
            None => Some(gone()),
            Some(batch) if batch.phase.settled() || Instant::now() >= until => Some(answer(batch)),
            Some(_) => None,
        },
        Response::error(ErrorCode::Timeout, "The batch did not settle in time"),
        cx,
    );
}
