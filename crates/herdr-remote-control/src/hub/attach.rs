//! Sizing a terminal to a phone: a direct attach to the pane's terminal,
//! which Herdr resizes to the attach's size for as long as it is held.

use super::{ApiMethod, Error, Hub, Result};
use herdr_client::{TerminalAttach, terminal_attach};
use serde_json::json;
use std::time::Duration;

/// How long a refused attach waits before its one retry: the attach a
/// view just dropped may still be detaching.
const RETRY: Duration = Duration::from_millis(250);

impl Hub {
    /// Holds `pane`'s terminal at `cols` by `rows` until the attach is
    /// dropped. Never takes the terminal from another direct client.
    pub(crate) async fn attach_terminal(
        &self,
        pane: &str,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalAttach> {
        let answer = self
            .api
            .call(ApiMethod::PaneGet, json!({ "pane_id": pane }))
            .await?;
        let terminal = answer["pane"]["terminal_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or(Error::Response)?
            .to_owned();
        let target = self.api.target();
        let open = move || TerminalAttach::open(&target, &terminal, cols, rows);
        let mut attempts = 0;
        loop {
            attempts += 1;
            let attempt = open.clone();
            let result = tokio::task::spawn_blocking(attempt)
                .await
                .map_err(|_| Error::NotConnected)?;
            match result {
                Err(herdr_client::Error::TerminalAttach(terminal_attach::Error::Refused(_)))
                    if attempts == 1 =>
                {
                    tokio::time::sleep(RETRY).await;
                }
                result => return Ok(result?),
            }
        }
    }
}
