//! The setup script new cloud devices run (see `crate::cloud_setup`): a card
//! choosing the agents every provider installs, with a preview of the user's
//! own lines, and the switch each provider's card shows to skip it once.
// i18n: migrated

use super::super::SettingsWindow;
use crate::{
    cloud::CloudProvider,
    cloud_setup::Agent,
    config::{Config, corners},
    i18n::fl,
};
use gpui::{prelude::*, *};

/// Lines of the user's own script the card previews.
const PREVIEW_LINES: usize = 6;

impl SettingsWindow {
    /// The script a new device from `provider` runs, when the card's switch
    /// leaves it on. A loaded config already composed every provider's.
    pub(super) fn cloud_setup_script(&self, provider: CloudProvider, run: bool) -> Option<String> {
        run.then(|| self.config.cloud.script(provider).ok().flatten())
            .flatten()
    }

    /// The switch that runs the setup script on the next device a provider's
    /// card makes; nothing when no script is configured for it.
    pub(super) fn cloud_setup_switch(
        &self,
        provider: CloudProvider,
        on: bool,
        enabled: bool,
        toggle: impl Fn(&mut Self) + 'static,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        self.cloud_setup_script(provider, true)?;
        let id = format!("cloud-{}-setup", provider.key());
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    self.control_switch(id, fl!("settings-cloud-setup-run"), on, enabled)
                        .when(enabled, |row| {
                            row.on_click(cx.listener(move |this, _, _, cx| {
                                toggle(this);
                                cx.notify();
                            }))
                        }),
                )
                .child(
                    self.control_note(crate::cloud_setup::summary(&self.config.cloud, provider)),
                ),
        )
    }

    /// The card choosing what every new cloud device runs.
    pub(super) fn render_cloud_setup(&self, cx: &mut Context<Self>) -> Div {
        let picked = self.config.cloud.shared_agents();
        // A click while a save runs would be dropped, so the switches wait.
        let ready = !self.busy();
        let mut card = self
            .cloud_card(fl!("settings-cloud-setup"))
            .relative()
            .debug_selector(|| "cloud-setup".into())
            .child(super::super::search::track_bounds("cloud-setup".into()))
            .child(self.control_note(fl!("settings-cloud-setup-note")));
        for agent in Agent::ALL.iter().copied() {
            let on = picked.contains(&agent);
            card = card.child(
                self.control_switch(
                    format!("cloud-setup-agent-{}", agent.key()),
                    fl!("settings-cloud-setup-install", agent = agent.label()),
                    on,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_cloud_agent(agent, cx);
                    }))
                }),
            );
        }
        card.child(self.cloud_setup_preview())
            .children(self.cloud_setup_edit())
    }

    fn toggle_cloud_agent(&mut self, agent: Agent, cx: &mut Context<Self>) {
        if self.busy() {
            return;
        }
        let mut agents = self.config.cloud.shared_agents();
        if agents.contains(&agent) {
            agents.retain(|picked| *picked != agent);
        } else {
            agents.push(agent);
        }
        self.save_native(move || Config::save_cloud_agents(&agents), cx);
    }

    fn cloud_setup_preview(&self) -> Div {
        let Some(script) = self.config.cloud.shared_setup().map(str::trim) else {
            return self.control_note(fl!("settings-cloud-setup-none"));
        };
        let lines: Vec<&str> = script.lines().collect();
        let more = lines.len().saturating_sub(PREVIEW_LINES);
        div()
            .debug_selector(|| "cloud-setup-preview".into())
            .flex()
            .flex_col()
            .rounded(px(corners::CONTROL))
            .bg(rgb(self.theme.active))
            .px(px(10.))
            .py(px(6.))
            .text_color(rgb(self.theme.muted))
            .children(
                lines
                    .into_iter()
                    .take(PREVIEW_LINES)
                    .map(|line| div().child(if line.is_empty() { " " } else { line }.to_owned())),
            )
            .when(more > 0, |preview| {
                preview.child(div().child(fl!("settings-cloud-setup-more", count = more)))
            })
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn cloud_setup_edit(&self) -> Option<Div> {
        let path = Config::local_path().ok()?;
        Some(
            div().flex().child(
                self.cloud_button("cloud-setup-edit", fl!("settings-cloud-setup-edit"), true)
                    .on_click(move |_, _, cx| {
                        super::super::pages::open_path(false, path.clone(), cx)
                    }),
            ),
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn cloud_setup_edit(&self) -> Option<Div> {
        None
    }
}

#[cfg(test)]
mod tests;
