//! Settings > Virtual Macs: what each machine is given, what it takes on
//! disk, and suspending, resuming, or deleting it. A machine's disk is tens
//! of gigabytes, so this page is also how it is given back.
// i18n: migrated
use super::{SettingsWindow, ui};
use crate::{
    SheprdWindow,
    i18n::{Localize as _, fl},
    machine::{self, Display, Limits, Listing, Name, Settings, Status},
    settings_window::Section,
};
use gpui::{prelude::*, *};
use std::collections::HashMap;

/// Opens Settings on this page, asking to delete `delete` if one is named.
pub(crate) fn open(source: WeakEntity<SheprdWindow>, delete: Option<Name>, cx: &mut App) {
    crate::settings_window::open(source, cx);
    // Queued after `open`'s own deferred work, so the window exists by now.
    cx.defer(move |cx| {
        let Some(handle) = cx
            .default_global::<crate::settings_window::SettingsWindowHandle>()
            .window
        else {
            return;
        };
        let _ = handle.update(cx, |view, window, cx| {
            view.select_section(Section::Machines, window, cx);
            view.machines.confirming = delete;
            window.activate_window();
        });
    });
}

#[derive(Default)]
pub(in crate::settings_window) struct MachinesPage {
    /// `None` until the folders are read.
    listings: Option<Vec<Listing>>,
    /// The machine whose deletion waits for a second press.
    confirming: Option<Name>,
    /// Being deleted, whose row stays until it is gone.
    deleting: Option<Name>,
    /// What went wrong last, for the user.
    failure: Option<String>,
    /// What each machine was doing when last drawn: a change moves its disk
    /// use, so the folders are read again.
    seen: HashMap<Name, Status>,
}

/// `bytes` as the Finder counts them.
pub(in crate::settings_window) fn gigabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1e9) // i18n: data
}

impl SettingsWindow {
    /// Reads the machines' folders again, off the UI thread.
    pub(in crate::settings_window) fn open_machines(&mut self, cx: &mut Context<Self>) {
        let read = cx.background_executor().spawn(async { machine::list() });
        cx.spawn(async move |this, cx| {
            let listed = read.await;
            let _ = this.update(cx, |this, cx| {
                match listed {
                    Ok(listings) => this.machines.listings = Some(listings),
                    Err(error) => this.machines.failure = Some(error.localize()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads the folders again if a machine has stopped, resumed, or saved
    /// since the last frame.
    fn follow_machines(&mut self, cx: &mut Context<Self>) {
        let Some(listings) = &self.machines.listings else {
            return;
        };
        let now: HashMap<Name, Status> = listings
            .iter()
            .map(|listing| (listing.name.clone(), machine::status(&listing.name)))
            .collect();
        if now != self.machines.seen {
            let first = self.machines.seen.is_empty();
            self.machines.seen = now;
            if !first {
                self.open_machines(cx);
            }
        }
    }

    fn change_machine(&mut self, name: &Name, settings: Settings, cx: &mut Context<Self>) {
        if let Some(listing) = self
            .machines
            .listings
            .iter_mut()
            .flatten()
            .find(|listing| listing.name == *name)
        {
            listing.settings = settings;
        }
        let name = name.clone();
        let write = cx
            .background_executor()
            .spawn(async move { machine::save_settings(&name, &settings) });
        cx.spawn(async move |this, cx| {
            let written = write.await;
            let _ = this.update(cx, |this, cx| {
                this.machines.failure = written.err().map(|error| error.localize());
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn forget_machine_state(&mut self, name: &Name, cx: &mut Context<Self>) {
        let name = name.clone();
        let discard = cx
            .background_executor()
            .spawn(async move { machine::discard_state(&name) });
        cx.spawn(async move |this, cx| {
            let discarded = discard.await;
            let _ = this.update(cx, |this, cx| {
                this.machines.failure = discarded.err().map(|error| error.localize());
                this.open_machines(cx);
            });
        })
        .detach();
    }

    fn delete_machine(&mut self, name: &Name, cx: &mut Context<Self>) {
        self.machines.confirming = None;
        self.machines.deleting = Some(name.clone());
        let this = cx.entity().downgrade();
        machine::delete(name, cx, move |deleted, cx| {
            let _ = this.update(cx, |this, cx| {
                this.machines.deleting = None;
                this.machines.failure = deleted.err().map(|error| error.localize());
                this.open_machines(cx);
            });
        });
        cx.notify();
    }

    /// A value between two buttons that step it within `range`.
    fn machine_stepper(
        &self,
        id: String,
        value: String,
        current: u16,
        range: std::ops::RangeInclusive<u16>,
        cx: &Context<Self>,
        set: impl Fn(&mut Self, u16, &mut Context<Self>) + Clone + 'static,
    ) -> Div {
        let button = |suffix: &str, label: &'static str, next: Option<u16>| {
            let set = set.clone();
            ui::button(&self.theme, format!("{id}-{suffix}"), label, next.is_some()).when_some(
                next,
                |button, next| {
                    button.on_click(cx.listener(move |this, _, _, cx| set(this, next, cx)))
                },
            )
        };
        let lower = current.checked_sub(1).filter(|next| range.contains(next));
        let higher = current.checked_add(1).filter(|next| range.contains(next));
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .child(button("less", "\u{2212}", lower)) // i18n: data
            .child(div().min_w(px(64.)).flex().justify_center().child(value))
            .child(button("more", "+", higher)) // i18n: data
    }

    fn render_machine(
        &self,
        listing: &Listing,
        limits: &Limits,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let name = listing.name.clone();
        let settings = listing.settings;
        let status = machine::status(&name);
        let deleting = self.machines.deleting.as_ref() == Some(&name);
        let size = gigabytes(listing.disk_bytes);
        let state = match (&status, listing.saved) {
            _ if deleting => fl!("settings-machines-deleting"),
            (Status::Running, _) => fl!("settings-machines-running"),
            (Status::Starting, _) => fl!("settings-machines-starting"),
            (Status::Saving, _) => fl!("settings-machines-saving"),
            (Status::Failed(message), _) => message.clone(),
            (Status::Stopped, true) => fl!("settings-machines-suspended"),
            (Status::Stopped, false) => fl!("settings-machines-stopped"),
        };
        let id = |what: &str| format!("machine-{name}-{what}");
        // A machine that is up or saved keeps what it booted with.
        let later = status != Status::Stopped || listing.saved;
        let cpus = {
            let name = name.clone();
            self.machine_stepper(
                id("cpus"),
                settings.cpus.to_string(),
                settings.cpus,
                limits.cpus.clone(),
                cx,
                move |this, cpus, cx| {
                    this.change_machine(&name, Settings { cpus, ..settings }, cx);
                },
            )
        };
        let memory = {
            let name = name.clone();
            self.machine_stepper(
                id("memory"),
                fl!(
                    "settings-machines-gb",
                    size = settings.memory_gib.to_string()
                ),
                settings.memory_gib,
                limits.memory_gib.clone(),
                cx,
                move |this, memory_gib, cx| {
                    let changed = Settings {
                        memory_gib,
                        ..settings
                    };
                    this.change_machine(&name, changed, cx);
                },
            )
        };
        let mut displays = vec![(Display::Fit, fl!("settings-machines-display-fit"))];
        displays.extend(Display::PRESETS.map(|display| (display, display.to_string())));
        let display = {
            let name = name.clone();
            let this = cx.entity().downgrade();
            ui::segmented(
                &self.theme,
                "machine-display",
                &displays,
                Some(settings.display),
                true,
                move |display, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.change_machine(
                            &name,
                            Settings {
                                display,
                                ..settings
                            },
                            cx,
                        );
                    });
                },
            )
        };
        // Each machine's controls share their names, so each has its own scope.
        div()
            .id(id("page"))
            .flex()
            .flex_col()
            .child(ui::section(&self.theme, name.as_str(), None))
            .child(
                ui::row(&self.theme, id("state"), state)
                    .description(fl!("settings-machines-disk", size = size.clone()))
                    .control(self.machine_actions(listing, &status, cx))
                    .into_div(),
            )
            .child(
                ui::row(&self.theme, id("cpus-row"), fl!("settings-machines-cpus"))
                    .control(cpus)
                    .into_div(),
            )
            .child(
                ui::row(
                    &self.theme,
                    id("memory-row"),
                    fl!("settings-machines-memory"),
                )
                .control(memory)
                .into_div(),
            )
            .child(
                ui::row(
                    &self.theme,
                    id("display-row"),
                    fl!("settings-machines-display"),
                )
                .description(fl!("settings-machines-display-description"))
                .control(display)
                .into_div(),
            )
            .when(later, |page| {
                page.child(self.control_note(fl!("settings-machines-applies-later")))
            })
            .child(self.machine_delete(listing, &status, cx))
    }

    fn machine_actions(&self, listing: &Listing, status: &Status, cx: &Context<Self>) -> Div {
        let name = &listing.name;
        let button = |what: &str, label: String, act: fn(&mut Self, &Name, &mut Context<Self>)| {
            let target = name.clone();
            ui::button(&self.theme, format!("machine-{name}-{what}"), label, true)
                .on_click(cx.listener(move |this, _, _, cx| act(this, &target, cx)))
        };
        let open = button("open", fl!("settings-machines-open"), |this, name, cx| {
            crate::browser::open_machine_in(&this.source, name.clone(), cx);
        });
        let suspend = button(
            "suspend",
            fl!("settings-machines-suspend"),
            |_, name, cx| {
                machine::suspend(name, cx);
            },
        );
        let shut_down = button(
            "shut-down",
            fl!("settings-machines-shut-down"),
            |this, name, cx| {
                this.machines.failure = machine::shut_down(name).err().map(|e| e.localize());
                cx.notify();
            },
        );
        let forget = button(
            "forget",
            fl!("settings-machines-forget"),
            |this, name, cx| {
                this.forget_machine_state(name, cx);
            },
        );
        let row = div().flex().flex_row().gap(px(6.));
        match (status, listing.saved) {
            (Status::Running, _) => row.child(open).child(suspend).child(shut_down),
            (Status::Stopped, true) => row.child(open).child(forget),
            (Status::Stopped | Status::Failed(_), _) => row.child(open),
            (Status::Starting | Status::Saving, _) => row,
        }
    }

    /// Delete, and the second press it asks for, which names what is lost
    /// and what comes back.
    fn machine_delete(&self, listing: &Listing, status: &Status, cx: &Context<Self>) -> Div {
        let name = &listing.name;
        let size = gigabytes(listing.disk_bytes);
        let row = |id: String, title: String| ui::row(&self.theme, id, title);
        if self.machines.confirming.as_ref() == Some(name) {
            let target = name.clone();
            let question = if *status == Status::Stopped {
                fl!(
                    "settings-machines-delete-ask",
                    name = name.as_str(),
                    size = size
                )
            } else {
                fl!(
                    "settings-machines-delete-ask-running",
                    name = name.as_str(),
                    size = size
                )
            };
            return row(format!("machine-{name}-delete-ask"), question)
                .control(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(6.))
                        .child(
                            ui::button(
                                &self.theme,
                                format!("machine-{name}-delete-confirm"),
                                fl!("settings-machines-delete-confirm"),
                                true,
                            )
                            .text_color(rgb(self.theme.palette[1]))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.delete_machine(&target, cx);
                                },
                            )),
                        )
                        .child(
                            ui::button(
                                &self.theme,
                                format!("machine-{name}-delete-cancel"),
                                fl!("settings-machines-delete-cancel"),
                                true,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.machines.confirming = None;
                                cx.notify();
                            })),
                        ),
                )
                .into_div();
        }
        let target = name.clone();
        let enabled = self.machines.deleting.is_none() && *status != Status::Saving;
        row(
            format!("machine-{name}-delete-row"),
            fl!("settings-machines-delete"),
        )
        .description(fl!("settings-machines-delete-description", size = size))
        .control(
            ui::button(
                &self.theme,
                format!("machine-{name}-delete"),
                fl!("settings-machines-delete-button"),
                enabled,
            )
            .when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.machines.confirming = Some(target.clone());
                    cx.notify();
                }))
            }),
        )
        .into_div()
    }

    pub(in crate::settings_window) fn render_machines(&mut self, cx: &mut Context<Self>) -> Div {
        self.follow_machines(cx);
        let limits = Limits::host();
        let listings = self.machines.listings.clone();
        let page = div().flex().flex_col();
        let page = match listings.as_deref() {
            None => page,
            Some([]) => page.child(self.control_note(fl!("settings-machines-none"))),
            Some(listings) => page.children(
                listings
                    .iter()
                    .map(|listing| self.render_machine(listing, &limits, cx)),
            ),
        };
        page.when_some(self.machines.failure.clone(), |page, failure| {
            page.child(self.control_note(failure))
        })
    }
}

#[cfg(test)]
mod tests;
