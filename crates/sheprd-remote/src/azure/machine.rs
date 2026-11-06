//! What a saved Azure device is. One Azure account offers two kinds of machine,
//! and both live under the same provider in the device catalog, so the saved ID
//! says which: a VM keeps its bare name, which Azure never lets contain `.`,
//! and a Container Apps sandbox is `sandbox.<id>`. The catalog only saves
//! plain identifiers, so the separator cannot be `/`.

const SANDBOX_PREFIX: &str = "sandbox.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine<'a> {
    Vm(&'a str),
    Sandbox(&'a str),
}

impl<'a> Machine<'a> {
    pub fn parse(id: &'a str) -> Self {
        match id.strip_prefix(SANDBOX_PREFIX) {
            Some(sandbox) => Self::Sandbox(sandbox),
            None => Self::Vm(id),
        }
    }

    /// The ID the device catalog saves for a sandbox.
    pub fn sandbox_id(sandbox: &str) -> String {
        format!("{SANDBOX_PREFIX}{sandbox}")
    }

    /// What Settings and the device picker call this kind of machine.
    pub fn noun(self) -> &'static str {
        match self {
            Self::Vm(_) => "VM",
            Self::Sandbox(_) => "sandbox",
        }
    }
}
