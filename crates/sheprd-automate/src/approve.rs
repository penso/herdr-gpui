//! Approving a script on this host, pinned to its exact text. A person
//! approves either on a terminal, typing the script's name after reading
//! its plan, or in Sheprd's dialog, which passes the hash it showed.
//!
//! This guards against mistakes, not against a hostile agent: an agent with
//! a shell could run commands directly. It refuses where an agent would
//! most easily approve its own script: off a terminal, from a pane Herdr
//! says runs an agent, and `--hash` from inside any Herdr pane.

use crate::{
    Result, fingerprint::Fingerprint, herdr::Herdr, paths::Paths, runner::now, scan, store::Store,
};
use sheprd_automation::{Limits, Plan, Source};
use std::{
    io::{BufRead, IsTerminal, Write},
    path::Path,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not approved: the name typed did not match")]
    Cancelled,
    #[error(
        "the script changed since its plan was shown (expected {expected}, found {found}); review it again"
    )]
    Changed { expected: String, found: String },
    #[error("approve on a terminal, or in Sheprd's dialog; stdin is not a terminal")]
    NotTerminal,
    #[error(
        "this pane runs an agent ({agent}); ask the user to approve in Sheprd or their own terminal"
    )]
    AgentPane { agent: String },
    #[error("--hash is for Sheprd's dialog; inside a Herdr pane, approve interactively instead")]
    HashInPane,
}

/// How the approval is given.
pub enum Mode<'a> {
    /// Show the plan and ask on this terminal.
    Terminal,
    /// The hash a dialog showed the user, who confirmed it.
    Hash(&'a str),
}

pub struct Approved {
    pub name: String,
    pub hash: String,
}

/// `pane` is the Herdr pane the command runs in (`HERDR_PANE_ID`), if any.
pub fn approve(
    paths: &Paths,
    store: &Store,
    herdr: &Herdr,
    file: &Path,
    mode: Mode<'_>,
    pane: Option<&str>,
) -> Result<Approved> {
    let found = scan::locate(paths, store, file)?;
    let source = Source::read(&found.path)?;
    let plan = Plan::build(&source, Limits::default())?;
    let fingerprint = Fingerprint::of(&source, &plan, found.repo.as_deref());
    let by = match mode {
        Mode::Hash(hash) => {
            if pane.is_some() {
                return Err(Error::HashInPane.into());
            }
            if hash != fingerprint.hash {
                return Err(Error::Changed {
                    expected: hash.to_owned(),
                    found: fingerprint.hash.clone(),
                }
                .into());
            }
            "herdr-ui"
        }
        Mode::Terminal => {
            if !std::io::stdin().is_terminal() {
                return Err(Error::NotTerminal.into());
            }
            if let Some(pane) = pane
                && let Some(agent) = herdr.pane_agent(pane).ok().and_then(|pane| pane.agent)
            {
                return Err(Error::AgentPane { agent }.into());
            }
            let mut out = std::io::stdout().lock();
            let _ = write!(
                out,
                "{plan}{}\nsha256 {}\n\nType the script's name ({}) to approve it on this host: ",
                fingerprint.describe().unwrap_or_default(),
                fingerprint.hash,
                source.name
            );
            let _ = out.flush();
            let mut answer = String::new();
            let _ = std::io::stdin().lock().read_line(&mut answer);
            if answer.trim() != source.name {
                return Err(Error::Cancelled.into());
            }
            "terminal"
        }
    };
    let path = found.path.to_string_lossy().into_owned();
    let now = now();
    store.approve(&path, &fingerprint.hash, by, now)?;
    // Fire nothing for the time before it was approved.
    store.clear_cursors(&path)?;
    store.put_script(&scan::row(
        &source,
        Ok(&plan),
        Some(&fingerprint),
        path.clone(),
        found.repo.as_deref(),
        now,
    ))?;
    Ok(Approved {
        name: source.name,
        hash: fingerprint.hash,
    })
}
