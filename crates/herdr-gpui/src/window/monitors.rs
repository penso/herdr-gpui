//! The background monitors the window polls as it ticks: plan usage, system
//! load, listening ports, agent checkpoints, and port forwards, each on the
//! hosts it watches.

use super::{Flash, HerdrWindow};
use gpui::Context;

impl HerdrWindow {
    /// Plan usage follows the selected host: a remote host reports its own
    /// agents' sign-ins, never this machine's.
    pub(super) fn update_usage(&mut self, cx: &mut Context<Self>) -> bool {
        let host = self
            .config
            .usage
            .show
            .then(|| self.endpoints.get(self.selected_endpoint))
            .flatten()
            .and_then(|endpoint| crate::usage::Host::of(&endpoint.connection.target));
        let granted = crate::usage::KeychainGrants::granted(cx);
        let changed = self.usage.poll(
            host,
            &self.config.usage,
            &granted,
            self.config_load_revision,
            self.active,
            std::time::Instant::now(),
        );
        // A denied prompt takes the grant back, so a later launch does not
        // ask again in the background.
        for provider in self.usage.take_denied() {
            crate::usage::KeychainGrants::revoke(provider, cx);
        }
        changed
    }

    /// CPU and memory are sampled for every enabled host, while they are
    /// shown, a Devices overview is open, or a host picker ranks hosts by them.
    pub(super) fn update_system_load(&mut self) -> bool {
        if !self.config.show_system_load
            && !self.devices_overview_open()
            && !self.overview_shown()
            && !self.dispatch_sampling()
            && !self.orchestrator_sampling
        {
            return self.system_load.poll(Vec::new());
        }
        let hosts = self.connected_hosts(crate::usage::Host::load_of);
        self.system_load.poll(hosts)
    }

    /// Listening ports are scanned on the same hosts as CPU and memory.
    pub(super) fn update_listening_ports(&mut self) -> bool {
        if !self.config.show_listening_ports {
            self.tunnels.close_all();
            return self.listening_ports.poll(Vec::new());
        }
        let hosts = self.watched_hosts();
        let changed = self.listening_ports.poll(hosts);
        // A tunnel lives as long as its remote port is listed at the address
        // it dials and its SSH child runs; either ending removes its close
        // button.
        let ports = &self.listening_ports;
        let closed = self.tunnels.retain(|key| ports.tunnels_to(key));
        changed || closed
    }

    /// The machines background monitors watch: this one always, a remote
    /// host while it is connected, so a dropped host is not dialled every
    /// few seconds.
    pub(super) fn watched_hosts(&self) -> Vec<crate::usage::Host> {
        self.connected_hosts(crate::usage::Host::of)
    }

    /// The hosts `host` finds for the enabled endpoints `watched_hosts` would
    /// watch. A cloud machine is only reached while it is connected, so a
    /// stopped one is never woken by a monitor.
    fn connected_hosts(
        &self,
        host: fn(&herdr_client::ConnectTarget) -> Option<crate::usage::Host>,
    ) -> Vec<crate::usage::Host> {
        self.endpoints
            .iter()
            .enumerate()
            .filter(|(index, endpoint)| {
                let live = if *index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                endpoint.enabled
                    && (live.status.is_connected() || !endpoint.connection.target.is_remote())
            })
            .filter_map(|(_, endpoint)| host(&endpoint.connection.target))
            .collect()
    }

    /// Agent turns are watched on every enabled, connected host this client
    /// may run Git on, and finished restores are reported wherever the user is.
    pub(super) fn update_checkpoints(&mut self, cx: &mut Context<Self>) {
        self.close_stale_checkpoints();
        let enabled = self.config.agent_checkpoints;
        let mut watched = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let (true, true, Some(host), Some(snapshot)) = (
                enabled,
                endpoint.enabled && live.status.is_connected(),
                crate::checkpoint::host_for(&endpoint.connection.target, live),
                live.snapshot.as_ref(),
            ) else {
                continue;
            };
            self.checkpoints.observe(&endpoint.id, &host, snapshot);
            watched.push(endpoint.id.as_str());
        }
        self.checkpoints
            .retain_endpoints(|endpoint| watched.contains(&endpoint));
        let (changed, restored) = self.checkpoints.poll();
        for restored in restored {
            let flash = match restored.result {
                Ok(()) => Flash::success(format!(
                    "Restored a checkpoint of {}",
                    restored.checkout.branch
                )),
                Err(error) => Flash::warning(format!(
                    "Checkpoint not restored: {}",
                    crate::checkpoint::describe(&error)
                )),
            };
            self.show_flash(flash, cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Forwards outlive a dropped connection, since SSH may still reach the
    /// host, but end once their host is removed or disabled. A report the
    /// user did not just ask for is flashed.
    pub(crate) fn update_port_forwards(&mut self, cx: &mut Context<Self>) {
        let endpoints = &self.endpoints;
        let mut changed = self.port_forwards.retain_hosts(|target| {
            endpoints.iter().any(|endpoint| {
                endpoint.enabled
                    && endpoint
                        .saved_ssh()
                        .is_some_and(|(saved, _)| saved == target)
            })
        });
        let notices = self.port_forwards.poll();
        if let Some(notice) = notices.last() {
            let flash = match notice {
                crate::port_forward::Notice::Listening { .. } => Flash::success(notice.text()),
                crate::port_forward::Notice::Ended { .. } => Flash::warning(notice.text()),
            };
            self.show_flash(flash, cx);
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// The machine the selected endpoint runs on, for its load.
    pub(crate) fn selected_host(&self) -> Option<crate::usage::Host> {
        self.endpoints
            .get(self.selected_endpoint)
            .and_then(|endpoint| crate::usage::Host::load_of(&endpoint.connection.target))
    }

    /// Update cloud activity timestamps for active cloud device connections.
    #[cfg(feature = "azure")]
    pub(super) fn update_cloud_activity(&mut self) {
        for endpoint in &self.endpoints {
            if let herdr_client::ConnectTarget::Cloud {
                provider: herdr_client::CloudProvider::Azure,
                id,
                ..
            } = &endpoint.connection.target
                && endpoint.live.status.is_connected()
            {
                crate::azure::record_activity(id);
            }
        }
    }
}
