//! Muting the notifications of the workspace or tab a menu targets, on the
//! selected endpoint. The mutes live in [`crate::notification_mutes`].

use crate::{
    HerdrWindow,
    notification_mutes::{Daemon, Mutes, Scope},
};
use gpui::{App, Context};
use std::time::SystemTime;

impl HerdrWindow {
    /// Whether the selected endpoint's workspace or tab `id` is muted on its
    /// own: a tab silent only through its workspace is not.
    pub(crate) fn muted_on_selected(&self, scope: Scope, id: &str, cx: &App) -> bool {
        let Some(snapshot) = self.live.snapshot.as_deref() else {
            return false;
        };
        let daemon = Daemon::of(&self.endpoints[self.selected_endpoint]);
        Mutes::of(cx).is_some_and(|mutes| {
            let silenced = mutes.silenced(&daemon, snapshot);
            match scope {
                Scope::Workspace => silenced.workspace(id),
                Scope::Tab => silenced.own_tab(id),
            }
        })
    }

    /// Mutes or unmutes the selected endpoint's workspace or tab `id`, while
    /// the menu still targets the connection and daemon process `boot` it
    /// opened on. Returns whether anything changed.
    pub(crate) fn set_muted_on_selected(
        &mut self,
        scope: Scope,
        id: &str,
        boot: &str,
        muted: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.menu_target_current() {
            return false;
        }
        let Some(snapshot) = self
            .live
            .snapshot
            .clone()
            .filter(|snapshot| snapshot.boot_id == boot)
        else {
            return false;
        };
        let daemon = Daemon::of(&self.endpoints[self.selected_endpoint]);
        let changed = Mutes::update(cx, |mutes| {
            mutes.set(&daemon, &snapshot, scope, id, muted, SystemTime::now())
        });
        if changed {
            cx.notify();
        }
        changed
    }
}
