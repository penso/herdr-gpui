//! The supervisor's part in pairing phones with other hosts
//! (`remote::links`): it tells the daemon which hosts this app connects to,
//! takes the phones' requests with each poll, runs a job per host, and
//! carries out the revocations hosts are owed.

use super::*;
use crate::remote::links::{self, Job, Known};
use herdr_client::ConnectTarget;
use herdr_remote_control::{
    app::enroll::Apns,
    control::hosts::{Host, HostId, HostOutcome, HostPairing, HostRevoke},
};
use std::{collections::HashSet, sync::atomic::AtomicBool};

/// How often the host list is sent again unchanged, for a daemon that
/// restarted between polls, and the revocations owed are asked for.
const RESEND: Duration = Duration::from_secs(60);
/// How long a host's answer is offered again to a daemon that could not
/// record it, doubling from the first wait to the longest.
const REPORT_FIRST_WAIT: Duration = Duration::from_secs(2);
const REPORT_LONGEST_WAIT: Duration = Duration::from_secs(60);
const REPORT_ATTEMPTS: u32 = 30;

/// The supervisor's host state.
#[derive(Debug, Default)]
pub(crate) struct Links {
    /// The hosts this app connects to, as windows last said.
    known: Vec<Known>,
    /// The list the daemon last took, and when.
    sent: Option<(Vec<Host>, Instant)>,
    revokes_checked: Option<Instant>,
    /// Pairings running, or waiting in `asks`.
    running: HashSet<(DeviceId, HostId)>,
    /// Pairings of hosts the desktop did not allow, waiting for a window to
    /// ask the user.
    asks: Vec<Job>,
    revoking: HashSet<HostRevoke>,
}

/// What a poll sends about hosts.
pub(super) struct Outgoing {
    hosts: Option<Vec<Host>>,
    revokes: bool,
}

/// What the daemon answered.
pub(super) struct Incoming {
    sent: Option<Vec<Host>>,
    pairings: Vec<HostPairing>,
    revokes: Option<Vec<HostRevoke>>,
}

impl Links {
    pub(super) fn outgoing(&self) -> Outgoing {
        let hosts: Vec<Host> = self.known.iter().map(|known| known.host.clone()).collect();
        let due = self
            .sent
            .as_ref()
            .is_none_or(|(sent, at)| *sent != hosts || at.elapsed() >= RESEND);
        Outgoing {
            hosts: due.then_some(hosts),
            revokes: self
                .revokes_checked
                .is_none_or(|checked| checked.elapsed() >= RESEND),
        }
    }

    /// A poll found the daemon down: everything is sent again.
    pub(super) fn lost(&mut self) {
        self.sent = None;
        self.revokes_checked = None;
    }

    fn target(&self, host: &HostId) -> Option<ConnectTarget> {
        self.known
            .iter()
            .find(|known| &known.host.id == host)
            .map(|known| known.target.clone())
    }
}

/// Sends what `outgoing` names and takes the phones' requests. Blocking.
pub(super) fn exchange(address: &Path, send: Send, outgoing: Outgoing) -> Incoming {
    let sent = outgoing.hosts.filter(|hosts| {
        matches!(
            send(
                address,
                &Request::Hosts {
                    hosts: hosts.clone()
                }
            ),
            Ok(Response::Done)
        )
    });
    let pairings = match send(address, &Request::HostPairings) {
        Ok(Response::HostPairings { pairings }) => pairings,
        _ => Vec::new(),
    };
    let revokes = outgoing
        .revokes
        .then(|| match send(address, &Request::HostRevokes) {
            Ok(Response::HostRevokes { revokes }) => Some(revokes),
            _ => None,
        })
        .flatten();
    Incoming {
        sent,
        pairings,
        revokes,
    }
}

impl Supervisor {
    /// The hosts a window connects to, for the daemon to offer phones.
    pub(crate) fn set_hosts(cx: &mut App, known: Vec<Known>) {
        let Some(supervisor) = cx.try_global::<Self>() else {
            return;
        };
        if supervisor.links.known != known {
            cx.global_mut::<Self>().links.known = known;
        }
    }

    /// The hosts a phone may be paired with, for the pairing dialog.
    pub(crate) fn hosts(cx: &App) -> Vec<Host> {
        cx.try_global::<Self>()
            .map(|supervisor| {
                supervisor
                    .links
                    .known
                    .iter()
                    .map(|known| known.host.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Takes a pairing a window should ask the user about, if any.
    pub(crate) fn take_host_ask(cx: &mut App) -> Option<Job> {
        let supervisor = cx.try_global::<Self>()?;
        if supervisor.links.asks.is_empty() {
            return None;
        }
        let links = &mut cx.global_mut::<Self>().links;
        let job = links.asks.remove(0);
        links
            .running
            .remove(&(job.pairing.device.clone(), job.pairing.host.clone()));
        Some(job)
    }

    /// Queues `job` for a window to ask about, as tests do.
    #[cfg(test)]
    pub(crate) fn queue_host_ask_for_tests(cx: &mut App, job: Job) {
        cx.global_mut::<Self>().links.asks.push(job);
    }

    /// The user's answer to a host the desktop had not allowed.
    pub(crate) fn answer_host(cx: &mut App, job: Job, allow: bool) {
        if allow {
            Self::run_host_job(cx, job);
            return;
        }
        let Some(client) = Self::client(cx) else {
            return;
        };
        client.notify(Request::HostPaired {
            device: job.pairing.device,
            host: job.pairing.host,
            public_key: Some(job.pairing.public_key),
            outcome: HostOutcome::Denied,
        });
    }

    /// Takes a poll's answers about hosts and starts their jobs.
    pub(super) fn take_incoming(cx: &mut App, incoming: Incoming) {
        let supervisor = cx.global_mut::<Self>();
        if let Some(sent) = incoming.sent {
            supervisor.links.sent = Some((sent, Instant::now()));
        }
        if incoming.revokes.is_some() {
            supervisor.links.revokes_checked = Some(Instant::now());
        }
        let via = match &supervisor.health {
            Health::Running(status) if !status.computer.is_empty() => status.computer.clone(),
            _ => "This computer".to_owned(),
        };
        for pairing in incoming.pairings {
            let key = (pairing.device.clone(), pairing.host.clone());
            let links = &mut cx.global_mut::<Self>().links;
            if !links.running.insert(key) {
                continue;
            }
            let job = Job {
                target: links.target(&pairing.host),
                pairing,
                via: via.clone(),
                apns: None,
            };
            if job.pairing.confirm {
                links.asks.push(job);
                cx.refresh_windows();
            } else {
                Self::run_host_job(cx, job);
            }
        }
        for revoke in incoming.revokes.into_iter().flatten() {
            Self::run_host_revoke(cx, revoke);
        }
    }

    fn run_host_job(cx: &mut App, mut job: Job) {
        let key = (job.pairing.device.clone(), job.pairing.host.clone());
        let Some(client) = Self::client(cx) else {
            cx.global_mut::<Self>().links.running.remove(&key);
            return;
        };
        cx.global_mut::<Self>().links.running.insert(key.clone());
        let public_key = job.pairing.public_key.clone();
        let task = cx.background_executor().spawn(async move {
            // The host gets this daemon's APNs key, to notify the phone
            // itself while this computer sleeps.
            job.apns = crate::remote::state_dir()
                .ok()
                .and_then(|dir| Apns::read(&dir));
            links::pair(&job, &AtomicBool::new(false))
        });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            let outcome = task.await;
            let (device, host) = key.clone();
            let request = Request::HostPaired {
                device,
                host,
                public_key: Some(public_key),
                outcome,
            };
            // The host holds the phone's key now: an answer the daemon did
            // not record is a device nobody could revoke later, so it is
            // offered again while the daemon cannot save or be reached.
            let mut wait = REPORT_FIRST_WAIT;
            for attempt in 1..=REPORT_ATTEMPTS {
                let answer = client.request(request.clone()).await;
                if !retry_report(&answer) {
                    break;
                }
                tracing::info!(attempt, "Remote control did not record a host's answer yet");
                executor.timer(wait).await;
                wait = (wait * 2).min(REPORT_LONGEST_WAIT);
            }
            cx.update(|cx| {
                cx.global_mut::<Self>().links.running.remove(&key);
                Self::refresh(cx);
            });
        })
        .detach();
    }

    fn run_host_revoke(cx: &mut App, revoke: HostRevoke) {
        let Some(client) = Self::client(cx) else {
            return;
        };
        let links = &mut cx.global_mut::<Self>().links;
        if !links.revoking.insert(revoke.clone()) {
            return;
        }
        let target = links.target(&revoke.host);
        let task = cx.background_executor().spawn({
            let device = revoke.device.clone();
            async move { links::revoke(target.as_ref(), &device, &AtomicBool::new(false)) }
        });
        cx.spawn(async move |cx| {
            match task.await {
                Ok(()) => {
                    let _ = client.request(Request::HostRevoked(revoke.clone())).await;
                }
                // Kept owed; tried again with the next check.
                Err(error) => {
                    tracing::info!(host = %revoke.host.0, %error, "Could not revoke a phone on a host yet");
                }
            }
            cx.update(|cx| {
                cx.global_mut::<Self>().links.revoking.remove(&revoke);
            });
        })
        .detach();
    }
}

/// Whether a host's answer should be offered again: the daemon was not
/// reached, or could not save it. A refusal stands.
fn retry_report(answer: &Result<Response, control::Error>) -> bool {
    match answer {
        Ok(_) => false,
        Err(control::Error::Daemon { code, .. }) => *code == control::ErrorCode::Unsaved,
        Err(_) => true,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
