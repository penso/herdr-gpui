//! A view of a verified, already-running CLI. Its original terminal owns all
//! input. This child only projects history; closing the GUI stops this child,
//! never the agent. No persistent sidecar, replacement session, or socket.

mod error;
mod identity;
#[cfg(test)]
mod tests;

pub use error::{Error, Result};

use crate::{
    cli::ObserveOptions,
    mirror::{Mirror, POLL, transcript::Source},
    session::{Attach, ConnId, Input},
};
use anyhow::{Context as _, bail};
use crossbeam_channel::{Receiver, bounded};
use herdr_chat_protocol::{
    ByeReason, ClientMessage, ServerMessage, SessionInfo, Transcript, VERSION, read_frame,
    write_frame,
};
use std::{
    io, thread,
    time::{Duration, Instant},
};

const CONN: ConnId = ConnId(1);
/// How long the client has to send its greeting.
const GREETING: Duration = Duration::from_secs(5);
/// Client messages read from stdin ahead of the observer.
const INPUTS: usize = 32;
/// Messages queued for stdout between two writes.
const OUTBOX: usize = 4096;

/// Reading and parsing stdin is bounded and separate from polling the file.
/// EOF tears down the observer even when the agent itself is idle.
fn inputs() -> io::Result<Receiver<ClientMessage>> {
    let (sender, receiver) = bounded(INPUTS);
    thread::Builder::new()
        .name("chat-observer-input".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            while let Ok(Some(message)) = read_frame(&mut stdin) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        })?;
    Ok(receiver)
}

pub(crate) fn run(options: ObserveOptions) -> anyhow::Result<()> {
    let receiver = inputs()?;
    handshake(&receiver)?;
    let mut observer = Observer::open(options)?;
    let (outbox, outgoing) = bounded(OUTBOX);
    observer.mirror.input(Input::Attach(Attach {
        conn: CONN,
        local: false,
        outbox,
        since: None,
    }));
    let mut stdout = io::stdout().lock();
    let mut next_poll = Instant::now() + POLL;
    loop {
        for message in outgoing.try_iter() {
            write_frame(&mut stdout, &message)?;
        }
        match receiver.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(ClientMessage::Command { id, command }) => {
                // Historical permission requests are never actionable here.
                observer.mirror.input(Input::Command {
                    conn: CONN,
                    id,
                    command,
                });
            }
            Ok(ClientMessage::Hello { .. }) => bail!("duplicate observer greeting"),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        if Instant::now() >= next_poll {
            if !observer.poll()? {
                write_frame(
                    &mut stdout,
                    &ServerMessage::Bye {
                        reason: ByeReason::Exiting,
                    },
                )?;
                break;
            }
            next_poll = Instant::now() + POLL;
        }
    }
    Ok(())
}

fn handshake(receiver: &Receiver<ClientMessage>) -> anyhow::Result<()> {
    let greeting = receiver.recv_timeout(GREETING)?;
    if !matches!(
        greeting,
        ClientMessage::Hello {
            version: VERSION,
            ..
        }
    ) {
        bail!("the observer requires a compatible chat greeting");
    }
    Ok(())
}

/// The verified agent, its pinned transcript, and the view projected from it.
struct Observer {
    identity: identity::Identity,
    mirror: Mirror,
    session: SessionInfo,
    source: Source,
}

impl Observer {
    fn open(options: ObserveOptions) -> anyhow::Result<Self> {
        let identity = identity::Identity::open(&options)?;
        let mut source = identity.source()?;
        let initial = source
            .read()?
            .context("the running agent's transcript is unavailable")?;
        identity.check()?;
        let session = SessionInfo {
            id: options.session,
            cwd: identity.cwd.clone(),
            title: None,
        };
        let mirror = Mirror::new(project(&session, initial), options.pane);
        Ok(Self {
            identity,
            mirror,
            session,
            source,
        })
    }

    /// Re-reads the transcript if it changed; `false` once the agent exited.
    /// Rotation or a temporarily incomplete header is an error that closes
    /// only this observer; the client may reconnect to the same live agent.
    fn poll(&mut self) -> Result<bool> {
        if !self.identity.process_matches()? {
            return Ok(false);
        }
        // One file check after the read covers both a rotated path and a
        // file rewritten in place under the read, so its result is never
        // shown unverified.
        let next = self.source.read()?;
        self.identity.check_file()?;
        if let Some(transcript) = next {
            self.mirror.update(project(&self.session, transcript));
        }
        Ok(true)
    }
}

/// The bounded tail need not include the original session header.
fn project(session: &SessionInfo, mut transcript: Transcript) -> Transcript {
    transcript.session = Some(session.clone());
    transcript
}
