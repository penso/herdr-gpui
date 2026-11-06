//! Chats among core terminals: those carrying the endpoint label. The
//! label is daemon data, so only the id is taken from it, and a terminal on
//! another machine this core added (`box/t7`) is out of reach.

use super::{pane_id, projection};
use crate::hub::{Layout, TerminalPane, layout::Chat};
use sheprd_chat_protocol::{EndpointId, TOKEN};
use sheprd_core_client::wire::Terminal;

/// Moves every chat pane out of its terminal tab into its workspace's
/// chats, as Herdr 1 lists a chat apart from the terminals.
pub(super) fn lift(layout: &mut Layout, boot: &str, records: &[Terminal]) {
    for workspace in &mut layout.workspaces {
        let mut chats = Vec::new();
        workspace.terminals.retain_mut(|terminal| {
            let tab = terminal.tab.clone();
            terminal.panes.retain(|pane| {
                let chat = chat(pane, &tab, boot, records);
                let keep = chat.is_none();
                chats.extend(chat);
                keep
            });
            let panes = &terminal.panes;
            if let Some(agent) = &terminal.agent
                && !panes.iter().any(|pane| pane.pane == agent.pane)
            {
                terminal.agent = None;
            }
            !terminal.panes.is_empty()
        });
        workspace.chats.extend(chats);
    }
}

fn chat(pane: &TerminalPane, tab: &str, boot: &str, records: &[Terminal]) -> Option<Chat> {
    let record = records
        .iter()
        .find(|record| pane_id(boot, &record.address) == pane.pane)
        .filter(|record| !record.address.contains('/'))?;
    let endpoint: EndpointId = record.labels.get(TOKEN)?.parse().ok()?;
    let status = record.status.as_ref();
    Some(Chat {
        agent: status
            .map(|status| status.app.clone())
            .filter(|name| !name.is_empty()),
        endpoint,
        pane: pane.pane.clone(),
        status: projection::status(record),
        tab: tab.to_owned(),
        title: pane.label.clone().unwrap_or_default(),
    })
}
