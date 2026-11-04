//! How the session reaches the harness it runs. The session speaks ACP and
//! nothing else; a harness that speaks ACP is wired straight to it, and one
//! with a protocol of its own gets a [`Translator`] in between, on a thread
//! of its own, so the session cannot tell the two apart.

pub(crate) mod out;

use std::{
    io::{Read, Write},
    thread,
};

use crossbeam_channel::{Receiver, bounded, select};
use serde_json::Value;

use crate::{
    Error,
    rpc::{self, Inbound},
};

/// Lines from the session waiting for a translator.
const FROM_SESSION: usize = 1024;
/// Messages from the harness waiting for a translator.
const FROM_HARNESS: usize = 1024;

/// The started harness's standard input and output.
pub(crate) struct Pipes {
    pub input: Box<dyn Write + Send>,
    pub output: Box<dyn Read + Send>,
}

/// Hands the session what the harness says, as ACP; `false` once nobody
/// listens.
pub(crate) type Deliver = Box<dyn FnMut(Inbound) -> bool + Send>;

/// A way to reach a started harness.
pub(crate) trait Reach {
    /// Wires the harness's pipes to the session: the session writes ACP lines
    /// to the returned outbox and hears the harness through `deliver`.
    fn connect(self: Box<Self>, pipes: Pipes, deliver: Deliver) -> std::io::Result<rpc::Outbox>;
}

/// A harness that speaks ACP itself, or through its own adapter program.
pub(crate) struct Acp;

impl Reach for Acp {
    fn connect(self: Box<Self>, pipes: Pipes, deliver: Deliver) -> std::io::Result<rpc::Outbox> {
        rpc::spawn_reader(pipes.output, deliver)?;
        Ok(rpc::spawn_writer(pipes.input)?.0)
    }
}

/// A message a translator passes on.
#[derive(Debug)]
pub(crate) enum Out {
    /// To the session, as if an ACP agent sent it.
    Session(Inbound),
    /// To the harness, as one JSON line.
    Harness(Value),
}

/// Turns the session's ACP into a harness's own JSON lines and back. It does
/// no I/O: each message in yields the messages to pass on, which keeps it
/// testable without a process.
pub(crate) trait Translator: Send + 'static {
    /// What to send the harness before anything else, such as its handshake.
    fn start(&mut self) -> Vec<Out>;
    fn on_session(&mut self, inbound: Inbound) -> Vec<Out>;
    fn on_harness(&mut self, message: &Value) -> Vec<Out>;
}

/// A harness reached through `T`.
pub(crate) struct Translated<T>(pub T);

impl<T: Translator> Reach for Translated<T> {
    fn connect(
        self: Box<Self>,
        pipes: Pipes,
        mut deliver: Deliver,
    ) -> std::io::Result<rpc::Outbox> {
        let (to_harness, _writer) = rpc::spawn_writer(pipes.input)?;
        let (harness_sender, from_harness) = bounded(FROM_HARNESS);
        rpc::spawn_lines(pipes.output, "harness-reader", move |read| {
            let message = match read {
                rpc::Lines::Line(line) => match serde_json::from_slice::<Value>(line) {
                    Ok(message) => FromHarness::Message(message),
                    // A line that is not JSON, such as a stray log line, is skipped.
                    Err(_) => return true,
                },
                rpc::Lines::End(error) => FromHarness::Closed(error),
            };
            harness_sender.send(message).is_ok()
        })?;
        let (session_sender, from_session) = bounded(FROM_SESSION);
        let Self(mut translator) = *self;
        thread::Builder::new()
            .name("harness-translator".to_owned())
            .spawn(move || {
                let first = translator.start();
                run(
                    &mut translator,
                    first,
                    &to_harness,
                    &mut deliver,
                    &from_session,
                    &from_harness,
                );
            })?;
        Ok(rpc::Outbox(session_sender))
    }
}

/// What the harness sends, read on its own thread.
enum FromHarness {
    Message(Value),
    Closed(Option<Error>),
}

fn run(
    translator: &mut impl Translator,
    first: Vec<Out>,
    to_harness: &rpc::Outbox,
    deliver: &mut Deliver,
    from_session: &Receiver<Vec<u8>>,
    from_harness: &Receiver<FromHarness>,
) {
    let mut outs = first;
    loop {
        for out in outs.drain(..) {
            let delivered = match out {
                Out::Session(inbound) => deliver(inbound),
                Out::Harness(message) => match to_harness.send(&message) {
                    Ok(()) => true,
                    Err(error) => deliver(Inbound::Closed(Some(error))),
                },
            };
            if !delivered {
                return;
            }
        }
        select! {
            recv(from_session) -> line => match line {
                Ok(line) => outs.extend(
                    rpc::classify(&line)
                        .map(|inbound| translator.on_session(inbound))
                        .unwrap_or_default(),
                ),
                // The session is gone.
                Err(_) => return,
            },
            recv(from_harness) -> message => match message {
                Ok(FromHarness::Message(message)) => outs.extend(translator.on_harness(&message)),
                Ok(FromHarness::Closed(error)) => {
                    deliver(Inbound::Closed(error));
                    return;
                }
                Err(_) => return,
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
