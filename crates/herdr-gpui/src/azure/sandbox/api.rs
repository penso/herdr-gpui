//! Sandbox group (ARM) and sandbox (data plane) requests.

use super::super::sizes::SandboxSize;
use super::super::{
    Error, Result, Settings,
    http::{Audience, Client},
    settings::is_segment,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// The version both planes of the sandboxes preview answer to.
const API_VERSION: &str = "2026-02-01-preview";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long a new role assignment may take to reach the data plane.
const ACCESS_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Marks sandboxes this app created, for anyone browsing the group.
const LABEL: (&str, &str) = ("herdr", "device");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum State {
    Running,
    /// Suspended or stopped: memory and disk kept, no compute charged.
    Stopped,
    /// Creating, resuming, or stopping: wait for it to settle.
    Changing,
    Failed,
    Deleting,
}

impl State {
    fn parse(text: &str) -> Self {
        match text {
            "Running" => Self::Running,
            "Stopped" | "Suspended" => Self::Stopped,
            "Failed" => Self::Failed,
            "Deleting" => Self::Deleting,
            _ => Self::Changing,
        }
    }
}

#[derive(Deserialize)]
struct SandboxResponse {
    #[serde(default)]
    id: String,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    resources: Option<Resources>,
}

#[derive(Deserialize)]
struct Resources {
    #[serde(default)]
    cpu: String,
    #[serde(default)]
    memory: String,
}

#[derive(Deserialize)]
struct GroupResponse {
    #[serde(default)]
    properties: Option<GroupProperties>,
}

#[derive(Deserialize)]
struct GroupProperties {
    #[serde(default, rename = "provisioningState")]
    provisioning_state: Option<String>,
}

/// The group's regional data plane, where its sandboxes are reached.
pub(super) fn endpoint(settings: &Settings) -> String {
    format!(
        "https://management.{}.azuredevcompute.io",
        settings.sandbox_location
    )
}

fn group_path(settings: &Settings) -> String {
    format!(
        "{}/subscriptions/{}/resourceGroups/{}/sandboxGroups/{}",
        endpoint(settings),
        settings.subscription_id,
        settings.resource_group,
        settings.sandbox_group
    )
}

fn sandbox_path(settings: &Settings, id: &str) -> Result<String> {
    if !is_segment(id) {
        return Err(Error::Field("Azure sandbox ID"));
    }
    Ok(format!("{}/sandboxes/{id}", group_path(settings)))
}

/// The exec stream of one sandbox, as a WebSocket URL.
pub(super) fn exec_stream_url(settings: &Settings, id: &str) -> Result<String> {
    let path = sandbox_path(settings, id)?;
    let path = path
        .strip_prefix("https://")
        .ok_or(Error::Field("Azure sandbox endpoint"))?;
    Ok(format!("wss://{path}/exec/stream"))
}

/// Create the resource group and sandbox group if they are missing, wait
/// until the group can hold sandboxes, and grant this identity its data
/// plane. Returns whether the grant went through; an identity that may not
/// assign roles can still hold the role already.
pub(super) fn ensure_group(settings: &Settings, cancelled: &dyn Fn() -> bool) -> Result<bool> {
    let client = Client::for_audience(settings, Audience::ResourceManager)?;
    super::super::api::ensure_resource_group(
        &client,
        &settings.resource_group,
        &settings.sandbox_location,
    )?;
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.App/sandboxGroups/{}?api-version={API_VERSION}",
        settings.subscription_id, settings.resource_group, settings.sandbox_group
    );
    let existing: Result<GroupResponse> = client.get(&path);
    if !matches!(existing, Ok(ref group) if provisioned(group)) {
        let body = serde_json::json!({ "location": settings.sandbox_location });
        let _: serde_json::Value = client.put(&path, &body)?;
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let group: GroupResponse = client.get(&path)?;
        match group
            .properties
            .and_then(|properties| properties.provisioning_state)
            .as_deref()
        {
            None | Some("Succeeded") => break,
            Some(state @ ("Failed" | "Canceled")) => {
                return Err(Error::Sandbox(format!("the sandbox group is {state}")));
            }
            Some(_) if Instant::now() >= deadline => return Err(Error::SandboxTimeout),
            Some(_) => wait(cancelled)?,
        }
    }
    match super::access::grant(&client, settings) {
        Ok(()) => Ok(true),
        Err(Error::Forbidden(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

fn provisioned(group: &GroupResponse) -> bool {
    group
        .properties
        .as_ref()
        .and_then(|properties| properties.provisioning_state.as_deref())
        .is_none_or(|state| state == "Succeeded")
}

/// Ask for a new sandbox and return its ID; it starts in the background.
/// With `granted`, refusals are retried while the new role assignment
/// reaches the data plane, which it does unevenly: a create can pass while
/// the next read is still refused.
pub(super) fn create(
    client: &Client,
    settings: &Settings,
    granted: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<String> {
    let deadline = Instant::now() + ACCESS_TIMEOUT;
    let retry = |refused| granted && refused && Instant::now() < deadline;
    let id = loop {
        let result = create_once(client, settings);
        if !retry(matches!(result, Err(Error::Forbidden(_)))) {
            break result?;
        }
        wait(cancelled)?;
    };
    loop {
        let result = state(client, settings, &id);
        if !retry(matches!(result, Err(Error::Forbidden(_)))) {
            result?;
            return Ok(id);
        }
        wait(cancelled)?;
    }
}

fn create_once(client: &Client, settings: &Settings) -> Result<String> {
    let lifecycle = match settings.auto_stop {
        Some(idle) => serde_json::json!({
            "autoSuspendPolicy": { "enabled": true, "interval": idle.as_secs(), "mode": "Memory" }
        }),
        None => serde_json::json!({ "autoSuspendPolicy": { "enabled": false } }),
    };
    let body = serde_json::json!({
        "sourcesRef": { "diskImage": { "name": settings.sandbox_image, "isPublic": true } },
        "resources": {
            "cpu": settings.sandbox_size.cpu(),
            "memory": settings.sandbox_size.memory(),
        },
        "lifecycle": lifecycle,
        "labels": { LABEL.0: LABEL.1 },
    });
    let path = format!(
        "{}/sandboxes?api-version={API_VERSION}",
        group_path(settings)
    );
    let sandbox: SandboxResponse = client.put(&path, &body)?;
    if !is_segment(&sandbox.id) {
        return Err(Error::Sandbox("Azure returned no usable sandbox ID".into()));
    }
    Ok(sandbox.id)
}

/// The size of each sandbox in the group, by ID.
pub(super) fn sizes(client: &Client, settings: &Settings) -> Result<HashMap<String, SandboxSize>> {
    let path = format!(
        "{}/sandboxes?api-version={API_VERSION}",
        group_path(settings)
    );
    let list: Vec<SandboxResponse> = client.get(&path)?;
    Ok(list
        .into_iter()
        .filter_map(|sandbox| {
            let resources = sandbox.resources?;
            let size = SandboxSize::reported(&resources.cpu, &resources.memory)?;
            Some((sandbox.id, size))
        })
        .collect())
}

/// Whether the sandbox runs now, without resuming it.
pub(super) fn running(client: &Client, settings: &Settings, id: &str) -> Result<bool> {
    Ok(matches!(state(client, settings, id)?, State::Running))
}

/// Whether the group still has the sandbox.
pub(super) fn exists(client: &Client, settings: &Settings, id: &str) -> Result<bool> {
    match state(client, settings, id) {
        Ok(_) => Ok(true),
        Err(Error::SandboxDeleted) => Ok(false),
        Err(error) => Err(error),
    }
}

fn state(client: &Client, settings: &Settings, id: &str) -> Result<State> {
    let path = format!("{}?api-version={API_VERSION}", sandbox_path(settings, id)?);
    match client.get::<SandboxResponse>(&path) {
        Ok(sandbox) => Ok(sandbox
            .state
            .as_deref()
            .map_or(State::Changing, State::parse)),
        Err(Error::Status(404, _)) => Err(Error::SandboxDeleted),
        Err(error) => Err(error),
    }
}

fn resume(client: &Client, settings: &Settings, id: &str) -> Result<()> {
    let path = format!(
        "{}/resume?api-version={API_VERSION}",
        sandbox_path(settings, id)?
    );
    let _: serde_json::Value = client.post_empty(&path)?;
    Ok(())
}

/// Delete the sandbox, memory and disk with it, and wait until the data
/// plane no longer has it.
pub(super) fn delete(
    client: &Client,
    settings: &Settings,
    id: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let path = format!("{}?api-version={API_VERSION}", sandbox_path(settings, id)?);
    match client.delete::<serde_json::Value>(&path) {
        Ok(_) | Err(Error::Status(404, _)) => {}
        Err(error) => return Err(error),
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match state(client, settings, id) {
            Err(Error::SandboxDeleted) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(Error::DeleteTimeout);
        }
        wait(cancelled)?;
    }
}

/// Resume the sandbox if it is suspended, and wait until it runs.
pub(super) fn wait_running(
    client: &Client,
    settings: &Settings,
    id: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    let mut resumed = false;
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        match state(client, settings, id)? {
            State::Running => return Ok(()),
            State::Failed => return Err(Error::Sandbox("the sandbox failed".into())),
            State::Deleting => return Err(Error::SandboxDeleted),
            State::Stopped if !resumed => {
                resume(client, settings, id)?;
                resumed = true;
            }
            State::Stopped | State::Changing => {}
        }
        if Instant::now() >= deadline {
            return Err(Error::SandboxTimeout);
        }
        wait(cancelled)?;
    }
}

fn wait(cancelled: &dyn Fn() -> bool) -> Result<()> {
    let wake = Instant::now() + POLL_INTERVAL;
    while Instant::now() < wake {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn parse_state(text: &str) -> State {
    State::parse(text)
}
