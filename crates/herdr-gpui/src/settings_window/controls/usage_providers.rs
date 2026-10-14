//! Settings > Status bar > Usage services: which AI services Herdr reads plan
//! usage for. Disabled services permit only installation metadata checks,
//! never credential reads or provider requests.
use super::*;
use crate::{
    HerdrWindow,
    config::{preferences::Preference, usage_providers::ProviderEdit},
    usage,
};

impl SettingsWindow {
    pub(in crate::settings_window) fn sync_usage_provider_controls(
        &mut self,
        source: &Entity<HerdrWindow>,
        refresh: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_changed = {
            let source = source.read(cx);
            source
                .endpoints
                .get(source.selected_endpoint)
                .is_some_and(|endpoint| {
                    source
                        .usage
                        .discovery_target_changed(&endpoint.connection.target)
                })
        };
        if refresh
            || target_changed
            || source
                .read(cx)
                .integration_scope_changed(self.usage_provider_scope.as_ref())
        {
            self.usage_provider_scope = source.read(cx).integration_scope().ok();
            source.update(cx, |source, cx| {
                source.load_integrations(cx);
                if let Some(endpoint) = source.endpoints.get(source.selected_endpoint) {
                    source.usage.request_discovery(&endpoint.connection.target);
                }
            });
        }
        let source = source.read(cx);
        let mut selected = source.installed_usage_providers();
        if let Some(endpoint) = source.endpoints.get(source.selected_endpoint) {
            let target = &endpoint.connection.target;
            let installed = source.usage.installed_for_target(target);
            for (displayed, installed) in selected.iter_mut().zip(installed) {
                *displayed |= installed;
            }
            if let Some(entry) = source.usage.for_target(target) {
                for provider in entry.detected() {
                    if let Some(index) = usage::providers().position(|known| known == provider) {
                        selected[index] = true;
                    }
                }
            }
        }
        for (provider, displayed) in usage::providers().zip(selected.iter_mut()) {
            *displayed |= source.config.usage.settings(provider).is_some()
                || source.config.usage.hidden(provider);
        }
        if selected == self.usage_providers {
            return false;
        }
        self.usage_providers = selected;
        true
    }

    pub(super) fn render_usage_provider_controls(&self, cx: &mut Context<Self>) -> Div {
        let config = &self.config.usage;
        let on = config.show;
        let interactive = on && !self.busy();
        let mut selected = self.usage_providers;
        // Saved preferences are app-level evidence, not a validated host sign-in.
        // Derive these at render time so settings loads need no source notification.
        for (provider, displayed) in usage::providers().zip(selected.iter_mut()) {
            *displayed |= config.settings(provider).is_some() || config.hidden(provider);
        }
        let mut total = 0;
        let mut chips = div().flex().flex_wrap().gap(px(8.));
        let mut enabled = 0;
        for (provider, displayed) in usage::providers().zip(selected) {
            if !displayed {
                continue;
            }
            total += 1;
            let id = provider.id();
            let active = !config.hidden(provider);
            enabled += usize::from(active);
            let edit = if active {
                ProviderEdit::Disable(id)
            } else {
                ProviderEdit::Enable(id)
            };
            let selector = format!("settings-usage-provider-{id}");
            chips = chips.child(
                self.control_choice(selector.clone(), provider.name(), active, interactive)
                    .debug_selector(move || selector.clone())
                    .map(|chip| {
                        #[cfg(all(feature = "integration-test", target_os = "macos"))]
                        let chip = chip
                            .relative()
                            .child(crate::settings_window::native::usage_services::probe(id));
                        chip
                    })
                    .when(interactive, |chip| {
                        chip.on_click(cx.listener(move |this, _, _, cx| {
                            this.save_preference(Preference::UsageProviders(edit), cx);
                        }))
                    }),
            );
        }
        self.control_card("Usage services")
            .child(self.control_note(
                "Choose which installed, detected, or saved services appear in the status bar. A service that is off is never contacted and its sign-in is not read. Services appear here from saved provider settings or off preferences, or when a supported CLI or sign-in file is found, or usage or a sign-in permission request is detected, on the selected host. Saved settings and finding a file do not validate the sign-in.",
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(self.bulk_choice(
                        "settings-usage-providers-all",
                        "Turn all on",
                        ProviderEdit::EnableMany(selected),
                        interactive && enabled < total,
                        cx,
                    ))
                    .child(self.bulk_choice(
                        "settings-usage-providers-none",
                        "Turn all off",
                        ProviderEdit::DisableMany(selected),
                        interactive && enabled > 0,
                        cx,
                    ))
                    .child(
                        self.control_note(format!("{enabled} of {total} on"))
                            .debug_selector(|| "settings-usage-providers-count".into()),
                    ),
            )
            .child(chips)
            .when(total == 0, |card| {
                card.child(
                    self.control_note(
                        "No installed, detected, or saved usage services on this host yet. Install a supported CLI, or sign in and refresh Usage to see the service here.",
                    )
                    .debug_selector(|| "settings-usage-providers-empty".into()),
                )
            })
            .when(!on, |card| {
                card.child(self.control_note("Turn Usage on above to choose services."))
            })
    }

    /// Changes only the installed, detected, or saved services displayed here.
    fn bulk_choice(
        &self,
        id: &'static str,
        label: &'static str,
        edit: ProviderEdit,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let enabled = enabled && !self.busy();
        self.control_choice(id, label, false, enabled)
            .debug_selector(move || id.into())
            .when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    this.save_preference(Preference::UsageProviders(edit), cx);
                }))
            })
    }
}

#[cfg(test)]
mod tests;
