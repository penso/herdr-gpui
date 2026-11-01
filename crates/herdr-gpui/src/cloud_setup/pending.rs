//! Setup scripts waiting for their device to be shown. A new device is not
//! opened for the user, who chooses it in the device picker when ready, maybe
//! in another window, so the queue is app-wide and waits long. Whichever
//! window shows the device first runs the script, once.

use crate::{HerdrWindow, i18n::fl, window::Flash};
#[cfg(test)]
use gpui::App;
use gpui::{Context, Global};
use herdr_client::{CloudProvider, ConnectTarget};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How long a new device's script waits to be shown.
const WAIT: Duration = Duration::from_secs(30 * 60);
/// Scripts kept waiting at once.
const MAX_WAITING: usize = 8;

/// A device's setup script, waiting for a window to show the device.
#[derive(Debug, Clone)]
pub(crate) struct Pending {
    pub(crate) endpoint_id: String,
    pub(crate) label: String,
    pub(crate) provider: CloudProvider,
    pub(crate) script: String,
    until: Instant,
}

impl Pending {
    pub(crate) fn new(
        endpoint_id: String,
        label: String,
        provider: CloudProvider,
        script: String,
        now: Instant,
    ) -> Self {
        Self {
            endpoint_id,
            label,
            provider,
            script,
            until: now + WAIT,
        }
    }
}

#[derive(Default)]
pub(crate) struct PendingSetups(VecDeque<Pending>);

impl Global for PendingSetups {}

impl PendingSetups {
    #[cfg(test)]
    pub(crate) fn waiting(cx: &App) -> Vec<String> {
        cx.try_global::<Self>()
            .map(|setups| setups.0.iter().map(|p| p.endpoint_id.clone()).collect())
            .unwrap_or_default()
    }
}

fn gave_up(label: &str) -> Flash {
    Flash::warning(fl!("cloud-setup-gave-up", device = label))
}

impl HerdrWindow {
    /// Run the configured setup script again on the cloud device `id`, as
    /// its menu and `herdr-gpui cloud setup` ask: queue it, then show the
    /// device, whose tab opens as a new device's would.
    pub(crate) fn rerun_cloud_setup(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> crate::Result<()> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.id == id && endpoint.enabled)
            .ok_or_else(|| super::Error::NoDevice(id.to_owned()))?;
        let ConnectTarget::Cloud { provider, .. } = endpoint.connection.target else {
            return Err(super::Error::NoDevice(id.to_owned()).into());
        };
        let script = self
            .config
            .cloud
            .script(provider)?
            .ok_or(super::Error::NoScript(crate::cloud::name(provider)))?;
        let pending = Pending::new(
            id.to_owned(),
            endpoint.label.clone(),
            provider,
            script,
            Instant::now(),
        );
        self.queue_cloud_setup(pending, cx);
        self.select_endpoint(id, cx);
        Ok(())
    }

    /// Queue `pending` for whichever window shows its device first. A device
    /// queued again replaces its earlier script; beyond [`MAX_WAITING`] the
    /// oldest is given up on, with a word.
    pub(crate) fn queue_cloud_setup(&mut self, pending: Pending, cx: &mut Context<Self>) {
        let setups = &mut cx.default_global::<PendingSetups>().0;
        setups.retain(|waiting| waiting.endpoint_id != pending.endpoint_id);
        setups.push_back(pending);
        let dropped = (setups.len() > MAX_WAITING)
            .then(|| setups.pop_front())
            .flatten();
        if let Some(dropped) = dropped {
            self.show_flash(gave_up(&dropped.label), cx);
        }
        cx.notify();
    }

    /// Start the shown device's waiting script once it is connected, has its
    /// shell surface, lists a workspace, and no other script is starting. Expired ones are dropped.
    pub(crate) fn poll_cloud_setup(&mut self, now: Instant, cx: &mut Context<Self>) {
        if cx
            .try_global::<PendingSetups>()
            .is_none_or(|setups| setups.0.is_empty())
        {
            return;
        }
        let setups = &mut cx.global_mut::<PendingSetups>().0;
        let expired: Vec<Pending> = setups
            .iter()
            .filter(|setup| now >= setup.until)
            .cloned()
            .collect();
        setups.retain(|setup| now < setup.until);
        for setup in expired {
            tracing::warn!("a new cloud device was never shown for its setup script");
            self.show_flash(gave_up(&setup.label), cx);
        }
        if self.script_slot_busy() || !self.live.status.is_connected() {
            return;
        }
        let Some(snapshot) = self.live.snapshot.as_ref() else {
            return;
        };
        // The daemon refuses `tab.create` from a client with no active shell
        // surface, and a device just shown is connected before it has one.
        // Taking the script then would fail it for good, so it keeps waiting.
        if !self.live.surface_ready() {
            return;
        }
        let workspace = snapshot
            .focused_workspace_id
            .as_ref()
            .filter(|id| snapshot.workspaces.iter().any(|w| &w.workspace_id == *id))
            .or_else(|| snapshot.workspaces.first().map(|w| &w.workspace_id))
            .cloned();
        let Some(workspace) = workspace else {
            return;
        };
        let boot = snapshot.boot_id.clone();
        let shown = &self.endpoints[self.selected_endpoint].id;
        let setups = &mut cx.global_mut::<PendingSetups>().0;
        let Some(index) = setups.iter().position(|setup| &setup.endpoint_id == shown) else {
            return;
        };
        let Some(pending) = setups.remove(index) else {
            return;
        };
        let label = pending.label.clone();
        if let Err(error) = self.start_cloud_setup(pending, &workspace, boot) {
            self.fail_cloud_setup(&label, error, cx);
        }
    }
}
