//! Claiming a bead for a dispatched agent with `bd update --claim`, which is
//! atomic, so two dispatches never both win, and giving it back with
//! `bd unclaim` when the agent never started. These run as `bd` run by hand
//! does, without `--sandbox`: a claim coordinates work, so it reaches a
//! shared Dolt remote when the project has one.

use super::{Output, run, valid_id};
use crate::{Error as CrateError, Result, SourceKey};
use serde::Deserialize;
use sheprd_remote::teleport::Host;
use std::sync::atomic::AtomicBool;

mod error;

pub use error::Error;

const CLAIMING: &str = "Claiming the bead";
const RELEASING: &str = "Releasing the bead";

/// A bead claimed for a dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    id: String,
    /// Who this dispatch made the holder. `None` when the same actor already
    /// held it, as a second agent on a task does: that claim is not this
    /// dispatch's to give back.
    taken_by: Option<String>,
}

/// Claims bead `id` of `source`, or says who holds it. Blocks.
pub fn take(host: &Host, source: &SourceKey, id: &str, cancelled: &AtomicBool) -> Result<Claim> {
    if !valid_id(id) {
        return Err(CrateError::BeadId(id.to_owned()));
    }
    let root = &source.repository;
    let before = run(
        host,
        root,
        &[],
        &["show", "--json", "--", id],
        CLAIMING,
        cancelled,
    )?;
    let held = assignee(&before)?.is_some();
    let after = run(
        host,
        root,
        &[],
        &["update", "--claim", "--json", "--", id],
        CLAIMING,
        cancelled,
    )
    .map_err(|error| refused(error, id))?;
    Ok(Claim {
        id: id.to_owned(),
        taken_by: if held { None } else { assignee(&after)? },
    })
}

/// Gives `claim` back, unless another holder took the bead since. Blocks.
pub fn release(
    host: &Host,
    source: &SourceKey,
    claim: &Claim,
    cancelled: &AtomicBool,
) -> Result<()> {
    let Some(holder) = &claim.taken_by else {
        return Ok(());
    };
    run(
        host,
        &source.repository,
        &[],
        &["unclaim", "--if-assignee", holder, "--", &claim.id],
        RELEASING,
        cancelled,
    )
    .map(drop)
}

/// A refused claim as who holds the bead; any other failure stays as it is.
fn refused(error: CrateError, id: &str) -> CrateError {
    let CrateError::Script {
        source: sheprd_client::Error::ScriptExit { stderr, .. },
        ..
    } = &error
    else {
        return error;
    };
    let id = id.to_owned();
    match holder(stderr) {
        Some(Some(holder)) => Error::Held { id, holder }.into(),
        Some(None) => Error::HeldByUnknown { id }.into(),
        None => error,
    }
}

/// What `bd` said when it refused a claim: `Some` when the bead is held,
/// with the holder when it named one. `bd` words it `already claimed by NAME`
/// for a claim, and `already claimed: already assigned to "NAME"` for an
/// assignment, and has no other mark of it.
pub fn holder(stderr: &str) -> Option<Option<String>> {
    let (_, rest) = stderr
        .lines()
        .find_map(|line| line.split_once("already claimed"))?;
    let named = rest
        .strip_prefix(" by ")
        .or_else(|| rest.split_once("assigned to \"").map(|(_, name)| name))
        .and_then(|name| name.split('"').next())
        .map(str::trim)
        .filter(|name| !name.is_empty());
    Some(named.map(str::to_owned))
}

#[derive(Deserialize)]
struct Holding {
    assignee: Option<String>,
}

/// The assignee in `bd`'s `--json` output for one bead.
fn assignee(output: &[u8]) -> Result<Option<String>> {
    let records: Output<Holding> =
        serde_json::from_slice(output).map_err(|source| CrateError::OutputJson {
            operation: CLAIMING,
            source,
        })?;
    let record = records
        .into_records()
        .into_iter()
        .next()
        .ok_or(CrateError::Output {
            operation: CLAIMING,
        })?;
    Ok(record.assignee.filter(|name| !name.is_empty()))
}
