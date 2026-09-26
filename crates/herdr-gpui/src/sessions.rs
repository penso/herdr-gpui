//! The sessions this window can attach to: the named local sessions and, since
//! a saved device's own sessions need an SSH round trip to list, the devices
//! themselves. Only the sessions popup asks for either: probing is I/O, so it
//! runs on a worker thread and an idle window never touches the disk or a host.
use crate::Error;
use herdr_client::{
    ConnectTarget, LocalSession, RemoteSession, list_local_sessions, list_remote_sessions,
};
use std::{
    collections::HashMap,
    sync::mpsc,
    time::{Duration, Instant},
};

/// How long an open popup shows probe results before refreshing them. Session
/// sockets outlive their daemon, so the state is a probe, not a fact.
const REFRESH: Duration = Duration::from_secs(5);

/// How long one saved device's answer stands before its host is asked again. A
/// device's list only changes when a session starts or stops there, and each
/// answer costs an SSH connection, so the popup does not redial at the local
/// probe's rate.
const DEVICE_REFRESH: Duration = Duration::from_secs(30);

pub(super) const DELETION_ANIMATION: Duration = Duration::from_millis(260);

pub(super) struct Departure {
    pub(super) target: ConnectTarget,
    started: Instant,
}

impl Departure {
    pub(super) fn new(target: ConnectTarget, now: Instant) -> Self {
        Self {
            target,
            started: now,
        }
    }

    /// Fade without changing layout, driven by the window's existing frame poll.
    pub(super) fn remaining(&self, now: Instant) -> f32 {
        let progress = (now.saturating_duration_since(self.started).as_secs_f32()
            / DELETION_ANIMATION.as_secs_f32())
        .clamp(0., 1.);
        1. - progress
    }
}

/// What the last probe of one saved device found.
pub(super) enum DeviceScan {
    /// The sessions that host reported.
    Sessions(Vec<RemoteSession>),
    /// Why they could not be listed, in the popup's own words.
    Failed(String),
}

/// Named sessions on this machine, the saved devices' own sessions, and the
/// scans that keep both current.
#[derive(Default)]
pub(super) struct Sessions {
    /// Last successfully scanned sessions, kept across popup opens.
    pub(super) entries: Vec<LocalSession>,
    /// The saved devices' sessions, which need a probe each.
    pub(super) devices: Devices,
    /// Set while a scan is outstanding, so the popup can say the list may be
    /// about to change.
    pub(super) scanning: bool,
    pub(super) error: Option<String>,
    pub(super) mutation: Option<gpui::Task<()>>,
    pub(super) mutation_error: Option<String>,
    pub(super) mutation_target: Option<ConnectTarget>,
    pub(super) departure: Option<Departure>,
    refresh_after_scan: bool,
    pending: Option<mpsc::Receiver<Result<Vec<LocalSession>, Error>>>,
    /// `None` means the next open scans immediately.
    next_scan: Option<Instant>,
}

/// Every saved device's sessions, and the probes that keep them current. Each
/// device ages out on its own, so one host answering slowly never holds up
/// another host's list.
#[derive(Default)]
pub(super) struct Devices {
    /// Last answer per endpoint id, kept across popup opens. Keyed by id rather
    /// than index, so a catalog reorder cannot move an answer to another host.
    pub(super) answers: HashMap<String, DeviceScan>,
    /// Answers on their way back, each naming the host it is about.
    pending: Option<mpsc::Receiver<Vec<(String, String, DeviceScan)>>>,
    /// What the outstanding pass is asking, host included, so a worker that dies
    /// fails only the devices whose answers are actually unknown.
    asking: Vec<(String, String)>,
    /// When each device was last asked and of which host, successful or not: a
    /// host that cannot answer is not redialled on every refresh, and a device
    /// whose host changed is asked again rather than answered for the old one.
    asked: HashMap<String, (String, Instant)>,
    discard_pending: bool,
}

impl Sessions {
    pub(super) fn finish_departure(&mut self) {
        let Some(departure) = self.departure.take() else {
            return;
        };
        match departure.target {
            ConnectTarget::Session { name, .. } => {
                self.entries.retain(|session| session.name != name)
            }
            ConnectTarget::Ssh { target, session } => {
                for (id, answer) in &mut self.devices.answers {
                    if self
                        .devices
                        .asked
                        .get(id)
                        .is_some_and(|(host, _)| *host == target)
                        && let DeviceScan::Sessions(sessions) = answer
                    {
                        sessions.retain(|entry| entry.name != session);
                    }
                }
            }
            _ => {}
        }
    }
    /// Probe again as soon as the popup is on screen, including when a scan is
    /// still running: the newest answer is the one worth showing.
    pub(super) fn refresh(&mut self) {
        self.next_scan = None;
        self.refresh_after_scan = self.pending.is_some();
    }
}

impl Devices {
    pub(super) fn refresh(&mut self) {
        // Keep the visible catalog while refreshing, but never accept an answer
        // started before the mutation. Track old workers until they finish.
        self.discard_pending = self.pending.is_some();
        let stale_at = Instant::now() - DEVICE_REFRESH;
        for (_, asked_at) in self.asked.values_mut() {
            *asked_at = stale_at;
        }
    }
    /// Apply a finished pass, and start one for every device whose answer has
    /// aged out. `targets` is the endpoint id and SSH target of each device this
    /// window may ask. Reports whether the popup should repaint. Opening the
    /// popup does not reset these ages: a host asked a moment ago is still
    /// inside its own interval.
    pub(super) fn poll(&mut self, targets: &[(String, String)], open: bool, now: Instant) -> bool {
        self.poll_with(targets, open, now, list_remote_sessions)
    }

    fn poll_with(
        &mut self,
        targets: &[(String, String)],
        open: bool,
        now: Instant,
        probe: impl Fn(&str) -> Result<Vec<RemoteSession>, herdr_client::Error> + Send + Sync + 'static,
    ) -> bool {
        let mut changed = false;
        // A device that is gone, or whose host changed under the same id, must not
        // keep painting what the previous host said.
        self.forget_replaced(targets);
        match self.pending.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(answers)) => {
                self.pending = None;
                self.asking.clear();
                let discard = std::mem::take(&mut self.discard_pending);
                for (id, host, answer) in answers {
                    // A device replaced while its probe was in flight keeps what
                    // its new host says, not what the old one said.
                    if !discard && self.asked.get(&id).is_some_and(|(asked, _)| *asked == host) {
                        self.answers.insert(id, answer);
                    }
                }
                changed = true;
            }
            // A worker that died without reporting leaves only the devices it was
            // asking unknown: an answer already in hand still stands.
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.pending = None;
                let discard = std::mem::take(&mut self.discard_pending);
                for (id, host) in std::mem::take(&mut self.asking) {
                    if !discard && self.asked.get(&id).is_some_and(|(asked, _)| *asked == host) {
                        self.answers.insert(
                            id,
                            DeviceScan::Failed("the device scan stopped unexpectedly".to_owned()),
                        );
                    }
                }
                changed = true;
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
        if open
            && self.pending.is_none()
            && targets
                .iter()
                .any(|(id, host)| stale(&self.asked, id, host, now))
        {
            let stale: Vec<(String, String)> = targets
                .iter()
                .filter(|(id, host)| stale(&self.asked, id, host, now))
                .cloned()
                .collect();
            self.start(stale, now, probe);
            changed = true;
        }
        changed
    }

    /// Forget a device's answer and its age when it is no longer one of this
    /// window's devices, or when its host changed under the same id.
    pub(super) fn forget_replaced(&mut self, targets: &[(String, String)]) {
        let still_here = |id: &String, host: &String| {
            targets
                .iter()
                .any(|(device, target)| device == id && target == host)
        };
        self.asking.retain(|(id, host)| still_here(id, host));
        self.asked.retain(|id, (host, _)| still_here(id, host));
        self.answers.retain(|id, _| self.asked.contains_key(id));
    }

    fn start(
        &mut self,
        stale: Vec<(String, String)>,
        now: Instant,
        probe: impl Fn(&str) -> Result<Vec<RemoteSession>, herdr_client::Error> + Send + Sync + 'static,
    ) {
        let ids: Vec<String> = stale.iter().map(|(id, _)| id.clone()).collect();
        let (tx, rx) = mpsc::sync_channel(1);
        // Stamp the pass before it runs, so a host that cannot answer waits out
        // the interval like one that can, and remember which hosts it is asking.
        self.asking = stale.clone();
        for (id, host) in &stale {
            self.asked.insert(id.clone(), (host.clone(), now));
        }
        match std::thread::Builder::new()
            .name("herdr-gui-devices".into())
            .spawn(move || {
                let _ = tx.send(probe_all(&stale, &probe));
            }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => {
                // A worker that cannot start must not leave the devices
                // checking: the popup says what went wrong on each of them.
                self.asking.clear();
                let message = Error::from(error).to_string();
                for id in ids {
                    self.answers.insert(id, DeviceScan::Failed(message.clone()));
                }
            }
        }
    }
}

/// Whether a device is due to be asked. One that was never asked, or whose host
/// changed under the same id, is due however recently the old host answered.
fn stale(asked: &HashMap<String, (String, Instant)>, id: &str, host: &str, now: Instant) -> bool {
    asked.get(id).is_none_or(|(asked_host, at)| {
        asked_host != host || now.duration_since(*at) >= DEVICE_REFRESH
    })
}

/// One device probe: the sessions that host reports, or why it could not.
type Probe = dyn Fn(&str) -> Result<Vec<RemoteSession>, herdr_client::Error> + Send + Sync;

/// Ask every stale device at once: one unreachable host must not hold up the
/// rest of the list, and each probe carries its own deadline.
fn probe_all(stale: &[(String, String)], probe: &Probe) -> Vec<(String, String, DeviceScan)> {
    let mut answers = Vec::with_capacity(stale.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = stale
            .iter()
            .map(|(_, target)| scope.spawn(|| probe(target)))
            .collect();
        for ((id, host), handle) in stale.iter().zip(handles) {
            // A probe that panicked still reports for its own device, so its row
            // never sits on "Checking…" waiting for a thread that is already gone.
            let answer = match handle.join() {
                Ok(Ok(sessions)) => DeviceScan::Sessions(sessions),
                Ok(Err(error)) => DeviceScan::Failed(error.to_string()),
                Err(_) => DeviceScan::Failed("the device probe stopped unexpectedly".to_owned()),
            };
            answers.push((id.clone(), host.clone(), answer));
        }
    });
    answers
}

impl Sessions {
    /// Apply a finished scan, and start one when the popup is open and its
    /// results have aged out. Reports whether the popup should repaint.
    pub(super) fn poll(&mut self, development: bool, open: bool, now: Instant) -> bool {
        self.poll_with(open, now, move || {
            list_local_sessions(development).map_err(Error::from)
        })
    }

    fn poll_with(
        &mut self,
        open: bool,
        now: Instant,
        scan: impl FnOnce() -> Result<Vec<LocalSession>, Error> + Send + 'static,
    ) -> bool {
        let mut changed = false;
        let finished = self.pending.as_ref().map(|rx| rx.try_recv());
        match finished {
            Some(Ok(result)) => {
                self.pending = None;
                self.scanning = false;
                let discard = std::mem::take(&mut self.refresh_after_scan);
                self.next_scan = if discard { None } else { Some(now + REFRESH) };
                match result {
                    _ if discard => {}
                    Ok(entries) => {
                        self.entries = entries;
                        self.error = None;
                    }
                    // A failed scan keeps the last list: names that were right a
                    // moment ago beat an empty popup.
                    Err(error) => self.error = Some(error.to_string()),
                }
                changed = true;
            }
            // A worker that died without reporting must not leave the popup
            // scanning forever, exactly as one that could not start.
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.pending = None;
                self.scanning = false;
                self.next_scan = if std::mem::take(&mut self.refresh_after_scan) {
                    None
                } else {
                    Some(now + REFRESH)
                };
                self.error = Some("the session scan stopped unexpectedly".to_owned());
                changed = true;
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
        if open && self.pending.is_none() && self.next_scan.is_none_or(|at| now >= at) {
            self.start(now, scan);
            changed = true;
        }
        changed
    }

    fn start(
        &mut self,
        now: Instant,
        scan: impl FnOnce() -> Result<Vec<LocalSession>, Error> + Send + 'static,
    ) {
        let (tx, rx) = mpsc::sync_channel(1);
        self.scanning = true;
        match std::thread::Builder::new()
            .name("herdr-gui-sessions".into())
            .spawn(move || {
                let _ = tx.send(scan());
            }) {
            Ok(_) => self.pending = Some(rx),
            Err(error) => {
                // A worker that cannot start must not leave the popup scanning.
                self.scanning = false;
                self.error = Some(Error::from(error).to_string());
                self.next_scan = Some(now + REFRESH);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use herdr_client::SessionState;
    use std::path::PathBuf;

    fn session(name: &str, state: SessionState) -> LocalSession {
        LocalSession {
            name: name.into(),
            state,
            socket: PathBuf::from(format!("/config/herdr/sessions/{name}/herdr-client.sock")),
        }
    }

    fn now() -> Instant {
        Instant::now()
    }

    /// A scan the test releases by hand, so when it finishes is never a guess.
    fn blocked_scan() -> (
        mpsc::Sender<()>,
        impl FnOnce() -> Result<Vec<LocalSession>, Error> + Send + 'static,
    ) {
        let (release, blocked) = mpsc::channel();
        let scan = move || {
            let _ = blocked.recv();
            Ok(vec![session("work", SessionState::Running)])
        };
        (release, scan)
    }

    #[test]
    fn a_closed_popup_never_scans_but_still_applies_a_finished_scan() {
        let mut sessions = Sessions::default();
        assert!(!sessions.poll_with(false, now(), || {
            unreachable!("a closed popup must not scan")
        }));
        let (tx, rx) = mpsc::sync_channel(1);
        sessions.pending = Some(rx);
        sessions.scanning = true;
        tx.send(Ok(vec![session("work", SessionState::Running)]))
            .unwrap();
        assert!(sessions.poll_with(false, now(), || unreachable!("still closed")));
        assert_eq!(sessions.entries.len(), 1);
        assert!(!sessions.scanning);
        assert!(sessions.error.is_none());
    }

    #[test]
    fn an_open_popup_scans_then_waits_for_the_refresh_interval() {
        let mut sessions = Sessions::default();
        let (release, scan) = blocked_scan();
        assert!(sessions.poll_with(true, now(), scan));
        assert!(sessions.scanning);
        // A second poll while that scan runs must not queue another one.
        assert!(!sessions.poll_with(true, now(), || {
            unreachable!("a scan is already running")
        }));
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let settled = |sessions: &mut Sessions| {
            sessions.poll_with(false, Instant::now(), || unreachable!("closed"))
        };
        while !settled(&mut sessions) {
            assert!(Instant::now() < deadline, "the scan never reported");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(sessions.entries.len(), 1);
        let scanned_at = now();
        // Inside the interval, an open popup leaves the probe and its result alone.
        assert!(!sessions.poll_with(true, scanned_at + REFRESH / 2, || {
            unreachable!("results are still fresh")
        }));
        // Past it, the popup probes again.
        assert!(sessions.poll_with(true, scanned_at + REFRESH, || Ok(vec![])));
    }

    #[test]
    fn a_failed_scan_keeps_the_last_list_and_reports_why() {
        let mut sessions = Sessions {
            entries: vec![session("work", SessionState::Stopped)],
            ..Default::default()
        };
        let (tx, rx) = mpsc::sync_channel(1);
        sessions.pending = Some(rx);
        tx.send(Err(Error::Client(herdr_client::Error::SessionLimit)))
            .unwrap();
        assert!(sessions.poll_with(false, now(), || unreachable!("closed")));
        assert_eq!(sessions.entries.len(), 1);
        assert_eq!(sessions.error.as_deref(), Some("too many sessions to list"));
    }

    #[test]
    fn a_worker_that_dies_without_reporting_does_not_leave_the_popup_scanning() {
        let mut sessions = Sessions {
            scanning: true,
            ..Default::default()
        };
        let (tx, rx) = mpsc::sync_channel::<Result<Vec<LocalSession>, Error>>(1);
        sessions.pending = Some(rx);
        drop(tx);
        assert!(sessions.poll_with(false, now(), || { unreachable!("a scan is outstanding") }));
        assert!(!sessions.scanning);
        assert!(sessions.pending.is_none());
        assert!(sessions.error.is_some());
    }

    #[test]
    fn opening_the_popup_requests_a_fresh_scan() {
        let mut sessions = Sessions::default();
        let scanned_at = now();
        assert!(sessions.poll_with(true, scanned_at, || Ok(vec![])));
        sessions.pending.take();
        sessions.scanning = false;
        sessions.next_scan = Some(scanned_at + REFRESH);
        // Opening again must not wait out the interval the previous open left.
        sessions.refresh();
        assert!(sessions.poll_with(true, scanned_at, || Ok(vec![])));
    }

    #[test]
    fn mutation_refresh_survives_an_older_local_scan() {
        let mut sessions = Sessions::default();
        let (tx, rx) = mpsc::sync_channel(1);
        sessions.pending = Some(rx);
        sessions.refresh();
        tx.send(Ok(vec![])).unwrap();
        sessions.poll_with(false, now(), || unreachable!("closed"));
        assert!(
            sessions.next_scan.is_none(),
            "the old answer must not delay the post-mutation scan"
        );
        assert!(sessions.poll_with(true, now(), || Ok(vec![])));
    }

    #[test]
    fn mutation_refresh_rejects_an_older_remote_answer() {
        let mut devices = Devices::default();
        let targets = [device("build")];
        devices
            .asked
            .insert(targets[0].0.clone(), (targets[0].1.clone(), now()));
        let (tx, rx) = mpsc::sync_channel(1);
        devices.pending = Some(rx);
        devices.refresh();
        tx.send(vec![(
            targets[0].0.clone(),
            targets[0].1.clone(),
            DeviceScan::Sessions(vec![]),
        )])
        .unwrap();
        devices.poll_with(&targets, false, now(), |_| unreachable!("closed"));
        assert!(devices.answers.is_empty());
        assert!(stale(&devices.asked, &targets[0].0, &targets[0].1, now()));
    }

    #[test]
    fn departure_is_bounded_and_an_old_scan_cannot_restore_a_deleted_row() {
        let started = now();
        let target = ConnectTarget::Session {
            name: "old".into(),
            development: false,
        };
        let departure = Departure::new(target, started);
        assert_eq!(departure.remaining(started), 1.);
        assert_eq!(departure.remaining(started + DELETION_ANIMATION / 2), 0.5);
        assert_eq!(departure.remaining(started + DELETION_ANIMATION), 0.);
        assert_eq!(departure.remaining(started + DELETION_ANIMATION * 2), 0.);
        let mut sessions = Sessions {
            entries: vec![
                session("old", SessionState::Stopped),
                session("keep", SessionState::Running),
            ],
            departure: Some(departure),
            ..Default::default()
        };
        let (tx, rx) = mpsc::sync_channel(1);
        sessions.pending = Some(rx);
        tx.send(Ok(vec![session("old", SessionState::Stopped)]))
            .unwrap();
        sessions.finish_departure();
        sessions.refresh();
        sessions.poll_with(false, now(), || unreachable!("closed"));
        assert_eq!(
            sessions
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["keep"]
        );
        assert!(sessions.departure.is_none());
        assert!(sessions.next_scan.is_none());
    }

    #[test]
    fn remote_departure_keeps_other_rows_and_refresh_keeps_the_updated_catalog() {
        let mut sessions = Sessions::default();
        for (id, host) in [("device", "host"), ("other", "elsewhere")] {
            sessions
                .devices
                .asked
                .insert(id.into(), (host.into(), now()));
            sessions.devices.answers.insert(
                id.into(),
                DeviceScan::Sessions(vec![
                    RemoteSession {
                        name: "old".into(),
                        running: false,
                    },
                    RemoteSession {
                        name: "keep".into(),
                        running: true,
                    },
                ]),
            );
        }
        sessions.departure = Some(Departure::new(
            ConnectTarget::Ssh {
                target: "host".into(),
                session: "old".into(),
            },
            now(),
        ));
        sessions.finish_departure();
        sessions.devices.refresh();
        assert!(
            matches!(&sessions.devices.answers["device"], DeviceScan::Sessions(rows) if rows.len() == 1 && rows[0].name == "keep")
        );
        assert!(
            matches!(&sessions.devices.answers["other"], DeviceScan::Sessions(rows) if rows.len() == 2)
        );
    }

    /// One saved device as the window's endpoint list describes it.
    fn device(name: &str) -> (String, String) {
        (format!("ssh:{name}"), format!("{name}.invalid"))
    }

    /// Wait for the pass the popup started, counting nothing it never began.
    fn settle(devices: &mut Devices, targets: &[(String, String)]) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while devices.pending.is_some() {
            assert!(Instant::now() < deadline, "the device pass never reported");
            devices.poll_with(targets, false, Instant::now(), |_| {
                unreachable!("a closed popup starts no pass")
            });
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A probe that records each target it was handed.
    fn recorder() -> (std::sync::Arc<std::sync::Mutex<Vec<String>>>, Box<Probe>) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let probe: Box<Probe> = Box::new(move |target: &str| {
            recorded.lock().unwrap().push(target.to_owned());
            Ok(Vec::new())
        });
        (seen, probe)
    }

    #[test]
    fn a_closed_popup_never_probes_a_device_but_still_applies_a_finished_pass() {
        let targets = [device("build")];
        let mut devices = Devices::default();
        assert!(!devices.poll_with(&targets, false, now(), |_| {
            unreachable!("a closed popup must not probe")
        }));
        let (tx, rx) = mpsc::sync_channel(1);
        devices.pending = Some(rx);
        devices
            .asked
            .insert("ssh:build".to_owned(), ("build.invalid".to_owned(), now()));
        tx.send(vec![(
            "ssh:build".to_owned(),
            "build.invalid".to_owned(),
            DeviceScan::Sessions(vec![RemoteSession {
                name: "agents".into(),
                running: true,
            }]),
        )])
        .unwrap();
        assert!(devices.poll_with(&targets, false, now(), |_| { unreachable!("still closed") }));
        assert!(devices.pending.is_none());
        assert!(matches!(
            devices.answers["ssh:build"],
            DeviceScan::Sessions(ref sessions) if sessions.len() == 1
        ));
    }

    #[test]
    fn a_pass_asks_every_due_device_and_leaves_the_others_alone() {
        let targets = [device("build"), device("prod")];
        let asked_at = now();
        let mut devices = Devices::default();
        // `prod` was asked a moment ago; `build` never has been.
        devices
            .asked
            .insert("ssh:prod".to_owned(), ("prod.invalid".to_owned(), asked_at));
        let (seen, probe) = recorder();
        assert!(devices.poll_with(&targets, true, asked_at, probe));
        settle(&mut devices, &targets);
        assert_eq!(
            *seen.lock().unwrap(),
            ["build.invalid"],
            "only the device whose answer has aged out is asked"
        );
        assert_eq!(devices.answers.len(), 1);
        // Inside the interval nothing is asked again, and past it both are.
        assert!(
            !devices.poll_with(&targets, true, asked_at + DEVICE_REFRESH / 2, |_| {
                unreachable!("both answers are still fresh")
            })
        );
        let (seen, probe) = recorder();
        assert!(devices.poll_with(&targets, true, asked_at + DEVICE_REFRESH, probe));
        settle(&mut devices, &targets);
        let mut both = seen.lock().unwrap().clone();
        both.sort();
        assert_eq!(both, ["build.invalid", "prod.invalid"]);
    }

    #[test]
    fn a_failed_probe_waits_out_the_interval_like_an_answer() {
        let targets = [device("build")];
        let mut devices = Devices::default();
        let asked_at = now();
        assert!(devices.poll_with(&targets, true, asked_at, |_| {
            Err(herdr_client::Error::SshTimeout)
        }));
        settle(&mut devices, &targets);
        assert!(matches!(
            devices.answers["ssh:build"],
            DeviceScan::Failed(_)
        ));
        // A host that cannot answer must not be redialled on every refresh.
        assert!(
            !devices.poll_with(&targets, true, asked_at + DEVICE_REFRESH / 2, |_| {
                unreachable!("the failure is still fresh")
            })
        );
        assert!(
            devices.poll_with(&targets, true, asked_at + DEVICE_REFRESH, |_| {
                Err(herdr_client::Error::SshTimeout)
            })
        );
    }

    #[test]
    fn a_worker_that_dies_fails_only_the_devices_it_was_asking() {
        let targets = [device("build"), device("prod")];
        let asked_at = now();
        let mut devices = Devices {
            asking: vec![device("build")],
            ..Default::default()
        };
        devices.asked.insert(
            "ssh:build".to_owned(),
            ("build.invalid".to_owned(), asked_at),
        );
        // `prod` answered a moment ago: it is not part of this pass.
        devices
            .asked
            .insert("ssh:prod".to_owned(), ("prod.invalid".to_owned(), asked_at));
        devices.answers.insert(
            "ssh:prod".to_owned(),
            DeviceScan::Sessions(vec![RemoteSession {
                name: "agents".into(),
                running: true,
            }]),
        );
        let (tx, rx) = mpsc::sync_channel::<Vec<(String, String, DeviceScan)>>(1);
        devices.pending = Some(rx);
        drop(tx);
        assert!(devices.poll_with(&targets, false, now(), |_| {
            unreachable!("a pass is outstanding")
        }));
        assert!(devices.pending.is_none());
        assert!(matches!(
            devices.answers["ssh:build"],
            DeviceScan::Failed(_)
        ));
        // The device that was not being asked keeps the answer it already had.
        assert!(matches!(
            devices.answers["ssh:prod"],
            DeviceScan::Sessions(ref sessions) if sessions.len() == 1
        ));
    }

    #[test]
    fn a_device_whose_host_changed_is_asked_again_at_once() {
        let asked_at = now();
        let mut devices = Devices::default();
        devices
            .asked
            .insert("ssh:build".to_owned(), ("old.invalid".to_owned(), asked_at));
        devices.answers.insert(
            "ssh:build".to_owned(),
            DeviceScan::Sessions(vec![RemoteSession {
                name: "agents".into(),
                running: true,
            }]),
        );
        // The same endpoint id now points at another host, inside the interval.
        let targets = [("ssh:build".to_owned(), "new.invalid".to_owned())];
        let (seen, probe) = recorder();
        assert!(devices.poll_with(&targets, true, asked_at + Duration::from_secs(1), probe));
        settle(&mut devices, &targets);
        assert_eq!(
            *seen.lock().unwrap(),
            ["new.invalid"],
            "a changed host is asked at once rather than after the old host's interval"
        );
        assert!(devices.answers.contains_key("ssh:build"));
    }

    #[test]
    fn an_answer_from_a_host_the_device_no_longer_points_at_is_dropped() {
        let asked_at = now();
        let mut devices = Devices {
            asking: vec![("ssh:build".to_owned(), "old.invalid".to_owned())],
            ..Default::default()
        };
        devices
            .asked
            .insert("ssh:build".to_owned(), ("old.invalid".to_owned(), asked_at));
        let (tx, rx) = mpsc::sync_channel::<Vec<(String, String, DeviceScan)>>(1);
        devices.pending = Some(rx);
        tx.send(vec![(
            "ssh:build".to_owned(),
            "old.invalid".to_owned(),
            DeviceScan::Sessions(vec![RemoteSession {
                name: "agents".into(),
                running: true,
            }]),
        )])
        .unwrap();
        // The device moved to another host while that probe was in flight.
        let targets = [("ssh:build".to_owned(), "new.invalid".to_owned())];
        assert!(devices.poll_with(&targets, false, asked_at, |_| {
            unreachable!("the pass is already outstanding")
        }));
        assert!(
            devices.answers.is_empty(),
            "the previous host's sessions are not this device's"
        );
    }
}
