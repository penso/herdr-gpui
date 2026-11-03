//! Running a chosen row: commands, Go To destinations, projects, and the
//! daemon custom commands a shortcut also reaches.

use super::{Action, Error, Frecency, Palette, Target, destination_exists, frecency};
use crate::{HerdrWindow, NavigationTarget, Result};
use gpui::*;
use herdr_client::{Method, protocol::ClientShellCommandAction};

impl HerdrWindow {
    pub(super) fn activate_palette(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A click chooses a row without moving the selection there, so
        // select it now: Resume Palette reopens on the row actually chosen.
        if let Some(palette) = &mut self.menu.palette {
            let chosen = action.identity();
            if let Some(row) = palette
                .filtered
                .iter()
                .position(|hit| palette.entries[hit.index].action.identity() == chosen)
            {
                palette.selected = row;
            }
        }
        let action = match action {
            Action::Note {
                endpoint,
                boot,
                workspace,
            } => Action::Go {
                endpoint,
                boot,
                target: NavigationTarget::Workspace(workspace),
            },
            action => action,
        };
        if self.menu.palette.as_ref().is_none_or(Palette::busy) {
            return;
        }
        // Found by identity, so a note row counts as its workspace.
        let identity = action.identity();
        let remembered = self.menu.palette.as_ref().and_then(|palette| {
            palette
                .entries
                .iter()
                .find(|entry| entry.action.identity() == identity)
                .and_then(|entry| entry.frecency.clone())
        });
        let remember = |cx: &mut Context<Self>| {
            if let Some(key) = remembered {
                Frecency::update(cx, |history| history.visit(key, frecency::now()));
            }
        };
        if !self.menu_target_current() {
            if let Some(palette) = &mut self.menu.palette {
                palette.error = Some(crate::i18n::fl!("palette-go-to-connection-changed"));
            }
            cx.notify();
            return;
        }
        if let Action::Native(command) = action {
            self.dismiss_menu(window, cx);
            self.command(command, window, cx);
            return;
        }
        if let Action::Mod { name, id } = action {
            self.dismiss_menu(window, cx);
            self.send_to_mod(&name, herdr_mods::Data::String(format!("command:{id}")), cx);
            return;
        }
        if let Action::Project(project) = action {
            remember(cx);
            self.activate_project(project, window, cx);
            return;
        }
        if let Action::NewProject(project) = action {
            self.create_project(project, window, cx);
            return;
        }
        if let Action::Go {
            endpoint,
            boot,
            target,
        } = &action
        {
            match self.go_to_ready(endpoint, boot, target.as_deref()) {
                Ok(selected) => {
                    remember(cx);
                    self.dismiss_menu(window, cx);
                    if selected {
                        self.navigate(target.as_deref(), cx);
                    } else {
                        self.navigate_endpoint(endpoint, target.as_deref(), cx);
                    }
                }
                Err(error) => {
                    if let Some(palette) = &mut self.menu.palette {
                        palette.error = Some(error.to_string());
                    }
                    cx.notify();
                }
            }
            return;
        }
        let result = (|| {
            if !self.input_ready() {
                return Err(Error::ConnectionNotReady.into());
            }
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NoSnapshot)?;
            let target = self
                .menu
                .palette
                .as_ref()
                .and_then(|p| p.target.as_ref())
                .ok_or(Error::NoSession)?;
            match &action {
                Action::Configured(id, action) => {
                    let mut target = target.clone();
                    if let Some(selection) = &mut target.selection {
                        self.fence_plugin_selection(selection);
                    }
                    target.invocation(snapshot, id, *action)
                }
                Action::Native(_)
                | Action::Go { .. }
                | Action::Project(_)
                | Action::NewProject(_)
                | Action::Note { .. }
                | Action::Mod { .. } => unreachable!(),
            }
        })();
        match result {
            Ok(params) => {
                self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
                    handle.request(boot, Method::CommandInvoke, params)
                });
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(palette) = &mut self.menu.palette {
                    palette.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    /// Runs a daemon custom command on the focused workspace, tab, and pane,
    /// as choosing it in the palette does; a shortcut has no palette to
    /// report a refusal in, so it reads as a local error instead.
    pub(crate) fn invoke_custom_command(
        &mut self,
        id: &str,
        action: ClientShellCommandAction,
        cx: &mut Context<Self>,
    ) {
        if self.activation_deadline.is_some()
            || !self.endpoints[self.selected_endpoint].surface_requested()
        {
            return;
        }
        let result = (|| {
            if !self.input_ready() {
                return Err(Error::ConnectionNotReady.into());
            }
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NoSnapshot)?;
            Target::capture(snapshot, self.plugin_selection()).invocation(snapshot, id, action)
        })();
        match result {
            Ok(params) => {
                self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
                    handle.request(boot, Method::CommandInvoke, params)
                });
                self.discard_composition(cx);
            }
            Err(error) => self.local_error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Checks a Go To destination against its host's current snapshot, and
    /// reports whether that host is the selected one. Another host is selected
    /// by the navigation itself, which waits for its surface when needed.
    fn go_to_ready(
        &self,
        endpoint: &str,
        boot: &str,
        target: NavigationTarget<&str>,
    ) -> Result<bool> {
        let index = self
            .endpoints
            .iter()
            .position(|e| e.id == endpoint && e.enabled)
            .ok_or(Error::HostUnavailable)?;
        let selected = index == self.selected_endpoint;
        let live = if selected {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        let snapshot = live.snapshot.as_ref().ok_or(Error::HostUnavailable)?;
        destination_exists(snapshot, boot, target)?;
        if selected && !self.input_ready() {
            return Err(Error::ConnectionNotReady.into());
        }
        Ok(selected)
    }
}
