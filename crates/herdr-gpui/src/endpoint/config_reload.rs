//! Config reloads for daemons whose surface this window may not be showing.
use super::Endpoint;
use herdr_client::Method;

impl Endpoint {
    /// Asks this endpoint's daemon to reread its config file.
    ///
    /// Herdr refuses every command except `client_shell.surface.set` from a
    /// connection whose surface is inactive, as every endpoint but the shown
    /// one is. The request is therefore held until this connection's next
    /// activation is queued, rather than sent to be refused.
    pub(crate) fn reload_config(&mut self) -> herdr_client::Result<()> {
        self.config_reload_pending = true;
        self.flush_config_reload()
    }

    /// Sends a held [`Self::reload_config`] once this connection's surface has
    /// been requested. The daemon handles a connection's commands in order, so
    /// the reload reaches it after the activation.
    ///
    /// A full queue is not a failure: the reload stays held and a later poll
    /// sends it once the queue drains, so no error outlives that retry.
    pub(crate) fn flush_config_reload(&mut self) -> herdr_client::Result<()> {
        if !self.config_reload_pending || !self.initial_surface || !self.live.status.is_connected()
        {
            return Ok(());
        }
        let (Some(handle), Some(snapshot)) = (&self.connection.handle, &self.live.snapshot) else {
            return Ok(());
        };
        match handle.request(
            &snapshot.boot_id,
            Method::ServerReloadConfig,
            serde_json::json!({}),
        ) {
            Err(herdr_client::Error::Full) => Ok(()),
            // Any other failure would only repeat on every poll.
            result => {
                self.config_reload_pending = false;
                result.map(drop)
            }
        }
    }
}
