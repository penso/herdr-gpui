//! A device's setup script reaching a visible pane, as a worktree's does: a
//! new `setup` tab gets the script in its environment, and the same fixed line
//! that runs a worktree script is typed into its shell. The tab has no working
//! directory of its own, so the shell starts where the device's does: home.

use super::Pending;
use crate::{HerdrWindow, NavigationTarget, worktree_scripts};
use gpui::Context;
use herdr_client::{CloudProvider, Method};
use serde_json::{Value, json};

/// Names the provider for a script that wants to tell its devices apart.
pub(crate) const PROVIDER_ENV: &str = "HERDR_CLOUD_PROVIDER";

/// A setup script whose `tab.create` is in flight, fenced like a worktree
/// script by the selection, connection, and daemon boot it was sent to.
pub(crate) struct Running {
    pub(super) boot: String,
    pub(super) endpoint: (u64, u64),
    pub(super) endpoint_id: String,
    pub(super) label: String,
    pub(super) request: String,
}

/// The `tab.create` a device's setup script runs in.
pub(super) fn tab_params(workspace: &str, provider: CloudProvider, script: &str) -> Value {
    json!({
        "workspace_id": workspace,
        "label": "setup",
        "focus": true,
        "env": {
            worktree_scripts::SCRIPT_ENV: script,
            PROVIDER_ENV: provider.key(),
        },
    })
}

impl HerdrWindow {
    /// Open the tab `pending` runs in, in `workspace` of the device shown.
    pub(super) fn start_cloud_setup(
        &mut self,
        pending: Pending,
        workspace: &str,
        boot: String,
    ) -> crate::Result<()> {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let request = endpoint.connection.request_script(
            &boot,
            Method::TabCreate,
            tab_params(workspace, pending.provider, &pending.script),
        )?;
        self.cloud_setup = Some(Running {
            boot,
            endpoint: (self.selection_epoch, endpoint.generation),
            endpoint_id: pending.endpoint_id,
            label: pending.label,
            request,
        });
        Ok(())
    }

    fn cloud_setup_current(&self, running: &Running) -> bool {
        running.endpoint
            == (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            )
            && self.live.status.is_connected()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == running.boot)
    }

    /// Once the setup tab exists, type the line that runs the script and
    /// show the tab. A reply from a replaced connection opens nothing.
    pub(crate) fn poll_cloud_setup_tab(&mut self, cx: &mut Context<Self>) {
        let Some(running) = &self.cloud_setup else {
            return;
        };
        if !self.cloud_setup_current(running) {
            let label = running.label.clone();
            self.cloud_setup = None;
            return self.fail_cloud_setup(&label, crate::Error::StaleConnection, cx);
        }
        let request = running.request.clone();
        let Some(result) = self.take_script_response(&request) else {
            return;
        };
        let Some(running) = self.cloud_setup.take() else {
            return;
        };
        let typed = result
            .map_err(crate::Error::WorktreeScriptsRequest)
            .and_then(|response| worktree_scripts::created_tab(&response))
            .and_then(|(tab, pane)| {
                let line =
                    worktree_scripts::command_line(worktree_scripts::ScriptKind::Setup, false);
                self.type_script_line(&running.boot, &pane, line)?;
                Ok(tab)
            });
        match typed {
            Ok(tab) => {
                tracing::info!(
                    category = "cloud_setup",
                    endpoint = %running.endpoint_id,
                    "A new cloud device's setup script started"
                );
                self.navigate_endpoint(&running.endpoint_id, NavigationTarget::Tab(&tab), cx);
            }
            Err(error) => self.fail_cloud_setup(&running.label, error, cx),
        }
    }

    pub(super) fn fail_cloud_setup(
        &mut self,
        label: &str,
        error: crate::Error,
        cx: &mut Context<Self>,
    ) {
        tracing::warn!(
            category = "cloud_setup",
            label,
            %error,
            "A new cloud device's setup script did not start"
        );
        // Kept on screen rather than flashed, as a worktree script's is.
        self.local_error = Some(format!(
            "The setup script for {label} did not start: {error}"
        ));
        cx.notify();
    }
}
