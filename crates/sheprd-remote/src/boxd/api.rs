//! The boxd API calls this app makes: create, read, list, wake, and delete a
//! machine. Requests and replies are the few `api.proto` fields in
//! [`wire`](super::wire).

use super::{
    Error, Result,
    error::{Code, Status},
    http::{NOT_FOUND, Request},
    settings::{DEVICE_LABEL, machine_name, machine_reference},
    wire::{Machine, Message},
};
use crate::cloud::Value;
use std::time::{Duration, Instant};

/// How long a machine may take to come back from stopped or hibernated.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);

/// What a machine needs before a command can run on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// Booting, waking, or moving; boxd finishes on its own.
    Busy,
    /// Stopped: `StartVm` cold-boots it.
    Stopped,
    /// In standby: `ResumeVm` brings it back in milliseconds.
    Suspended,
    /// Hibernated: `WakeVm` restores its memory from disk.
    Hibernated,
    Failed,
}

impl From<&str> for Readiness {
    fn from(status: &str) -> Self {
        match status {
            "running" => Self::Ready,
            "stopped" => Self::Stopped,
            "suspended" | "standby" => Self::Suspended,
            "hibernated" => Self::Hibernated,
            "failed" | "destroyed" => Self::Failed,
            // pending, starting, migrating, and anything newer.
            _ => Self::Busy,
        }
    }
}

fn reference(id: &str) -> Result<&str> {
    if machine_reference(id) {
        Ok(id)
    } else {
        Err(Error::Field(Value::BoxdMachine))
    }
}

fn gone(id: &str) -> impl FnOnce(Error) -> Error + '_ {
    move |error| match error {
        Error::Status(Status {
            code: Code::Grpc(NOT_FOUND),
            ..
        }) => Error::Gone(id.to_owned()),
        other => other,
    }
}

impl Request<'_> {
    /// Make the machine `name` with the configured labels and the device
    /// label. boxd's own image is a full Ubuntu with a `boxd` user.
    pub fn create(&self, name: &str) -> Result<Machine> {
        if !machine_name(name) {
            return Err(Error::Field(Value::BoxdMachine));
        }
        let mut request = Message::default().string(1, name);
        for (key, value) in &self.settings.labels {
            request = request.entry(14, key, value);
        }
        request = request.entry(14, DEVICE_LABEL.0, DEVICE_LABEL.1);
        let reply = self.call("create machine", "CreateVm", &request.into_bytes())?;
        Machine::decode_created(&reply)
    }

    /// The machine `id` (or name), or [`Error::Gone`] when it no longer exists.
    pub fn machine(&self, id: &str) -> Result<Machine> {
        let request = Message::default().string(1, reference(id)?).into_bytes();
        self.call("read machine", "GetVm", &request)
            .map_err(gone(id))
            .and_then(|reply| Machine::decode(&reply))
    }

    /// The machines this app made that the key sees.
    pub fn machines(&self) -> Result<Vec<Machine>> {
        let selector = format!("{}={}", DEVICE_LABEL.0, DEVICE_LABEL.1);
        let request = Message::default().string(3, &selector).into_bytes();
        let reply = self.call("list machines", "ListVms", &request)?;
        Machine::decode_list(&reply)
    }

    /// Delete the machine `id`; one already gone is not an error.
    pub fn delete(&self, id: &str) -> Result<()> {
        let request = Message::default().string(1, reference(id)?).into_bytes();
        match self
            .call("delete machine", "DestroyVm", &request)
            .map_err(gone(id))
        {
            Ok(_) | Err(Error::Gone(_)) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn lifecycle(&self, context: &'static str, method: &str, id: &str) -> Result<()> {
        let request = Message::default().string(1, reference(id)?).into_bytes();
        self.call(context, method, &request)
            .map(drop)
            .map_err(gone(id))
    }

    /// Wait until the machine `id` runs, starting, resuming, or waking it
    /// once as its status asks. `report` hears each status that is not ready.
    pub fn wait_ready(
        &self,
        id: &str,
        cancelled: impl Fn() -> bool,
        report: impl Fn(Readiness),
    ) -> Result<Machine> {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut asked = false;
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            let machine = self.machine(id)?;
            let readiness = Readiness::from(machine.status.as_str());
            match readiness {
                Readiness::Ready => return Ok(machine),
                Readiness::Failed => return Err(Error::Failed(machine.name)),
                _ => report(readiness),
            }
            if !asked {
                let call = match readiness {
                    Readiness::Stopped => Some(("start machine", "StartVm")),
                    Readiness::Suspended => Some(("resume machine", "ResumeVm")),
                    Readiness::Hibernated => Some(("wake machine", "WakeVm")),
                    _ => None,
                };
                if let Some((context, method)) = call {
                    self.lifecycle(context, method, id)?;
                    asked = true;
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::NotReady(machine.name));
            }
            std::thread::sleep(POLL);
        }
    }
}
