use super::*;
use crate::config::{UpdateChannel, preferences::Preference};

impl SettingsWindow {
    pub(super) fn render_update_controls(&self, cx: &mut Context<Self>) -> Div {
        let beta = self.config.updates.channel == UpdateChannel::Beta;
        self.control_card("Updates")
            .child(self.preference_switch(
                "settings-update-beta",
                "Install beta releases",
                beta,
                Preference::UpdateChannel(if beta {
                    UpdateChannel::Stable
                } else {
                    UpdateChannel::Beta
                }),
                cx,
            ))
            .child(self.control_note(
                "Betas ship before they are promoted to stable. Turning this off never downgrades; Homebrew installations always follow stable.",
            ))
    }
}
