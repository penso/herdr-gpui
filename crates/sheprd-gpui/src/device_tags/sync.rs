//! Meeting a device's own daemon, which holds its tags
//! (`sheprd_remote_control::control::tags`). This computer's is one socket
//! away; another machine's is asked over SSH, as `remote::links` enrolls a
//! phone there. Everything here blocks: run it on the background executor.

use super::Error;
use crate::{remote, teleport::Host};
use sheprd_client::{ConnectTarget, ScriptLimits};
use sheprd_remote_control::{app::tags, control::tags::ComputerTags};
use std::{sync::atomic::AtomicBool, time::Duration};

/// What `tags` may print: one small JSON answer.
const OUTPUT: u64 = 16 * 1024;
const IDLE: Duration = Duration::from_secs(20);

/// Finds the program without installing it: tagging a machine is no reason
/// to put a daemon on it, and with no phone paired there no one else reads
/// its tags. An older build refuses `tags`, which prints nothing too. Standard
/// input is drained so the sender is never left writing to no one.
pub(crate) const SCRIPT: &str = "\
program=$(command -v sheprd-remote-control || command -v herdr-remote-control || true)
[ -n \"$program\" ] || program=\"$HOME/.local/bin/sheprd-remote-control\"
if [ -x \"$program\" ]; then \"$program\" tags --offer 2>/dev/null || true; fi
cat >/dev/null
";

/// Where a device's daemon is reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Daemon {
    /// This computer's.
    Local,
    Remote(ConnectTarget),
}

/// Offers `offer` to the device's daemon and returns the tags that stand,
/// or `None` when the device has no daemon that keeps tags.
pub(super) fn exchange(
    daemon: &Daemon,
    offer: ComputerTags,
    cancelled: &AtomicBool,
) -> Result<Option<ComputerTags>, Error> {
    match daemon {
        Daemon::Local => {
            let Ok(state_dir) = remote::state_dir() else {
                return Ok(None);
            };
            Ok(Some(tags::exchange(&state_dir, Some(offer))?))
        }
        Daemon::Remote(target) => {
            let host = Host::new(target).map_err(|_| Error::Unsupported)?;
            let input = serde_json::to_vec(&offer).map_err(Error::Answer)?;
            let mut output = Vec::new();
            host.stream(
                SCRIPT,
                input.as_slice(),
                &mut output,
                ScriptLimits {
                    output: OUTPUT,
                    idle: IDLE,
                },
                cancelled,
            )
            .map_err(Error::Run)?;
            parse(&output)
        }
    }
}

/// The host's answer, bounded; `None` when it printed nothing.
pub(crate) fn parse(output: &[u8]) -> Result<Option<ComputerTags>, Error> {
    if output.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let answer: ComputerTags = serde_json::from_slice(output).map_err(Error::Answer)?;
    Ok(Some(ComputerTags::new(answer.tags, answer.updated_ms)))
}
