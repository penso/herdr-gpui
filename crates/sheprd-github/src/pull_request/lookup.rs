//! The single prefetch worker and the handle that talks to it. One batch is
//! in flight at a time; a superseded batch is dropped by generation rather
//! than cancelled mid-flight, and dropping the handle retires the generation.

use super::{Input, Origin, Origins, PullRequest, batch::Batch, batch::fetch_batch};
use crate::Tokens;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};

/// A batch's generation, checkouts, device, tokens, and the logins of the
/// accounts paused when it was sent.
pub(super) type Request = (u64, Vec<Input>, Origin, Tokens, Vec<String>);

pub(super) struct Worker {
    pub(super) requests: mpsc::SyncSender<Request>,
    pub(super) results: mpsc::Receiver<(u64, Batch)>,
}

/// One checkout's lookup as a view shows it.
#[derive(Default)]
pub struct Lookup {
    pub loading: bool,
    pub value: Option<PullRequest>,
    pub message: Option<String>,
    pub checked: Option<Instant>,
}

impl Lookup {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// How the current batch ended: answered, or never answered because the
/// worker could not run, which every checkout in it reports.
pub(super) enum Finished {
    Answered(Batch),
    Failed(&'static str),
}

#[derive(Default)]
pub(super) struct Prefetch {
    pub(super) worker: Option<Worker>,
    pub(super) generation: Arc<AtomicU64>,
    pub(super) busy: bool,
    waiting: Option<(Vec<Input>, Origin, Tokens, Vec<String>)>,
}

impl Drop for Prefetch {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Prefetch {
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        // Sign-out also drains already-queued private results without scheduling
        // more work. A still-running cancelled batch is drained on a later tick.
        if let Some(worker) = &self.worker
            && worker.results.try_recv().is_ok()
        {
            self.busy = false;
        }
        self.waiting = None;
    }

    pub fn request(
        &mut self,
        inputs: Vec<Input>,
        origin: Origin,
        tokens: impl Into<Tokens>,
        paused: Vec<String>,
    ) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.waiting = Some((inputs, origin, tokens.into(), paused));
    }

    /// The current batch's answer, once, after sending a waiting batch to the
    /// worker if it is free. A superseded batch's answer is discarded.
    pub fn poll(&mut self) -> Option<Finished> {
        let mut finished = None;
        if let Some(worker) = &self.worker {
            match worker.results.try_recv() {
                Ok((generation, batch)) => {
                    self.busy = false;
                    if generation == self.generation.load(Ordering::Relaxed) {
                        finished = Some(Finished::Answered(batch));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.busy = false;
                    self.waiting = None;
                    self.worker = None;
                    return Some(Finished::Failed(
                        "PR worker stopped; retrying automatically.",
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.busy
            && let Some((inputs, origin, token, paused)) = self.waiting.take()
        {
            if self.worker.is_none() {
                let (requests, incoming) = mpsc::sync_channel::<Request>(1);
                let (outgoing, results) = mpsc::sync_channel(1);
                let current = self.generation.clone();
                match thread::Builder::new()
                    .name("herdr-pr".into())
                    .spawn(move || {
                        // Remote repositories are resolved over SSH; remember
                        // them so a refresh does not dial the host again.
                        let mut origins = Origins::default();
                        for (generation, inputs, origin, token, paused) in incoming {
                            let batch =
                                fetch_batch(inputs, &origin, &mut origins, &token, &paused, || {
                                    current.load(Ordering::Relaxed) != generation
                                });
                            if outgoing.send((generation, batch)).is_err() {
                                break;
                            }
                        }
                    }) {
                    Ok(_) => self.worker = Some(Worker { requests, results }),
                    Err(_) => return Some(Finished::Failed("Could not start PR worker.")),
                }
            }
            if let Some(worker) = &self.worker {
                self.busy = worker
                    .requests
                    .try_send((
                        self.generation.load(Ordering::Relaxed),
                        inputs,
                        origin,
                        token,
                        paused,
                    ))
                    .is_ok();
            }
        }
        finished
    }
}
