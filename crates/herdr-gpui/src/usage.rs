//! Plan usage of the AI services signed in on the selected host, for the
//! status bar and its panel. One worker thread reads one host at a time and
//! reports each provider as it answers; the UI thread only queues a host and
//! takes answers on later ticks. Each host keeps its last answers, so
//! switching back shows them at once and a failed refresh keeps the numbers it
//! had, marked stale, rather than blanking them.

mod cookies;
mod icons;
mod model;
mod panel;
mod probe;
mod providers;
mod registry;
mod render;
mod service;
mod settings;
mod ui;
mod values;

#[cfg(test)]
mod tests;

pub(crate) use model::{Host, Provider};
pub(crate) use panel::PANEL_WIDTH;
pub use settings::UsageConfig;

use cookies::CookieJar;
use model::Report;
use probe::{Exec, Probe, Shell};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant, SystemTime},
};

/// The services rate limit these endpoints, and the numbers move slowly.
const REFRESH: Duration = Duration::from_secs(10 * 60);
const RATE_LIMITED: Duration = Duration::from_secs(30 * 60);
/// A host that could not be reached is retried sooner than a full refresh.
const ERROR_BACKOFF: Duration = Duration::from_secs(5 * 60);
/// A click cannot queue reads back to back.
const MANUAL_SPACING: Duration = Duration::from_secs(10);
/// Hosts are few; an unbounded catalog still cannot grow the cache past this.
const HOST_LIMIT: usize = 16;

pub(crate) fn icon(path: &str) -> Option<&'static [u8]> {
    icons::PROVIDER_ICONS
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

pub(crate) fn icon_paths() -> impl Iterator<Item = &'static str> {
    icons::PROVIDER_ICONS.iter().map(|(name, _)| *name)
}

/// One provider on the shown host: its last good report, and why the latest
/// refresh failed if it did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Reading {
    pub provider: Provider,
    pub report: Option<Report>,
    pub error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Entry {
    pub readings: Vec<Reading>,
    /// Why the host itself could not be read, such as SSH failing.
    pub error: Option<String>,
    pub updated: Option<SystemTime>,
    due: Option<Instant>,
    requested: Option<Instant>,
    /// Providers heard from in the read under way, so ones that went
    /// silent (signed out) can be dropped when it ends.
    round: HashSet<Provider>,
    rate_limited: bool,
}

/// How many providers the status bar has room for; the panel lists all.
pub(crate) const HEADLINE: usize = 2;

impl Entry {
    /// The providers the status bar shows: those with numbers to show, the
    /// ones closest to a limit first, at most `limit` of them. A provider
    /// that has no report yet, or no windows or balances, waits in the panel.
    pub fn headline(&self, limit: usize) -> Vec<&Reading> {
        let mut shown: Vec<&Reading> = self
            .readings
            .iter()
            .filter(|reading| has_numbers(reading))
            .collect();
        // Stable: equally used providers keep the registry's order.
        shown.sort_by(|a, b| urgency(b).total_cmp(&urgency(a)));
        shown.truncate(limit);
        shown
    }
}

impl Entry {
    /// The providers the panel offers as tabs: those with numbers to show,
    /// and those the config asked for, whose panel then says what to set up.
    /// A detected sign-in that yields nothing, such as an account without a
    /// plan, is left out.
    pub fn tabs(&self, config: &UsageConfig) -> Vec<&Reading> {
        self.readings
            .iter()
            .filter(|reading| has_numbers(reading) || config.shown(reading.provider))
            .collect()
    }
}

fn has_numbers(reading: &Reading) -> bool {
    reading
        .report
        .as_ref()
        .is_some_and(|report| !report.windows.is_empty() || !report.balances.is_empty())
}

fn urgency(reading: &Reading) -> f32 {
    reading
        .report
        .as_ref()
        .and_then(|report| report.tightest())
        .map_or(0., |window| window.used)
}

enum Message {
    Reading(Host, Provider, crate::Result<Report>),
    Done(Host, crate::Result<()>),
}

struct Worker {
    requests: mpsc::SyncSender<(Host, Arc<UsageConfig>)>,
    results: mpsc::Receiver<Message>,
}

#[derive(Default)]
pub(crate) struct Usage {
    host: Option<Host>,
    entries: HashMap<Host, Entry>,
    worker: Option<Worker>,
    busy: Option<Host>,
    minute: u64,
    revision: Option<u64>,
}

impl Usage {
    /// What the status bar shows for the tracked host.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.host.as_ref()?)
    }

    /// Whether the tracked host is being read right now.
    pub fn busy(&self) -> bool {
        self.host.is_some() && self.busy == self.host
    }

    /// Reads the tracked host on the next poll, unless it was just read.
    pub fn refresh(&mut self, now: Instant) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let entry = self.entries.entry(host).or_default();
        if entry
            .requested
            .is_none_or(|requested| now.duration_since(requested) >= MANUAL_SPACING)
        {
            entry.due = Some(now);
        }
    }

    /// Follows `host` (None hides usage), takes finished answers, and starts
    /// the next read when it is due. Reads only start while `active`, so a
    /// background window costs no requests; a new config `revision` makes
    /// every host due. Returns whether anything shown changed, including the
    /// minute the reset countdowns count from.
    pub fn poll(
        &mut self,
        host: Option<Host>,
        config: &UsageConfig,
        revision: u64,
        active: bool,
        now: Instant,
    ) -> bool {
        let mut changed = self.host != host;
        self.host = host;
        if self
            .revision
            .replace(revision)
            .is_some_and(|last| last != revision)
        {
            for entry in self.entries.values_mut() {
                entry.due = Some(now);
            }
        }
        while let Some(worker) = &self.worker {
            match worker.results.try_recv() {
                Ok(message) => {
                    self.apply(message, now);
                    changed = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.busy = None;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        let minute = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 60;
        if minute != self.minute {
            self.minute = minute;
            changed |= self
                .current()
                .is_some_and(|entry| !entry.readings.is_empty());
        }
        if active && self.busy.is_none() {
            changed |= self.dispatch(config, now);
        }
        changed
    }

    fn dispatch(&mut self, config: &UsageConfig, now: Instant) -> bool {
        let Some(host) = self.host.clone() else {
            return false;
        };
        if self
            .entries
            .get(&host)
            .and_then(|entry| entry.due)
            .is_some_and(|due| due > now)
        {
            return false;
        }
        if self.worker.is_none() {
            self.worker = spawn();
        }
        let Some(worker) = &self.worker else {
            self.entries.entry(host).or_default().due = Some(now + ERROR_BACKOFF);
            return false;
        };
        match worker
            .requests
            .try_send((host.clone(), Arc::new(config.clone())))
        {
            Ok(()) => {
                self.begin(host, now);
                true
            }
            Err(mpsc::TrySendError::Full(_)) => false,
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.worker = None;
                false
            }
        }
    }

    /// Marks `host` as being read, making room for it in the cache.
    fn begin(&mut self, host: Host, now: Instant) {
        if self.entries.len() >= HOST_LIMIT && !self.entries.contains_key(&host) {
            let shown = self.host.clone();
            self.entries
                .retain(|kept, _| *kept == host || Some(kept) == shown.as_ref());
        }
        let entry = self.entries.entry(host.clone()).or_default();
        entry.requested = Some(now);
        entry.round.clear();
        entry.rate_limited = false;
        // Due again only once this read answers.
        entry.due = Some(now + REFRESH);
        self.busy = Some(host);
    }

    fn apply(&mut self, message: Message, now: Instant) {
        match message {
            Message::Reading(host, provider, report) => {
                let Some(entry) = self.entries.get_mut(&host) else {
                    return;
                };
                entry.round.insert(provider);
                let previous = entry
                    .readings
                    .iter()
                    .position(|reading| reading.provider == provider);
                let reading = match report {
                    Ok(report) => Reading {
                        provider,
                        report: Some(report),
                        error: None,
                    },
                    Err(error) => {
                        entry.rate_limited |= matches!(error, crate::Error::UsageRateLimited);
                        Reading {
                            provider,
                            report: previous.and_then(|index| entry.readings[index].report.clone()),
                            error: Some(error.to_string()),
                        }
                    }
                };
                match previous {
                    Some(index) => entry.readings[index] = reading,
                    None => {
                        entry.readings.push(reading);
                        entry
                            .readings
                            .sort_by_key(|reading| registry::position(reading.provider));
                    }
                }
            }
            Message::Done(host, result) => {
                if self.busy.as_ref() == Some(&host) {
                    self.busy = None;
                }
                let Some(entry) = self.entries.get_mut(&host) else {
                    return;
                };
                match result {
                    Ok(()) => {
                        let heard = std::mem::take(&mut entry.round);
                        // A provider that stopped answering has been signed out.
                        entry
                            .readings
                            .retain(|reading| heard.contains(&reading.provider));
                        entry.error = None;
                        entry.updated = Some(SystemTime::now());
                        entry.due = Some(
                            now + if entry.rate_limited {
                                RATE_LIMITED
                            } else {
                                REFRESH
                            },
                        );
                    }
                    Err(error) => {
                        entry.round.clear();
                        entry.error = Some(error.to_string());
                        entry.due = Some(now + ERROR_BACKOFF);
                    }
                }
            }
        }
    }
}

fn spawn() -> Option<Worker> {
    let (requests, incoming) = mpsc::sync_channel::<(Host, Arc<UsageConfig>)>(1);
    let (outgoing, results) = mpsc::sync_channel(256);
    let spawned = thread::Builder::new()
        .name("herdr-usage".into())
        .spawn(move || {
            // Browser cookie keys are kept for the worker's life, so a
            // browser asks for Keychain access once.
            let mut cookies = CookieJar::default();
            for (host, config) in incoming {
                let done = read(&host, &config, &mut cookies, |provider, report| {
                    outgoing
                        .send(Message::Reading(host.clone(), provider, report))
                        .is_ok()
                });
                if outgoing.send(Message::Done(host, done)).is_err() {
                    break;
                }
            }
        });
    match spawned {
        Ok(_) => Some(Worker { requests, results }),
        Err(error) => {
            tracing::warn!(category = "usage", %error, "could not start the usage worker");
            None
        }
    }
}

/// Reads every provider on `host`, reporting each through `report` as it
/// answers. A provider the config lists but the host has no sign-in for
/// answers with what to set up.
fn read(
    host: &Host,
    config: &UsageConfig,
    cookies: &mut CookieJar,
    mut report: impl FnMut(Provider, crate::Result<Report>) -> bool,
) -> crate::Result<()> {
    let mut exec = match host {
        Host::Local => Exec::Local,
        Host::Ssh(target) => Exec::Remote(Shell::connect(target)?),
    };
    for provider in registry::all() {
        if config.hidden(provider) {
            continue;
        }
        let requested = config.shown(provider);
        let mut probe = Probe::new(
            &mut exec,
            provider,
            config.settings(provider),
            cookies,
            requested && config.browser_cookies,
        );
        let answer = match provider.service().fetch(&mut probe) {
            Some(answer) => answer,
            None if requested => Err(crate::Error::UsageNotSignedIn),
            None => continue,
        };
        if !report(provider, answer) {
            break;
        }
    }
    Ok(())
}
