//! Each herdr-core tab's panes as the window keeps them: a view per pane,
//! the pane in use, panes whose program ended, and the layout saved with the
//! tab once a change settles.

use super::{CoreTerminalView, Start, TabPanes, panes::Axis, worker};
use crate::{
    HerdrWindow,
    browser::{Location, Store, TabId},
    window::Flash,
};
use gpui::{prelude::*, *};
use herdr_backend::{Backend as _, Role, Size, TerminalId};

/// The size a terminal starts at, before its pane is laid out.
const FIRST_SIZE: (u16, u16) = (80, 24);

impl HerdrWindow {
    /// Gives every pane of the focused workspace's herdr-core tabs a view,
    /// once the hub is connected, and drops the views of closed tabs and
    /// panes.
    pub(super) fn sync_core_views(&mut self, cx: &mut Context<Self>) {
        let Some(store) = cx.try_global::<Store>() else {
            self.core_terminals.views.clear();
            self.core_terminals.tabs.clear();
            return;
        };
        let terminals = &mut self.core_terminals;
        terminals.tabs.retain(|id, _| store.get(*id).is_some());
        let tabs = &terminals.tabs;
        terminals.views.retain(|(tab, terminal), _| {
            tabs.get(tab)
                .is_some_and(|shown| shown.panes.contains(terminal))
        });
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let mut missing: Vec<(TabId, TerminalId, Role)> = Vec::new();
        for tab in store.in_workspace(&scope, &workspace) {
            let Some(Location::CoreTerminal { panes, observe }) = &tab.location else {
                continue;
            };
            let shown = self
                .core_terminals
                .tabs
                .entry(tab.id)
                .or_insert_with(|| TabPanes::new(panes.clone(), *observe));
            missing.extend(
                shown
                    .panes
                    .terminals()
                    .into_iter()
                    .filter(|terminal| {
                        !self
                            .core_terminals
                            .views
                            .contains_key(&(tab.id, (*terminal).clone()))
                    })
                    .map(|terminal| {
                        let role = if shown.observe {
                            Role::Observer
                        } else {
                            Role::Active
                        };
                        (tab.id, terminal.clone(), role)
                    }),
            );
        }
        if missing.is_empty() {
            return;
        }
        // Restored tabs join a running core; only a command starts one.
        let Some(backend) = self.core_terminals.hub.backend().cloned() else {
            self.core_terminals.hub.connect(Start::Join);
            return;
        };
        let look = self.core_look();
        for (tab, terminal, role) in missing {
            let Ok(size) = Size::new(FIRST_SIZE.0, FIRST_SIZE.1) else {
                continue;
            };
            let link = match worker::spawn(backend.clone(), terminal.clone(), size, role) {
                Ok(link) => link,
                Err(error) => {
                    tracing::warn!(%error, "Could not start a herdr-core terminal worker");
                    continue;
                }
            };
            let painter = self.painter.clone();
            let look = look.clone();
            let observe = role == Role::Observer;
            let view = cx.new(|cx| {
                CoreTerminalView::new(terminal.clone(), link, look, painter, observe, cx)
            });
            self.core_terminals.views.insert((tab, terminal), view);
        }
        // The tab drew its placeholder; it shows the new views now.
        cx.notify();
    }

    /// Takes every pane's new screen, follows the pane the keyboard moved
    /// to, closes panes whose program ended in a split, and titles each tab
    /// after its pane in use, as a Herdr tab is titled after its program.
    pub(super) fn poll_core_panes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((tab, terminal)) = self.core_terminals.focused_pane(window, cx)
            && let Some(shown) = self.core_terminals.tabs.get_mut(&tab)
            && shown.active != terminal
        {
            shown.active = terminal;
            cx.notify();
        }
        let views: Vec<((TabId, TerminalId), Entity<CoreTerminalView>)> = self
            .core_terminals
            .views
            .iter()
            .map(|(key, view)| (key.clone(), view.clone()))
            .collect();
        let mut ended = Vec::new();
        for ((tab, terminal), view) in views {
            view.update(cx, |view, cx| view.poll(cx));
            let split = self
                .core_terminals
                .tabs
                .get(&tab)
                .is_some_and(|shown| shown.panes.len() > 1);
            if split && view.read(cx).ended() {
                ended.push((tab, terminal));
            }
        }
        for (tab, terminal) in ended {
            self.remove_core_pane(tab, &terminal, window, cx);
        }
        let titles: Vec<(TabId, String)> = self
            .core_terminals
            .tabs
            .iter()
            .filter_map(|(tab, shown)| {
                let view = self
                    .core_terminals
                    .views
                    .get(&(*tab, shown.active.clone()))?;
                Some((*tab, view.read(cx).title()?.to_owned()))
            })
            .collect();
        for (tab, title) in titles {
            let stale = cx
                .try_global::<Store>()
                .and_then(|store| store.get(tab))
                .is_some_and(|shown| shown.title != title);
            if stale {
                Store::update(cx, |store| store.visited(tab, None, Some(&title)));
            }
        }
    }

    /// Puts a started terminal beside `at` in `tab`, and gives it the
    /// keyboard. One that cannot be placed is ended rather than left behind.
    pub(super) fn place_split(
        &mut self,
        tab: TabId,
        at: &TerminalId,
        axis: Axis,
        new: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placed = match self.core_terminals.tabs.get_mut(&tab) {
            Some(shown) => shown.panes.split(at, axis, new.clone()).map(|()| {
                shown.active = new.clone();
                shown.set_zoom(None);
                shown.unsaved = true;
            }),
            None => Err(super::panes::Error::Missing(at.clone())),
        };
        if let Err(error) = placed {
            self.show_flash(Flash::warning(error.to_string()), cx);
            self.end_core_terminal(new);
            return;
        }
        self.sync_core_views(cx);
        self.focus_core_terminal(tab, window, cx);
    }

    /// Takes `terminal` out of `tab`'s panes. The pane beside it, or else
    /// the first, is the tab's pane in use after, and takes the keyboard when
    /// the closed one had it.
    pub(super) fn remove_core_pane(
        &mut self,
        tab: TabId,
        terminal: &TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let had_focus = self
            .core_terminals
            .views
            .get(&(tab, terminal.clone()))
            .is_some_and(|view| view.read(cx).focus_handle().is_focused(window));
        let Some(shown) = self.core_terminals.tabs.get_mut(&tab) else {
            return false;
        };
        let next = [
            super::panes::Direction::Left,
            super::panes::Direction::Up,
            super::panes::Direction::Right,
            super::panes::Direction::Down,
        ]
        .into_iter()
        .find_map(|direction| shown.panes.neighbor(terminal, direction));
        if !shown.panes.remove(terminal) {
            return false;
        }
        if shown.zoomed.as_ref() == Some(terminal) {
            shown.zoomed = None;
            shown.zoom = crate::motion::Toggle::default();
        }
        if &shown.active == terminal {
            shown.active = next.unwrap_or_else(|| shown.panes.first().clone());
        }
        shown.unsaved = true;
        self.core_terminals.views.remove(&(tab, terminal.clone()));
        if had_focus {
            self.focus_core_terminal(tab, window, cx);
        }
        cx.notify();
        true
    }

    /// Ends a terminal on a worker thread, so herdr-core's answer is never
    /// awaited on the UI thread.
    pub(super) fn end_core_terminal(&self, terminal: TerminalId) {
        let Some(backend) = self.core_terminals.hub.backend().cloned() else {
            return;
        };
        let ended = std::thread::Builder::new()
            .name("herdr-core-kill".into())
            .spawn(move || {
                if let Err(error) = backend.kill(&terminal) {
                    tracing::warn!(%error, %terminal, "Could not end a herdr-core terminal");
                }
            });
        if let Err(error) = ended {
            tracing::warn!(%error, "Could not start a herdr-core kill worker");
        }
    }

    /// Writes changed layouts into their tabs, which saves them.
    pub(super) fn save_core_panes(&mut self, cx: &mut Context<Self>) {
        let changed: Vec<(TabId, Location)> = self
            .core_terminals
            .tabs
            .iter_mut()
            .filter_map(|(tab, shown)| {
                std::mem::take(&mut shown.unsaved).then(|| {
                    let location = Location::CoreTerminal {
                        panes: shown.panes.clone(),
                        observe: shown.observe,
                    };
                    (*tab, location)
                })
            })
            .collect();
        for (tab, location) in changed {
            Store::update(cx, |store| store.visited(tab, Some(location), None));
        }
    }
}
