//! The Boat REST calls this app makes: list, read, create, resume, rename, and
//! authorize the app's SSH key. Readiness maps Boat lifecycle states onto what
//! adding or connecting needs to know.

use super::{Error, Result, http::Request};
use crate::cloud::Value;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const START_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL: Duration = Duration::from_secs(3);
const LIST_LIMIT: usize = 100;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Sandbox {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Stopped,
    Pending(String),
    Failed(String),
    Deleted,
}

impl Sandbox {
    pub fn readiness(&self) -> Readiness {
        match self.state.as_str() {
            "ready" | "idle" | "running" => Readiness::Ready,
            "archived" => Readiness::Stopped,
            "error" => Readiness::Failed(
                self.error
                    .as_deref()
                    .map(super::http::diagnostic)
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or_else(|| self.state.clone()),
            ),
            "cancelled" => Readiness::Deleted,
            other => {
                let state = super::http::diagnostic(other).replace('_', " ");
                Readiness::Pending(if state.trim().is_empty() {
                    "unknown".into()
                } else {
                    state.trim().to_owned()
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandboxCount {
    Exact(usize),
    AtLeast(usize),
}

#[derive(Deserialize)]
struct List {
    sandboxes: Vec<Sandbox>,
}

#[derive(Deserialize)]
struct One {
    sandbox: Sandbox,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Create {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(super) machine_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) environment: Option<String>,
    #[serde(rename = "ttlSeconds")]
    pub(super) ttl_seconds: Option<u32>,
}

#[derive(Serialize)]
struct Rename<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct PublicKey<'a> {
    key: &'a str,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshAccess {
    pub machine_ip: Option<String>,
    pub ssh_endpoint: Option<String>,
    pub ssh_user: String,
    pub host_key: String,
}

fn segment(id: &str) -> Result<&str> {
    const ALPHABET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";
    if id
        .strip_prefix("bx_")
        .is_some_and(|value| value.len() == 8 && value.bytes().all(|byte| ALPHABET.contains(&byte)))
    {
        Ok(id)
    } else {
        Err(Error::Field(Value::BoatSandboxId))
    }
}

impl Request<'_> {
    pub fn sandbox_count(&self) -> Result<SandboxCount> {
        let reply: List = self.get("sandboxes", &format!("/sandboxes?limit={LIST_LIMIT}"))?;
        let count = reply.sandboxes.len();
        Ok(if count < LIST_LIMIT {
            SandboxCount::Exact(count)
        } else {
            SandboxCount::AtLeast(count)
        })
    }

    pub fn sandbox(&self, id: &str) -> Result<Sandbox> {
        self.get::<One>("sandbox", &format!("/sandboxes/{}", segment(id)?))
            .and_then(|reply| {
                if reply.sandbox.id != id {
                    return Err(Error::Field(Value::BoatSandboxId));
                }
                Ok(reply.sandbox)
            })
            .map_err(|error| match error {
                Error::Status(super::Status { code: 404, .. }) => Error::Deleted,
                error => error,
            })
    }

    pub(super) fn create(
        &self,
        creation: &super::pending::Creation,
        cancelled: impl Fn() -> bool,
        remember: impl FnOnce(&Sandbox) -> Result<()>,
    ) -> Result<Sandbox> {
        let send = || {
            self.post_idempotent(
                "create_sandbox",
                "/sandboxes",
                &creation.key,
                &creation.payload,
            )
        };
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let mut created: Result<One> = send();
        // A lost reply may still have created a billable sandbox. Every retry
        // keeps the same key, even if cancelled after sending the first request.
        for delay in [500, 1_000, 2_000] {
            if !matches!(
                &created,
                Err(Error::Network(_) | Error::Read(_))
                    | Err(Error::Status(super::Status {
                        code: 409 | 500..=599,
                        ..
                    }))
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(delay));
            created = send();
        }
        let created = created?;
        segment(&created.sandbox.id)?;
        // The caller must record the confirmed resource before even optional
        // HTTP work, so an interrupted rename cannot lose its identity.
        remember(&created.sandbox)?;
        // Boat assigns a default name during create and exposes renaming as a
        // separate operation. If naming alone fails, keep the usable sandbox.
        match self.patch::<One>(
            "rename_sandbox",
            &format!("/sandboxes/{}", segment(&created.sandbox.id)?),
            &Rename {
                name: &creation.name,
            },
        ) {
            Ok(reply) if reply.sandbox.id == created.sandbox.id => Ok(reply.sandbox),
            Ok(_) => {
                tracing::warn!(
                    category = "boat_api",
                    "Boat rename returned a different sandbox ID"
                );
                Ok(created.sandbox)
            }
            Err(error) => {
                tracing::warn!(category = "boat_api", %error, "Boat sandbox could not be renamed");
                Ok(created.sandbox)
            }
        }
    }

    pub fn resume(&self, id: &str) -> Result<()> {
        self.post::<One>(
            "resume_sandbox",
            &format!("/sandboxes/{}/resume", segment(id)?),
            &serde_json::json!({"ttlSeconds": self.settings.ttl_seconds}),
        )
        .map(drop)
    }

    pub fn extend(&self, id: &str) -> Result<()> {
        self.patch::<One>(
            "extend_sandbox",
            &format!("/sandboxes/{}", segment(id)?),
            &serde_json::json!({"ttlSeconds": self.settings.ttl_seconds}),
        )
        .map(drop)
    }

    /// Delete the sandbox and its snapshots for good. Boat accepts it at once
    /// and erases the data in the background; from then on the sandbox is
    /// gone from lists, resumes, and billing. One already gone is deleted.
    pub fn destroy(&self, id: &str) -> Result<()> {
        let id = segment(id)?;
        match self.delete_confirmed("delete_sandbox", &format!("/sandboxes/{id}"), id) {
            Err(Error::Status(super::Status { code: 404, .. })) => Ok(()),
            result => result,
        }
    }

    pub fn ssh_access(&self, id: &str, public_key: &str) -> Result<SshAccess> {
        self.post(
            "ssh_access",
            &format!("/sandboxes/{}/sshkey", segment(id)?),
            &PublicKey { key: public_key },
        )
    }
}

pub fn wait_ready(
    request: &Request,
    id: &str,
    cancelled: impl Fn() -> bool,
    mut progress: impl FnMut(&Readiness),
) -> Result<Sandbox> {
    let deadline = Instant::now() + START_TIMEOUT;
    let mut resumed = false;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let sandbox = request.sandbox(id)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let readiness = sandbox.readiness();
        match &readiness {
            Readiness::Ready => return Ok(sandbox),
            Readiness::Failed(reason) => return Err(Error::Sandbox(reason.clone())),
            Readiness::Deleted => return Err(Error::Deleted),
            Readiness::Stopped if !resumed => {
                resumed = true;
                request.resume(id)?;
            }
            Readiness::Stopped | Readiness::Pending(_) => {}
        }
        progress(&readiness);
        if Instant::now() >= deadline {
            return Err(Error::StartTimeout);
        }
        let wake = Instant::now() + POLL;
        while Instant::now() < wake {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests;
