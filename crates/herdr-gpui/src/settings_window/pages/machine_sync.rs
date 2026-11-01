//! Machine sync: this computer's address and name in the synced list,
//! turning sync on once the user has read what it changes on each machine,
//! the saved devices that take part, and how the last sync went. The work is
//! `crate::machine_sync`'s; this page only asks it.
// i18n: migrated

use super::{SettingsWindow, ui};
use crate::direction::Logical;
use crate::{
    i18n::{Localize, fl},
    machine_sync::{MachineSync, Outcome, Suggestion},
    notifications::safe_text,
    search_input::SearchInput,
    toggles::checkbox,
};
use gpui::{prelude::*, *};
use herdr_client::{
    SavedHost,
    machine_sync::{MachineReport, SyncState},
};
use std::time::SystemTime;

/// The page's state while it shows.
#[derive(Default)]
pub(in crate::settings_window) struct MachineSyncSettings {
    pub(in crate::settings_window) address: Option<Entity<SearchInput>>,
    pub(in crate::settings_window) name: Option<Entity<SearchInput>>,
    /// Turning sync on waits for the user to confirm what it changes.
    pub(in crate::settings_window) confirming: bool,
    /// The address or name typed could not be saved, and why.
    pub(in crate::settings_window) invalid: Option<String>,
    /// What the fields were last filled with, saved or guessed, so the saved
    /// state or a better guess arriving later replaces only what the user
    /// has not edited.
    filled: Option<Suggestion>,
    /// Watches for the saved state or a better guess while the page shows.
    watch: Option<Subscription>,
}

/// What the page shows of [`MachineSync`], copied out so the page can hand
/// its context to the controls it builds.
struct Snapshot {
    state: Option<SyncState>,
    hosts: Vec<SavedHost>,
    loaded: bool,
    running: bool,
    status: String,
    /// How the last sync went on each machine, by profile ID.
    lines: Vec<(String, String)>,
    /// The devices the last sync saved here, as one line.
    learned: Option<String>,
}

impl Snapshot {
    fn of(sync: &MachineSync) -> Self {
        let on = sync.state().is_some();
        let running = sync.running();
        let report = match sync.last() {
            Some(Outcome::Done { report, .. }) => Some(report),
            _ => None,
        };
        Self {
            state: sync.state().cloned(),
            hosts: sync.hosts().to_vec(),
            loaded: sync.loaded(),
            running,
            status: status(sync, on, running),
            lines: report
                .map(|report| {
                    report
                        .machines
                        .iter()
                        .map(|machine| (machine.id.clone(), machine_line(machine)))
                        .collect()
                })
                .unwrap_or_default(),
            learned: report
                .filter(|report| !report.learned.is_empty())
                .map(|report| {
                    report
                        .learned
                        .iter()
                        .map(|host| safe_text(&host.label, 64))
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
        }
    }
}

/// What the fields show: the saved address and name while sync is on, or
/// the best guess at this computer's while it is off.
fn fill(sync: &MachineSync) -> Option<Suggestion> {
    match sync.state() {
        Some(state) => Some(Suggestion {
            address: state.target.clone(),
            name: state.label.clone(),
        }),
        None => sync.suggestion().cloned(),
    }
}

/// The switch row's line: why it cannot work, what runs, or the last sync.
fn status(sync: &MachineSync, on: bool, running: bool) -> String {
    match (sync.failure(), sync.last(), running) {
        (Some(error), _, _) => fl!(
            "settings-machine-sync-failure",
            error = safe_text(&error.localize(), 300),
        ),
        (None, _, true) => fl!("settings-machine-sync-running"),
        (None, _, false) if !on => fl!("settings-machine-sync-off"),
        (None, Some(Outcome::Failed(error)), false) => fl!(
            "settings-machine-sync-failed",
            error = safe_text(&error.localize(), 300),
        ),
        (None, Some(Outcome::Done { report, at }), false) => {
            let seconds = SystemTime::now()
                .duration_since(*at)
                .map_or(0, |elapsed| elapsed.as_secs());
            fl!(
                "settings-machine-sync-done",
                ago = crate::ago::text(seconds),
                reached = report
                    .machines
                    .iter()
                    .filter(|machine| machine.result.is_ok())
                    .count(),
                machines = report.machines.len(),
            )
        }
        (None, None, false) => fl!("settings-machine-sync-waiting"),
    }
}

/// How the last sync went on one machine.
fn machine_line(report: &MachineReport) -> String {
    match &report.result {
        Ok(change) => fl!(
            "settings-machine-sync-machine-done",
            keys = change.keys,
            devices = change.devices,
        ),
        Err(error) => fl!(
            "settings-machine-sync-machine-failed",
            error = safe_text(&error.localize(), 300),
        ),
    }
}

impl SettingsWindow {
    /// The page shows: reread the state, and make the fields.
    pub(in crate::settings_window) fn open_machine_sync(&mut self, cx: &mut Context<Self>) {
        MachineSync::refresh(cx);
        self.make_machine_sync_fields(cx);
        if cx.has_global::<MachineSync>() {
            self.machines.watch =
                Some(cx.observe_global::<MachineSync>(Self::update_machine_sync_suggestion));
        }
    }

    /// The saved state finished loading, or a suggestion arrived, such as
    /// this computer's Tailscale name: it replaces each field the user has
    /// not changed from what it was filled with. The saved state wins over
    /// any guess, so one shown before the read finished is never saved.
    pub(in crate::settings_window) fn update_machine_sync_suggestion(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(new) = cx.try_global::<MachineSync>().and_then(fill) else {
            return;
        };
        let Some(old) = self.machines.filled.replace(new.clone()) else {
            return;
        };
        for (input, old, new) in [
            (&self.machines.address, &old.address, &new.address),
            (&self.machines.name, &old.name, &new.name),
        ] {
            if let Some(input) = input
                && old != new
                && input.read(cx).text() == old
            {
                input.update(cx, |input, cx| input.set_text_selected(new, cx));
            }
        }
    }

    /// The address and name fields, filled from the saved state, or with a
    /// guess at this computer's while sync is off.
    pub(in crate::settings_window) fn make_machine_sync_fields(&mut self, cx: &mut Context<Self>) {
        let filled = cx
            .try_global::<MachineSync>()
            .and_then(fill)
            .unwrap_or_else(Suggestion::local);
        let (address, name) = (filled.address.clone(), filled.name.clone());
        if self.machines.address.is_none() {
            self.machines.filled = Some(filled);
        }
        let field = |text: String, placeholder: &str, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_text_selected(&text, cx);
                input.set_placeholder(placeholder, cx);
            });
            input
        };
        // The address example is the same in every language; the name is a word.
        if self.machines.address.is_none() {
            self.machines.address = Some(field(address, "me@laptop.tailnet.ts.net", cx));
        }
        if self.machines.name.is_none() {
            self.machines.name = Some(field(
                name,
                &fl!("settings-machine-sync-name-placeholder"),
                cx,
            ));
        }
        self.sync_machine_sync_fields(cx);
    }

    /// The page went away. Its fields go too, so reopening it fills them
    /// again from the state it rereads.
    pub(in crate::settings_window) fn close_machine_sync(&mut self) {
        self.machines.confirming = false;
        self.machines.invalid = None;
        self.machines.watch = None;
        self.machines.address = None;
        self.machines.name = None;
        self.machines.filled = None;
    }

    pub(in crate::settings_window) fn sync_machine_sync_fields(&mut self, cx: &mut Context<Self>) {
        for input in [&self.machines.address, &self.machines.name]
            .into_iter()
            .flatten()
        {
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.terminal.clone(), self.theme.clone(), cx);
            });
        }
    }

    fn typed(&self, cx: &App) -> (String, String) {
        let text = |input: &Option<Entity<SearchInput>>| {
            input
                .as_ref()
                .map(|input| input.read(cx).text().trim().to_owned())
                .unwrap_or_default()
        };
        (text(&self.machines.name), text(&self.machines.address))
    }

    /// Turn on (confirmed) or Save: keep the typed address and name, then sync.
    fn save_machine_sync(&mut self, cx: &mut Context<Self>) {
        let (label, target) = self.typed(cx);
        match MachineSync::save(cx, label, target) {
            Ok(()) => {
                self.machines.confirming = false;
                self.machines.invalid = None;
            }
            Err(error) => self.machines.invalid = Some(error.localize()),
        }
        cx.notify();
    }

    /// Turn on…: check what was typed before asking.
    pub(in crate::settings_window) fn ask_machine_sync(&mut self, cx: &mut Context<Self>) {
        let (label, target) = self.typed(cx);
        // The ID is checked too, so any valid one stands in for the real one.
        let id = "0".repeat(32);
        match SyncState::new(id, label, target) {
            Ok(_) => {
                self.machines.confirming = true;
                self.machines.invalid = None;
            }
            Err(error) => {
                self.machines.invalid = Some(herdr_client::Error::from(error).localize());
            }
        }
        cx.notify();
    }

    pub(in crate::settings_window) fn render_machine_sync(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(sync) = cx.try_global::<MachineSync>() else {
            return div()
                .child(
                    ui::row(theme, "machine-sync", fl!("settings-machine-sync-title"))
                        .description(fl!("settings-machine-sync-unavailable")),
                )
                .child(
                    ui::row(
                        theme,
                        "machine-sync-machines",
                        fl!("settings-machine-sync-section-machines"),
                    )
                    .description(fl!("settings-machine-sync-unavailable")),
                );
        };
        let snapshot = Snapshot::of(sync);
        let on = snapshot.state.is_some();
        let running = snapshot.running;
        let mut switch_row = ui::row(theme, "machine-sync", fl!("settings-machine-sync-title"))
            .description(snapshot.status.clone())
            .switch(ui::toggle(theme, "machine-sync-toggle", on, !running).when(
                !running,
                |toggle| {
                    toggle.on_click(cx.listener(move |this, _, _, cx| {
                        if on {
                            MachineSync::turn_off(cx);
                        } else {
                            this.ask_machine_sync(cx);
                        }
                        cx.notify();
                    }))
                },
            ));
        if on {
            switch_row = switch_row.control(
                ui::button(
                    theme,
                    "machine-sync-now",
                    fl!("settings-machine-sync-now"),
                    !running,
                )
                .when(!running, |button| {
                    button.on_click(cx.listener(|_, _, _, cx| MachineSync::sync(cx)))
                }),
            );
        }
        let state = snapshot.state.as_ref();
        let included: Vec<&SavedHost> = snapshot
            .hosts
            .iter()
            .filter(|host| host.enabled && state.is_none_or(|s| s.includes(&host.id)))
            .collect();
        div()
            .flex()
            .flex_col()
            .child(switch_row)
            .when(self.machines.confirming && !on, |page| {
                page.child(self.machine_sync_confirm(&included, cx))
            })
            .child(ui::section(
                theme,
                &fl!("settings-machine-sync-section-this"),
                None,
            ))
            .child(self.machine_sync_fields(on, cx))
            .child(self.machine_sync_hosts(&snapshot, cx))
    }

    /// What turning sync on changes, on which machines, and the buttons
    /// that turn it on or leave it off.
    fn machine_sync_confirm(&self, included: &[&SavedHost], cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let names = included
            .iter()
            .map(|host| safe_text(&host.label, 64))
            .collect::<Vec<_>>()
            .join(", ");
        let machines = if included.is_empty() {
            fl!("settings-machine-sync-confirm-none")
        } else {
            fl!("settings-machine-sync-confirm-machines", names = names)
        };
        self.control_card(&fl!("settings-machine-sync-confirm"))
            .debug_selector(|| "machine-sync-confirm".into())
            .child(div().child(machines))
            .child(self.control_note(fl!("settings-machine-sync-confirm-changes")))
            .child(self.control_note(fl!("settings-machine-sync-confirm-ssh")))
            .child(
                div()
                    .row()
                    .gap(px(8.))
                    .child(
                        ui::button(
                            theme,
                            "machine-sync-confirm-on",
                            fl!("settings-machine-sync-confirm-on"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.save_machine_sync(cx))),
                    )
                    .child(
                        ui::button(
                            theme,
                            "machine-sync-confirm-cancel",
                            fl!("settings-machine-sync-confirm-cancel"),
                            true,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.machines.confirming = false;
                            cx.notify();
                        })),
                    ),
            )
    }

    /// This computer's address and name, saved with Save while sync is on.
    fn machine_sync_fields(&self, on: bool, cx: &mut Context<Self>) -> Div {
        let field = |label: String,
                     note: String,
                     selector: &'static str,
                     input: &Option<Entity<SearchInput>>| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .row()
                        .items_center()
                        .gap(px(8.))
                        .child(div().w(px(80.)).flex_none().child(label))
                        .children(input.as_ref().map(|input| {
                            div()
                                .debug_selector(move || selector.into())
                                .flex_1()
                                .min_w_0()
                                .child(input.clone())
                        })),
                )
                .child(self.control_note(note))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .py(px(6.))
            .child(field(
                fl!("settings-machine-sync-address"),
                fl!("settings-machine-sync-address-note"),
                "machine-sync-address",
                &self.machines.address,
            ))
            .child(field(
                fl!("settings-machine-sync-name"),
                fl!("settings-machine-sync-name-note"),
                "machine-sync-name",
                &self.machines.name,
            ))
            .when_some(self.machines.invalid.clone(), |card, invalid| {
                card.child(self.control_note(invalid))
            })
            .when(on, |card| {
                card.child(
                    ui::button(
                        &self.theme,
                        "machine-sync-save",
                        fl!("settings-machine-sync-save"),
                        true,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.save_machine_sync(cx))),
                )
            })
    }

    /// One row per saved SSH device, ticked while it takes part.
    fn machine_sync_hosts(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let state = snapshot.state.as_ref();
        let hosts = &snapshot.hosts;
        let note = match (hosts.is_empty(), snapshot.loaded) {
            (false, _) => fl!("settings-machine-sync-machines-tick"),
            (true, true) => fl!("settings-machine-sync-machines-none"),
            (true, false) => fl!("settings-machine-sync-machines-loading"),
        };
        let heading = ui::row(
            theme,
            "machine-sync-machines",
            fl!("settings-machine-sync-section-machines"),
        )
        .description(note);
        let rows = hosts.iter().enumerate().map(|(index, host)| {
            let name = safe_text(&host.label, 64);
            let ticked = state.is_none_or(|state| state.includes(&host.id));
            // Only a saved state has a list to change.
            let enabled = host.enabled && state.is_some();
            let line = snapshot
                .lines
                .iter()
                .find(|(id, _)| *id == host.id)
                .map(|(_, line)| line.clone());
            let detail = match (host.enabled, ticked, line) {
                (false, _, _) => fl!("settings-machine-sync-machine-disabled"),
                (true, false, _) => fl!("settings-machine-sync-machine-left-out"),
                (true, true, Some(line)) => line,
                (true, true, None) => fl!("settings-machine-sync-machine-pending"),
            };
            let id = host.id.clone();
            let mark = div()
                .id(("machine-sync-host", index))
                .debug_selector(move || format!("machine-sync-host-{index}"))
                .role(Role::CheckBox)
                .aria_label(SharedString::from(name.clone()))
                .aria_toggled(if ticked {
                    Toggled::True
                } else {
                    Toggled::False
                })
                .p(px(3.))
                .rounded(px(crate::config::corners::CONTROL))
                .when(enabled, |mark| {
                    mark.cursor_pointer()
                        .hover(|mark| mark.bg(rgb(theme.active)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            MachineSync::set_included(cx, &id, !ticked);
                        }))
                })
                .when(!enabled, |mark| mark.opacity(0.5))
                .child(checkbox(theme, 14., ticked && host.enabled));
            ui::row(theme, format!("machine-sync-machine-{index}"), name)
                .description(format!("{} \u{b7} {detail}", safe_text(&host.target, 128)))
                .control(mark)
                .into_any_element()
        });
        let learned = snapshot
            .learned
            .clone()
            .map(|names| self.control_note(fl!("settings-machine-sync-learned", names = names)));
        div()
            .flex()
            .flex_col()
            .child(heading)
            .children(rows)
            .children(learned)
    }
}
