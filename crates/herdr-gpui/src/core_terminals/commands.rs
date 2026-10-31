//! The window's pane commands, run on a herdr-core tab's panes while one
//! holds the keyboard: split, move focus, resize, zoom, and close. The same
//! shortcuts work on Herdr panes, so a herdr-core tab handles them here
//! rather than letting them reach Herdr.

use super::{
    Purpose,
    panes::{Axis, Direction, Panes},
};
use crate::{
    HerdrWindow,
    browser::{Location, Store, TabId},
    controls::Command,
    i18n::fl,
    window::Flash,
};
use gpui::{Context, Window};
use herdr_backend::TerminalId;

impl HerdrWindow {
    /// Runs `command` on the focused herdr-core tab's panes. False when no
    /// herdr-core pane has the keyboard or the command is not a pane's, so
    /// the window runs it as usual.
    pub(crate) fn core_pane_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((tab, terminal)) = self.core_terminals.focused_pane(window, cx) else {
            return false;
        };
        let Some(shown) = self.core_terminals.tabs.get_mut(&tab) else {
            return false;
        };
        // A mirror only watches: it moves focus and zooms, never splits.
        if shown.observe && matches!(command, Command::SplitRight | Command::SplitDown) {
            return true;
        }
        let direction = |command: Command| match command {
            Command::FocusLeft | Command::ResizeLeft => Some(Direction::Left),
            Command::FocusRight | Command::ResizeRight => Some(Direction::Right),
            Command::FocusUp | Command::ResizeUp => Some(Direction::Up),
            Command::FocusDown | Command::ResizeDown => Some(Direction::Down),
            _ => None,
        };
        match command {
            Command::SplitRight | Command::SplitDown => {
                let axis = match command {
                    Command::SplitRight => Axis::Row,
                    _ => Axis::Column,
                };
                self.core_terminals.wanted = Some(Purpose::Split {
                    tab,
                    at: terminal,
                    axis,
                });
                self.start_core_shell();
            }
            Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => {
                let Some(next) = direction(command)
                    .and_then(|direction| shown.panes.neighbor(&terminal, direction))
                else {
                    return true;
                };
                // Moving on from a zoomed pane shows the split again.
                if shown.zoom.is_open() {
                    shown.set_zoom(None);
                }
                shown.active = next;
                self.focus_core_terminal(tab, window, cx);
                cx.notify();
            }
            Command::ResizeLeft
            | Command::ResizeRight
            | Command::ResizeUp
            | Command::ResizeDown => {
                if direction(command)
                    .is_some_and(|direction| shown.panes.resize(&terminal, direction))
                {
                    shown.unsaved = true;
                    cx.notify();
                }
            }
            Command::Zoom => {
                if shown.panes.len() > 1 {
                    let zoom = (!shown.zoom.is_open()).then_some(terminal);
                    shown.set_zoom(zoom);
                    cx.notify();
                }
            }
            Command::MirrorCorePane => self.mirror_core_pane(tab, terminal, window, cx),
            // The last pane closes as its tab does: the window's own close.
            Command::ClosePane if shown.panes.len() > 1 => {
                // A mirror's pane stops watching; the terminal runs on.
                let mirror = shown.observe;
                if self.remove_core_pane(tab, &terminal, window, cx) && !mirror {
                    self.end_core_terminal(terminal);
                }
            }
            _ => return false,
        }
        true
    }

    /// Opens a watch-only view of `terminal` in a new tab of `tab`'s
    /// workspace, which a second group can show beside the first.
    fn mirror_core_pane(
        &mut self,
        tab: TabId,
        terminal: TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((scope, workspace)) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(tab))
            .map(|shown| (shown.scope.clone(), shown.workspace_id.clone()))
        else {
            return;
        };
        let location = Location::CoreTerminal {
            panes: Panes::single(terminal),
            observe: true,
        };
        let Some(id) = Store::update(cx, |store| {
            store.open(scope, &workspace, Some(location), None)
        }) else {
            self.show_flash(Flash::warning(fl!("core-terminals-too-many-tabs")), cx);
            return;
        };
        self.sync_core_views(cx);
        self.show_browser_tab(id, window, cx);
    }
}
