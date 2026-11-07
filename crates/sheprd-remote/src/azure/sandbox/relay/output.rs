//! Finds where the remote command's output begins in the terminal's output.
//! The service only feeds stdin to a process on a terminal, so the stream
//! opens with the terminal echoing the relay's setup line; the shell then
//! prints [`START`], and everything after it is the command's stdout. Its
//! stderr is saved apart and read once it exits; see the relay.

use super::super::super::{Error, Result};
use std::io::Write;

/// Printed once the terminal is raw. Everything before it is the terminal
/// echoing the setup line, which spells the marker with escapes rather than
/// these bytes.
pub(super) const START: &[u8] = b"\x01herdr-relay-start\x01";
/// Setup output kept for a diagnostic when the shell never gets started.
const PREAMBLE_LIMIT: usize = 64 * 1024;
const DIAGNOSTIC_LIMIT: usize = 512;

#[derive(Debug, Default)]
pub(in crate::azure::sandbox) struct Output {
    started: bool,
    /// What arrived before [`START`], while waiting for it.
    preamble: Vec<u8>,
}

impl Output {
    /// Route one chunk of terminal output. Returns true when this chunk
    /// carried [`START`]: the shell is now waiting for the remote command.
    pub(in crate::azure::sandbox) fn push(
        &mut self,
        bytes: &[u8],
        stdout: &mut impl Write,
    ) -> Result<bool> {
        if self.started {
            write(stdout, bytes)?;
            return Ok(false);
        }
        self.preamble.extend_from_slice(bytes);
        let Some(at) = find(&self.preamble, START) else {
            let excess = self.preamble.len().saturating_sub(PREAMBLE_LIMIT);
            self.preamble.drain(..excess);
            return Ok(false);
        };
        let rest = self.preamble.split_off(at + START.len());
        self.preamble.clear();
        self.started = true;
        write(stdout, &rest)?;
        Ok(true)
    }

    pub(in crate::azure::sandbox) fn started(&self) -> bool {
        self.started
    }

    /// What the terminal showed before the shell failed to start.
    pub(in crate::azure::sandbox) fn diagnostic(&self) -> String {
        let text = String::from_utf8_lossy(&self.preamble);
        let text = text.trim();
        let skip = text.chars().count().saturating_sub(DIAGNOSTIC_LIMIT);
        text.chars().skip(skip).collect()
    }
}

fn write(stdout: &mut impl Write, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    stdout.write_all(bytes).map_err(Error::Process)?;
    stdout.flush().map_err(Error::Process)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
