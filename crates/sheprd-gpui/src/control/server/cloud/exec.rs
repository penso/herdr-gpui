//! `cloud exec`: one command on a saved cloud machine, run on a worker
//! thread while the request is parked. A caller that hangs up, or a request
//! that runs out of time, stops the command and its remote session.
use super::{Caller, Device, Duration, MAX_WAIT_SECONDS, Reply, devices, invalid, resolve};
use crate::{
    cloud::exec::{self, Error, Machine},
    control::{
        protocol::{ErrorCode, Response, cloud::ExecInfo},
        server::{Watches, socket, window_for},
    },
};
use gpui::App;
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicBool, Ordering},
};

/// The most words a command may have.
const MAX_WORDS: usize = 1024;
/// Time beyond the command's own for the answer to come back.
const SLACK: Duration = Duration::from_secs(5);

pub(in crate::control::server) struct Request {
    pub(in crate::control::server) device: String,
    pub(in crate::control::server) command: Vec<String>,
    pub(in crate::control::server) cwd: Option<String>,
    pub(in crate::control::server) wait_seconds: u64,
}

/// A command on its worker. Dropping it, as an answered or abandoned
/// request does, stops the command if it still runs.
struct Running {
    cancel: Arc<AtomicBool>,
    answer: Arc<Mutex<Option<Response>>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Running {
    fn answer(&self) -> Option<Response> {
        self.answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

pub(in crate::control::server) fn serve(
    caller: &Caller,
    request: Request,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let found = window_for(caller, cx).and_then(|handle| {
        handle
            .read_with(cx, |view, _| {
                resolve(&devices(view), &request.device).cloned()
            })
            .unwrap_or_else(|_| Err(Response::error(ErrorCode::NoWindow, "The window closed")))
    });
    let started = found.and_then(|device| start(device, request));
    let (running, wait) = match started {
        Ok(started) => started,
        Err(response) => return incoming.respond(response),
    };
    let seconds = wait.as_secs();
    watches.add(
        incoming,
        wait + SLACK,
        move |_| running.answer(),
        Response::error(
            ErrorCode::Timeout,
            format!("The command did not finish within {seconds} seconds, so it was stopped"),
        ),
        cx,
    );
}

fn start(device: Device, request: Request) -> Result<(Running, Duration), Response> {
    if let Some(reason) = crate::cloud::unavailable() {
        return Err(Response::error(ErrorCode::Unsupported, reason));
    }
    if request.command.is_empty() {
        return Err(invalid("Name the command to run after --"));
    }
    if request.command.len() > MAX_WORDS {
        return Err(invalid(format!("A command has at most {MAX_WORDS} words")));
    }
    let wait = Duration::from_secs(request.wait_seconds.clamp(1, MAX_WAIT_SECONDS));
    let machine = Machine {
        provider: device.target.provider,
        account: device.account,
        id: device.target.id,
    };
    let endpoint = device.target.endpoint;
    let running = Running {
        cancel: Arc::default(),
        answer: Arc::default(),
    };
    let cancel = running.cancel.clone();
    let answer = running.answer.clone();
    tracing::info!(
        category = "cloud_exec",
        endpoint,
        "Running a command on a cloud device"
    );
    std::thread::Builder::new()
        .name("herdr-cloud-exec".into())
        .spawn(move || {
            let result = exec::run(
                &machine,
                &request.command,
                request.cwd.as_deref(),
                wait,
                &cancel,
            );
            let response = respond(endpoint, result);
            *answer.lock().unwrap_or_else(PoisonError::into_inner) = Some(response);
        })
        .map_err(|error| {
            tracing::error!(category = "cloud_exec", %error, "Could not start a command worker");
            Response::error(ErrorCode::Failed, "Sheprd could not start a worker for it")
        })?;
    Ok((running, wait))
}

fn respond(device: String, result: exec::Result<exec::Finished>) -> Response {
    match result {
        Ok(finished) => Response::Cloud {
            reply: Reply::Exec(Box::new(ExecInfo {
                device,
                status: finished.code,
                stdout: finished.stdout,
                stderr: finished.stderr,
                truncated: finished.truncated,
            })),
        },
        Err(error @ Error::Timeout(_)) => Response::error(ErrorCode::Timeout, error.to_string()),
        Err(error @ Error::Empty) => invalid(error.to_string()),
        Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
    }
}

#[cfg(test)]
mod tests;
