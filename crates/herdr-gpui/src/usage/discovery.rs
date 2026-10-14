//! Offline installation evidence for Settings. One bounded worker, with
//! request-generation fencing, independent of provider enablement and quotas.
use super::{
    Host, PROVIDER_COUNT,
    probe::{Detector, Exec, Shell},
    providers,
};
use herdr_client::ConnectTarget;
use std::{sync::mpsc, thread};

struct Job {
    generation: u64,
    host: Host,
}

struct Answer {
    generation: u64,
    installed: crate::Result<[bool; PROVIDER_COUNT]>,
}

struct Worker {
    requests: mpsc::SyncSender<Job>,
    answers: mpsc::Receiver<Answer>,
}

pub(super) struct Discovery {
    target: Option<ConnectTarget>,
    generation: u64,
    installed: [bool; PROVIDER_COUNT],
    queued: Option<Job>,
    worker: Option<Worker>,
    busy: bool,
}

impl Default for Discovery {
    fn default() -> Self {
        Self {
            target: None,
            generation: 0,
            installed: [false; PROVIDER_COUNT],
            queued: None,
            worker: None,
            busy: false,
        }
    }
}
impl Discovery {
    pub fn request(&mut self, target: &ConnectTarget) {
        self.generation = self.generation.wrapping_add(1);
        if self.target.as_ref() != Some(target) {
            self.installed = [false; PROVIDER_COUNT];
        }
        self.target = Some(target.clone());
        self.queued = Host::of(target).map(|host| Job {
            generation: self.generation,
            host,
        });
        self.dispatch();
    }

    pub fn target_changed(&self, target: &ConnectTarget) -> bool {
        self.target
            .as_ref()
            .is_some_and(|previous| previous != target)
    }

    pub fn installed(&self, target: &ConnectTarget) -> [bool; PROVIDER_COUNT] {
        if self.target.as_ref() == Some(target) {
            self.installed
        } else {
            [false; PROVIDER_COUNT]
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Some(worker) = &self.worker {
            match worker.answers.try_recv() {
                Ok(answer) => {
                    self.busy = false;
                    if answer.generation == self.generation {
                        match answer.installed {
                            Ok(installed) if self.installed != installed => {
                                self.installed = installed;
                                changed = true;
                            }
                            Ok(_) => {}
                            Err(error) => tracing::warn!(
                                category = "usage",
                                %error,
                                "could not discover installation metadata"
                            ),
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.busy = false;
                }
            }
        }
        self.dispatch();
        changed
    }

    fn dispatch(&mut self) {
        if self.busy || self.queued.is_none() {
            return;
        }
        if self.worker.is_none() {
            self.worker = spawn();
        }
        let Some(worker) = &self.worker else {
            self.queued = None;
            return;
        };
        let Some(job) = self.queued.take() else {
            return;
        };
        match worker.requests.try_send(job) {
            Ok(()) => self.busy = true,
            Err(mpsc::TrySendError::Full(job)) => self.queued = Some(job),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.worker = None;
            }
        }
    }
}

fn spawn() -> Option<Worker> {
    let (requests, incoming) = mpsc::sync_channel::<Job>(1);
    let (outgoing, answers) = mpsc::sync_channel(1);
    let spawned = thread::Builder::new()
        .name("herdr-usage-discovery".into())
        .spawn(move || {
            for job in incoming {
                let installed = detect(&job.host);
                if outgoing
                    .send(Answer {
                        generation: job.generation,
                        installed,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
    match spawned {
        Ok(_) => Some(Worker { requests, answers }),
        Err(error) => {
            tracing::warn!(category = "usage", %error, "could not start installation discovery");
            None
        }
    }
}

fn detect(host: &Host) -> crate::Result<[bool; PROVIDER_COUNT]> {
    let mut exec = match host {
        Host::Local => Exec::Local,
        Host::Ssh(target) => Exec::Remote(Shell::connect(target)?),
        Host::Wsl(_) => return Err(crate::Error::WslHostUnsupported),
    };
    Ok(detect_with(&mut exec))
}

pub(super) fn detect_with(exec: &mut Exec) -> [bool; PROVIDER_COUNT] {
    let mut detector = Detector::new(exec);
    let mut installed = [false; PROVIDER_COUNT];
    for (index, provider) in providers().enumerate() {
        installed[index] = provider.service().installed(&mut detector);
    }
    installed
}

#[cfg(test)]
mod tests;
