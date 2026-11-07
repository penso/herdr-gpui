//! A stand-in for `ssh` that lets the setup and reconnect flows run end to end
//! without a network: [`install_script`] runs any script in `ssh`'s place.
#![allow(clippy::unwrap_used)]

use std::{cell::RefCell, path::PathBuf, process::Command};

thread_local! {
    static SCRIPT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// `sh <script>` while a [`StandIn`] lives on this thread, so no freshly
/// written file is ever executed directly.
pub(super) fn base() -> Option<Command> {
    SCRIPT.with(|script| {
        script.borrow().as_ref().map(|script| {
            let mut command = Command::new("/bin/sh");
            command.arg(script);
            command
        })
    })
}

pub(super) fn state_dir() -> Option<PathBuf> {
    SCRIPT.with(|script| {
        script
            .borrow()
            .as_ref()
            .map(|script| script.with_file_name("state"))
    })
}

pub struct StandIn {
    dir: tempfile::TempDir,
}

/// Route this thread's `ssh` commands to `sh` running `body`, in which `@LOG@`
/// is a file each run may append lines to (see [`StandIn::runs`]) and `@STATE@`
/// a directory that outlives the runs, for a fake service's memory.
pub fn install_script(body: &str) -> StandIn {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("runs");
    let state = dir.path().join("state");
    std::fs::create_dir(&state).unwrap();
    let script = dir.path().join("ssh.sh");
    std::fs::write(
        &script,
        body.replace("@LOG@", &log.display().to_string())
            .replace("@STATE@", &state.display().to_string()),
    )
    .unwrap();
    SCRIPT.with(|slot| *slot.borrow_mut() = Some(script));
    StandIn { dir }
}

impl StandIn {
    /// The lines the runs appended to their log.
    pub fn runs(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("runs"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        SCRIPT.with(|slot| *slot.borrow_mut() = None);
    }
}
