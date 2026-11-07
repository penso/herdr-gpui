//! `herdr-automate serve`: the scheduler. One per host, held by a lock
//! file. Every two seconds it beats, takes requests from the command line,
//! starts what is due, and records what finished; every thirty it rescans
//! scripts. It never resumes a run a previous daemon left behind.

use crate::{
    Error, Result,
    fingerprint::Fingerprint,
    herdr::Herdr,
    logbook::Logbook,
    paths::{Paths, private_dir, private_file},
    runner::{self, Ended, Job, Slots},
    sampler,
    scan::{self, Found},
    settings::Settings,
    store::{NewRun, RequestKind, ScriptRow, Store, Switch},
};
use jiff::Timestamp;
use sheprd_automation::{
    Plan, Schedule, Source, due,
    report::{Reason, RunState},
};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

const TICK: Duration = Duration::from_secs(2);
const SCAN_EVERY: i64 = 30;
/// Failed runs in a row after which a script is paused.
const FAILURES_TO_PAUSE: u32 = 3;
/// How long shutdown waits for runs to stop.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(30);

pub fn version() -> &'static str {
    option_env!("HERDR_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

/// The lock that makes this the host's only daemon; released on drop.
pub struct Lock {
    _file: File,
}

impl Lock {
    pub fn acquire(paths: &Paths) -> Result<Self> {
        let path = paths.lock();
        if let Some(parent) = path.parent() {
            private_dir(parent).map_err(Error::io(parent))?;
        }
        let mut file = private_file()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(Error::io(&path))?;
        if file.try_lock().is_err() {
            let pid = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.trim().parse().ok())
                .unwrap_or(0);
            return Err(Error::AlreadyRunning { pid });
        }
        file.set_len(0).map_err(Error::io(&path))?;
        write!(file, "{}", std::process::id()).map_err(Error::io(&path))?;
        Ok(Self { _file: file })
    }

    /// Whether another process holds the lock now.
    pub fn held(paths: &Paths) -> bool {
        let Ok(file) = OpenOptions::new().read(true).write(true).open(paths.lock()) else {
            return false;
        };
        let free = file.try_lock().is_ok();
        if free {
            let _ = file.unlock();
        }
        !free
    }
}

/// A script as the daemon holds it between scans.
struct Loaded {
    source: Source,
    plan: Plan,
    fingerprint: Fingerprint,
    repo: Option<PathBuf>,
}

/// A run going on, by script path.
struct Live {
    run: i64,
    stop: Arc<AtomicBool>,
}

/// Everything the scheduler holds while it serves.
struct Daemon<'a> {
    paths: &'a Paths,
    store: Store,
    log: Logbook,
    settings: Settings,
    herdr: Herdr,
    loaded: HashMap<String, Loaded>,
    live: HashMap<String, Live>,
    slots: Arc<Slots>,
    done: mpsc::Sender<(String, i64, Result<Ended>)>,
}

pub fn serve(paths: &Paths) -> Result<()> {
    let _lock = Lock::acquire(paths)?;
    let store = Store::open(&paths.database())?;
    let log = Logbook::new(paths.log());
    let started = runner::now();
    let interrupted = store.interrupt_running(started)?;
    log.info(format!(
        "daemon {} started, pid {}",
        version(),
        std::process::id()
    ));
    if interrupted > 0 {
        log.warn(format!(
            "{interrupted} run(s) were interrupted by the previous daemon's exit"
        ));
    }
    let settings = Settings::read(&paths.settings())?;
    let herdr = Herdr::new(settings.herdr.clone(), settings.herdr_session.clone());
    let stop_sampler = Arc::new(AtomicBool::new(false));
    let sampler = {
        let (paths, herdr, log, stop) = (
            paths.clone(),
            herdr.clone(),
            log.clone(),
            stop_sampler.clone(),
        );
        thread::spawn(move || sampler::run(&paths, &herdr, &log, &stop))
    };
    let (done, finished) = mpsc::channel();
    let mut daemon = Daemon {
        paths,
        store,
        log,
        settings,
        herdr,
        loaded: HashMap::new(),
        live: HashMap::new(),
        slots: Arc::default(),
        done,
    };
    let served = daemon.serve(started, &finished);
    daemon.shut_down(&finished)?;
    stop_sampler.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    daemon.store.clear_beat()?;
    served
}

impl Daemon<'_> {
    fn serve(
        &mut self,
        started: i64,
        finished: &mpsc::Receiver<(String, i64, Result<Ended>)>,
    ) -> Result<()> {
        let mut next_scan = 0;
        loop {
            let now = runner::now();
            self.store
                .beat(std::process::id(), version(), started, now)?;
            if now >= next_scan {
                match Settings::read(&self.paths.settings()) {
                    Ok(read) => self.settings = read,
                    Err(error) => self
                        .log
                        .warn(format!("settings: {error}; keeping the previous ones")),
                }
                self.refresh(now)?;
                next_scan = now + SCAN_EVERY;
            }
            if self.handle_requests(now)? {
                return Ok(());
            }
            let paused_all = self.store.paused_all()?;
            let paths = self.loaded.keys().cloned().collect::<Vec<_>>();
            for path in paths {
                self.schedule(&path, paused_all, now)?;
            }
            while let Ok((path, run, ended)) = finished.try_recv() {
                self.live.remove(&path);
                self.finish(&path, run, ended)?;
            }
            thread::sleep(TICK);
        }
    }

    /// Acts on the requests commands left; whether one asked the daemon to
    /// shut down.
    fn handle_requests(&mut self, now: i64) -> Result<bool> {
        let requests = self.store.take_requests(now)?;
        // A run-now often follows an approval or an edit by seconds: read
        // the scripts again first, rather than act on a copy up to a scan
        // old that lacks the script or has its old hash.
        if requests
            .iter()
            .any(|request| matches!(request.kind, RequestKind::RunNow { .. }))
        {
            self.refresh(now)?;
        }
        for request in requests {
            match request.kind {
                RequestKind::RunNow { path, trigger } => {
                    match self.manual_trigger(&path, trigger.as_deref()) {
                        Some(index) => self.start(&path, index, Reason::Manual, None),
                        None => self.log.warn(format!(
                            "run-now: {path} has no such trigger or is not loaded"
                        )),
                    }
                }
                RequestKind::Stop { run } => {
                    if let Some(found) = self.live.values().find(|found| found.run == run) {
                        found.stop.store(true, Ordering::Relaxed);
                        self.log.info(format!("stopping run {run}"));
                    }
                }
                RequestKind::Shutdown => return Ok(true),
            }
        }
        Ok(false)
    }

    /// Asks every run to stop and waits a while for them.
    fn shut_down(&mut self, finished: &mpsc::Receiver<(String, i64, Result<Ended>)>) -> Result<()> {
        self.log.info("daemon stopping");
        for found in self.live.values() {
            found.stop.store(true, Ordering::Relaxed);
        }
        let deadline = std::time::Instant::now() + SHUTDOWN_WAIT;
        while !self.live.is_empty() && std::time::Instant::now() < deadline {
            while let Ok((path, run, ended)) = finished.try_recv() {
                self.live.remove(&path);
                self.finish(&path, run, ended)?;
            }
            thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    }

    /// Rereads every script, planning only those whose text changed.
    fn refresh(&mut self, now: i64) -> Result<()> {
        let found = scan::find(self.paths, &self.store)?;
        let mut kept = HashMap::new();
        for Found { path, repo } in found {
            let key = path.to_string_lossy().into_owned();
            let repo_text = repo
                .as_ref()
                .map(|repo| repo.to_string_lossy().into_owned());
            let source = match Source::read(&path) {
                Ok(source) => source,
                Err(error) => {
                    self.store.put_script(&ScriptRow {
                        path: key,
                        name: path
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        repo: repo_text,
                        error: Some(error.to_string()),
                        seen_at: now,
                        ..ScriptRow::default()
                    })?;
                    continue;
                }
            };
            if let Some(known) = self.loaded.remove(&key)
                && known.source.hash == source.hash
            {
                // The script is unchanged, but the worktree scripts it runs
                // may not be: a new fingerprint needs approval again.
                let fingerprint = Fingerprint::of(&source, &known.plan, repo.as_deref());
                let row = if fingerprint == known.fingerprint {
                    ScriptRow {
                        repo: repo_text,
                        seen_at: now,
                        ..self.store.script(&key)?.unwrap_or_default()
                    }
                } else {
                    scan::row(
                        &source,
                        Ok(&known.plan),
                        Some(&fingerprint),
                        key.clone(),
                        repo.as_deref(),
                        now,
                    )
                };
                self.store.put_script(&row)?;
                kept.insert(
                    key,
                    Loaded {
                        repo,
                        fingerprint,
                        ..known
                    },
                );
                continue;
            }
            let plan = Plan::build(&source, sheprd_automation::Limits::default());
            let fingerprint = plan
                .as_ref()
                .ok()
                .map(|plan| Fingerprint::of(&source, plan, repo.as_deref()));
            let row = scan::row(
                &source,
                plan.as_ref(),
                fingerprint.as_ref(),
                key.clone(),
                repo.as_deref(),
                now,
            );
            if let Some(error) = &row.error {
                self.log.warn(format!("{}: {error}", source.name));
            }
            self.store.put_script(&row)?;
            if let (Ok(plan), Some(fingerprint)) = (plan, fingerprint) {
                kept.insert(
                    key,
                    Loaded {
                        source,
                        plan,
                        fingerprint,
                        repo,
                    },
                );
            }
        }
        self.loaded = kept;
        self.store.forget_scripts_before(now)?;
        Ok(())
    }

    /// Starts any due trigger of an approved, active script, and records
    /// when each fires next.
    fn schedule(&mut self, path: &str, paused_all: bool, now: i64) -> Result<()> {
        let Some(script) = self.loaded.get(path) else {
            return Ok(());
        };
        let switch = self.store.switch(path)?;
        let approved =
            self.store.approved_hash(path)?.as_deref() == Some(script.fingerprint.hash.as_str());
        let active = approved && !switch.paused && !switch.disabled && !paused_all;
        let mut statuses = scan::triggers(&script.plan);
        let mut due_now = Vec::new();
        let at = |second: i64| Timestamp::from_second(second).unwrap_or(Timestamp::UNIX_EPOCH);
        for (index, trigger) in script.plan.triggers.iter().enumerate() {
            if matches!(trigger.schedule, Schedule::Manual) {
                continue;
            }
            let zone = trigger.zone.time_zone();
            let Some((checked, fired)) = self.store.cursor(path, index)?.filter(|_| active) else {
                // Inactive, or never checked: count from now, so a script
                // fires nothing for the time it was off. Now is also the
                // anchor an interval counts from until its first firing;
                // `checked` moves every tick, so it cannot be.
                self.store.set_cursor(path, index, now, Some(now))?;
                continue;
            };
            let found = due(
                &trigger.schedule,
                &zone,
                at(checked),
                fired.map(at),
                at(now),
            );
            let fired_now = found.map(|due| due.scheduled.as_second());
            self.store.set_cursor(path, index, now, fired_now)?;
            if let Some(due) = found {
                let reason = if due.late {
                    Reason::Late
                } else {
                    Reason::Schedule
                };
                due_now.push((index, reason, due.scheduled.as_second()));
            }
            if let Some(status) = statuses.get_mut(index) {
                status.next = trigger
                    .schedule
                    .next_after(&zone, at(now), fired_now.or(fired).map(at))
                    .map(Timestamp::as_second);
            }
        }
        self.store.set_triggers(path, &statuses)?;
        for (index, reason, scheduled) in due_now {
            self.start(path, index, reason, Some(scheduled));
        }
        Ok(())
    }

    /// Starts trigger `index` of the script at `path` on its own thread.
    fn start(&mut self, path: &str, index: usize, reason: Reason, scheduled: Option<i64>) {
        let Some(script) = self.loaded.get(path) else {
            return;
        };
        let name = &script.source.name;
        if self.live.contains_key(path) {
            self.log
                .info(format!("{name}: skipped; the previous run is still going"));
            return;
        }
        // Fingerprinted once, now: the run gets exactly the worktree
        // scripts whose hash matched the approval.
        let fingerprint = Fingerprint::of(&script.source, &script.plan, script.repo.as_deref());
        match self.store.approved_hash(path) {
            Ok(Some(hash)) if hash == fingerprint.hash => {}
            _ => {
                self.log.warn(format!(
                    "{name}: not approved in its current form; not running it"
                ));
                return;
            }
        }
        let Some(trigger) = script.plan.triggers.get(index) else {
            return;
        };
        let local = Timestamp::from_second(scheduled.unwrap_or_else(runner::now))
            .unwrap_or(Timestamp::UNIX_EPOCH)
            .to_zoned(trigger.zone.time_zone());
        let new = NewRun {
            path: path.to_owned(),
            name: name.clone(),
            trigger: index,
            trigger_name: trigger.name.clone(),
            reason,
            scheduled,
            hash: fingerprint.hash.clone(),
            started_at: runner::now(),
        };
        let run = match self.store.start_run(&new) {
            Ok(Some(run)) => run,
            Ok(None) => return,
            Err(error) => {
                self.log
                    .error(format!("{name}: could not record the run: {error}"));
                return;
            }
        };
        self.log.info(format!(
            "{name}: run {run} started ({})",
            crate::store::reason_label(reason)
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let job = Job {
            run,
            path: path.to_owned(),
            repo: script.repo.clone(),
            source: script.source.clone(),
            worktree: fingerprint.worktree,
            trigger: index,
            date: local.strftime("%Y-%m-%d").to_string(),
            time: local.strftime("%H:%M").to_string(),
            stop: stop.clone(),
        };
        self.live.insert(path.to_owned(), Live { run, stop });
        let (paths, settings, herdr, slots, done) = (
            self.paths.clone(),
            self.settings.clone(),
            self.herdr.clone(),
            self.slots.clone(),
            self.done.clone(),
        );
        thread::spawn(move || {
            let ended = runner::execute(&paths, &settings, &herdr, &slots, &job);
            let _ = done.send((job.path.clone(), job.run, ended));
        });
    }

    fn finish(&self, path: &str, run: i64, ended: Result<Ended>) -> Result<()> {
        finish(&self.store, path, run, ended, &self.log)
    }

    /// The trigger a manual run starts: by name, by number from 1, or the
    /// first manual trigger, else the first trigger.
    fn manual_trigger(&self, path: &str, wanted: Option<&str>) -> Option<usize> {
        manual_trigger(&self.loaded.get(path)?.plan, wanted)
    }
}

fn finish(store: &Store, path: &str, run: i64, ended: Result<Ended>, log: &Logbook) -> Result<()> {
    let ended = ended.unwrap_or_else(|error| Ended {
        state: RunState::Failed,
        detail: Some(error.to_string()),
    });
    store.finish_run(run, ended.state, ended.detail.as_deref(), runner::now())?;
    log.info(format!(
        "run {run} ended: {:?}{}",
        ended.state,
        ended
            .detail
            .as_deref()
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default()
    ));
    if store.failures_in_a_row(path)? >= FAILURES_TO_PAUSE {
        let mut switch = store.switch(path)?;
        if !switch.paused {
            switch = Switch {
                paused: true,
                reason: Some(format!(
                    "paused after {FAILURES_TO_PAUSE} failed runs in a row"
                )),
                ..switch
            };
            store.set_switch(path, &switch)?;
            log.warn(format!(
                "{path}: {}",
                switch.reason.as_deref().unwrap_or_default()
            ));
        }
    }
    Ok(())
}

/// The trigger a manual run starts: by name, by number from 1, or the first
/// manual trigger, else the first trigger.
pub(crate) fn manual_trigger(plan: &Plan, wanted: Option<&str>) -> Option<usize> {
    let triggers = &plan.triggers;
    match wanted {
        Some(wanted) => triggers
            .iter()
            .position(|trigger| trigger.name.as_deref() == Some(wanted))
            .or_else(|| {
                wanted
                    .parse::<usize>()
                    .ok()
                    .and_then(|number| number.checked_sub(1))
                    .filter(|index| *index < triggers.len())
            }),
        None => triggers
            .iter()
            .position(|trigger| matches!(trigger.schedule, Schedule::Manual))
            .or((!triggers.is_empty()).then_some(0)),
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests;
