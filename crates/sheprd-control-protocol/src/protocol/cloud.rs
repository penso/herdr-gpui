//! Cloud machines an agent works with: creating several and destroying them
//! after one confirmation in the window each, running a command on one, and
//! following what each batch did to every machine.
use serde::{Deserialize, Serialize};

/// The most devices one batch holds, as many as the device list keeps.
pub const MAX_DEVICES: usize = 64;
/// The most machines one `cloud create` makes.
pub const MAX_CREATE: u32 = 16;

/// What kind of machine a provider with more than one makes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// The provider's usual machine, such as an Azure VM.
    #[default]
    Machine,
    /// A lighter sandbox, such as an Azure Container Apps session.
    Sandbox,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// New machines `cloud create` asks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateOp {
    /// The provider's key or name, such as `coder`.
    pub provider: String,
    pub count: u32,
    /// Coder: the template by name or ID; absent, the deployment's only one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Coder: the template's preset by name; absent, its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub shape: Shape,
    /// Whether Herdr's installer may run on a machine that lacks Herdr.
    pub install: bool,
    /// Whether the `[cloud]` setup script runs before a machine is ready.
    pub setup: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloudOp {
    /// Ask in the window to create machines. Nothing is created until the
    /// user confirms there.
    Create(CreateOp),
    /// Ask in the window to destroy saved cloud devices: the ones `devices`
    /// names (endpoint ID, machine ID, or label), and the ones batch `batch`
    /// holds. Nothing is deleted until the user confirms there.
    Destroy {
        #[serde(default)]
        devices: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        batch: Option<u64>,
    },
    /// Batch `batch`, once every device in it is settled or `wait_seconds`
    /// pass.
    Wait { batch: u64, wait_seconds: u64 },
    /// The batches the app keeps, oldest first.
    Batches,
    /// Run `command` (a program and its arguments, quoted as they are) on
    /// the saved device `device`, in `cwd` when given, for at most
    /// `wait_seconds` in all.
    Exec {
        device: String,
        command: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        wait_seconds: u64,
    },
}

impl CloudOp {
    pub fn wait_seconds(&self) -> u64 {
        match self {
            Self::Wait { wait_seconds, .. } | Self::Exec { wait_seconds, .. } => *wait_seconds,
            Self::Create(_) | Self::Destroy { .. } | Self::Batches => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchKind {
    Create,
    Destroy,
}

/// Where a batch is: waiting for the user, working, or finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The window asks the user; nothing has started.
    Confirming,
    Running,
    /// Every device is settled, each one well or not.
    Done,
    /// The user said no, or the dialog closed without an answer.
    Cancelled,
}

impl Phase {
    pub fn settled(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    /// As the wire and events name it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Confirming => "confirming",
            Self::Running => "running",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Not started: the batch waits for the user.
    Waiting,
    /// Being made, until it is saved as a device.
    Creating,
    /// Saved, and running the `[cloud]` setup script.
    SettingUp,
    /// Made, saved, and set up: ready for work.
    Ready,
    Destroying,
    /// Deleted at its provider, and forgotten here.
    Destroyed,
    /// Forgotten here only, as its provider cannot delete it from this app.
    Forgotten,
    Failed,
    Cancelled,
}

impl DeviceState {
    pub fn settled(self) -> bool {
        !matches!(
            self,
            Self::Waiting | Self::Creating | Self::SettingUp | Self::Destroying
        )
    }

    /// Settled without what was asked happening.
    pub fn unsuccessful(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The provider's key, such as `coder`.
    pub provider: String,
    /// The machine's ID at the provider.
    pub id: String,
    /// The device's endpoint ID, `provider:id`.
    pub endpoint: String,
    pub label: String,
    pub state: DeviceState,
    /// What it is doing now, for a person, such as "Building (pending)…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// Why it failed, for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchInfo {
    pub id: u64,
    pub kind: BatchKind,
    pub phase: Phase,
    pub devices: Vec<DeviceInfo>,
}

impl BatchInfo {
    /// Whether the batch finished with every device as asked.
    pub fn succeeded(&self) -> bool {
        self.phase == Phase::Done
            && !self
                .devices
                .iter()
                .any(|device| device.state.unsuccessful())
    }
}

/// How a command on a cloud machine ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecInfo {
    /// The device's endpoint ID.
    pub device: String,
    /// The command's exit code; `None` when a signal ended it.
    pub status: Option<i32>,
    /// The tail of each stream, without terminal controls.
    pub stdout: String,
    pub stderr: String,
    /// Whether either stream was longer than the tail kept.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Batch {
        batch: BatchInfo,
    },
    Batches {
        batches: Vec<BatchInfo>,
        /// Older batches left out so the answer stays inside the socket bound.
        #[serde(default, skip_serializing_if = "is_zero")]
        omitted: usize,
    },
    Exec(Box<ExecInfo>),
}

fn is_zero(count: &usize) -> bool {
    *count == 0
}

#[cfg(test)]
mod tests;
