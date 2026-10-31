//! Mods (`docs/mods-plan.md`): sandboxed Luau folders under
//! `<config>/herdr/gpui-mods/` that add status bar items, sidebar sections,
//! and palette commands. `herdr-mods` loads and checks them; this module runs
//! each on a worker thread of its own and draws the trees they return.
//!
//! The UI thread never runs Lua or reads a mod's files. On each tick it
//! offers the window's state when it changed, passes on clicks, and takes
//! what workers left in their mailboxes: the latest tree, the latest error,
//! log lines, and the catalog commands a click asked for. A scanner thread
//! lists the folder, so a new mod starts and a removed one stops by itself.

mod activity;
mod config;
mod error;
mod panel;
mod render;
mod scan;
mod state;
mod worker;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub(crate) use config::Config;
pub(crate) use error::Error;

use crate::HerdrWindow;
use herdr_mods::{Data, LogLine, Manifest, Node, Pace, State};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the window's state is offered to mods at most; they run again
/// only when a topic they watch changed.
const STATE_GAP: Duration = Duration::from_millis(250);
/// Mods run at once, whatever the folder holds: the first by name of those
/// not turned off.
const MAX_MODS: usize = 32;
/// Log lines kept per mod for `herdr-gpui mod logs`.
const LOG_LINES: usize = 200;

/// Every mod this window runs.
pub(crate) struct Mods {
    dir: Option<PathBuf>,
    scanner: Option<scan::Scanner>,
    entries: BTreeMap<String, Entry>,
    offered: Option<Arc<State>>,
    pace: Pace,
    /// The right-hand panel's sections and motion; see `panel`.
    panel: panel::PanelState,
}

impl Default for Mods {
    fn default() -> Self {
        // Unit tests never run the developer's own mods.
        let dir = crate::config::Config::mods_dir()
            .ok()
            .filter(|_| !cfg!(test));
        Self {
            dir,
            scanner: None,
            entries: BTreeMap::new(),
            offered: None,
            pace: Pace::new(STATE_GAP),
            panel: panel::PanelState::default(),
        }
    }
}

/// One mod's worker and what it last reported.
pub(crate) struct Entry {
    worker: worker::Handle,
    pub(crate) manifest: Option<Manifest>,
    /// The activity bar icon's SVG text, when the mod declares `[activity]`.
    pub(crate) icon: Option<String>,
    /// That icon tinted for the bar, by the color it was drawn in.
    icon_image: std::cell::RefCell<Option<(u32, Arc<gpui::Image>)>>,
    /// What it last drew; `None` while loading or when it draws nothing.
    pub(crate) tree: Option<Node>,
    /// It drew at least once, possibly nothing.
    pub(crate) drawn: bool,
    /// Why the last `update` or `view` failed.
    call_error: Option<herdr_mods::Error>,
    /// Why the mod's files last failed to load; the last good mod runs on.
    load_error: Option<herdr_mods::Error>,
    pub(crate) log: VecDeque<LogLine>,
}

/// What a poll found.
#[derive(Debug, Default)]
pub(crate) struct Polled {
    /// Something drawn changed.
    pub changed: bool,
    /// Catalog commands clicks asked for, in order.
    pub commands: Vec<herdr_commands::Command>,
}

impl Mods {
    #[cfg(test)]
    pub(crate) fn in_dir(dir: PathBuf) -> Self {
        Self {
            dir: Some(dir),
            ..Self::default()
        }
    }

    /// Adds a mod that already drew `tree` (nothing, when `None`), for
    /// layout tests. Its worker watches a folder that does not exist, so it
    /// never draws again.
    #[cfg(test)]
    pub(crate) fn insert_drawn(&mut self, manifest: Manifest, tree: Option<Node>) {
        let missing = std::env::temp_dir()
            .join("herdr-mods-test-missing")
            .join(&manifest.name);
        let Ok(worker) = worker::Handle::spawn(missing) else {
            return;
        };
        self.entries.insert(
            manifest.name.clone(),
            Entry {
                worker,
                manifest: Some(manifest),
                icon: None,
                icon_image: Default::default(),
                tree,
                drawn: true,
                call_error: None,
                load_error: None,
                log: VecDeque::new(),
            },
        );
    }

    /// Starts and stops workers to match the folder and `config`, and takes
    /// what they reported.
    pub(crate) fn poll(&mut self, config: &Config) -> Polled {
        let mut polled = Polled::default();
        let names = self.wanted(config);
        let before = self.entries.len();
        self.entries.retain(|name, _| names.contains(name));
        polled.changed |= self.entries.len() != before;
        for name in names {
            if self.entries.contains_key(&name) {
                continue;
            }
            let Some(dir) = self.dir.as_ref().map(|dir| dir.join(&name)) else {
                continue;
            };
            match worker::Handle::spawn(dir) {
                Ok(worker) => {
                    if let Some(state) = &self.offered {
                        worker.offer(Arc::clone(state));
                    }
                    self.entries.insert(
                        name,
                        Entry {
                            worker,
                            manifest: None,
                            icon: None,
                            icon_image: Default::default(),
                            tree: None,
                            drawn: false,
                            call_error: None,
                            load_error: None,
                            log: VecDeque::new(),
                        },
                    );
                }
                Err(error) => tracing::warn!(mod_name = %name, %error, "could not start mod"),
            }
        }
        for entry in self.entries.values_mut() {
            let report = entry.worker.take();
            polled.changed |= report.changed();
            if let Some(icon) = report.icon {
                entry.icon = icon;
                entry.icon_image.replace(None);
            }
            if let Some(manifest) = report.manifest {
                entry.manifest = Some(manifest);
            }
            if let Some(tree) = report.tree {
                entry.tree = tree;
                entry.drawn = true;
            }
            if let Some(error) = report.load_error {
                entry.load_error = error;
            }
            if let Some(error) = report.error {
                entry.call_error = error;
            }
            for line in report.log {
                if entry.log.len() == LOG_LINES {
                    entry.log.pop_front();
                }
                entry.log.push_back(line);
            }
            polled.commands.extend(report.commands);
        }
        polled
    }

    /// Whether to build the window's state now: some mod runs and the last
    /// offer is old enough.
    pub(crate) fn wants_state(&self, now: Instant) -> bool {
        !self.entries.is_empty() && self.pace.wait(now).is_zero()
    }

    /// Offers the window's state to every worker, if it changed.
    pub(crate) fn offer(&mut self, state: State, now: Instant) {
        if !self.pace.take(now) || self.offered.as_deref() == Some(&state) {
            return;
        }
        let state = Arc::new(state);
        for entry in self.entries.values() {
            entry.worker.offer(Arc::clone(&state));
        }
        self.offered = Some(state);
    }

    /// Sends a message to a mod's `update`, as a click or palette command.
    pub(crate) fn send(&self, name: &str, msg: Data) -> Result<(), Error> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| Error::NotRunning(name.to_owned()))?;
        entry.worker.send(msg)
    }

    /// Reads a mod's files again now rather than at its next check.
    pub(crate) fn reload(&self, name: &str) -> Result<(), Error> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| Error::NotRunning(name.to_owned()))?;
        entry.worker.reload();
        Ok(())
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry))
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    pub(crate) fn dir(&self) -> Option<&std::path::Path> {
        self.dir.as_deref()
    }

    /// The mods to run: the folder's, minus those turned off. The scanner
    /// starts on first use and stops when mods are turned off.
    fn wanted(&mut self, config: &Config) -> BTreeSet<String> {
        if !config.enabled {
            self.scanner = None;
            return BTreeSet::new();
        }
        let Some(dir) = &self.dir else {
            return BTreeSet::new();
        };
        let scanner = self
            .scanner
            .get_or_insert_with(|| scan::Scanner::spawn(dir.clone()));
        scanner
            .names()
            .into_iter()
            .filter(|name| !config.disabled.contains(name))
            .take(MAX_MODS)
            .collect()
    }
}

impl HerdrWindow {
    /// Runs on every tick: offers state, takes reports, and runs the
    /// commands clicks asked for.
    pub(crate) fn poll_mods(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        let now = Instant::now();
        if self.update_mod_panel() {
            cx.notify();
        }
        if self.mods.wants_state(now) {
            let state = self.mods_state();
            self.mods.offer(state, now);
        }
        let polled = self.mods.poll(&self.config.mods);
        for command in polled.commands {
            self.run_mod_command(command, window, cx);
        }
        if polled.changed {
            cx.notify();
        }
    }
}

impl Entry {
    /// What is wrong with the mod now: its files first, since a broken save
    /// matters more than the old mod's last call.
    pub(crate) fn error(&self) -> Option<&herdr_mods::Error> {
        self.load_error.as_ref().or(self.call_error.as_ref())
    }

    /// The activity bar app the mod declares, once it loaded.
    pub(crate) fn activity(&self) -> Option<&herdr_mods::Activity> {
        self.manifest.as_ref()?.activity.as_ref()
    }

    pub(crate) fn places(&self) -> &[herdr_mods::Place] {
        self.manifest
            .as_ref()
            .map_or(&[], |manifest| manifest.places.as_slice())
    }
}
