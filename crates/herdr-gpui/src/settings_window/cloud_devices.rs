//! Settings > Cloud devices: one tab per cloud provider whose machines this
//! app uses as devices. Each provider's card keeps its own fields and account
//! state; this section only switches between them, so another provider adds a
//! tab and a card.

#[cfg(feature = "azure")]
mod azure;
#[cfg(feature = "boat")]
mod boat;
#[cfg(feature = "coder")]
mod coder;
#[cfg(feature = "daytona")]
mod daytona;
#[cfg(feature = "railway")]
mod railway;
mod setup;

#[cfg(feature = "azure")]
pub(super) use azure::AzureCard;
#[cfg(feature = "boat")]
pub(super) use boat::BoatCard;
#[cfg(feature = "coder")]
pub(super) use coder::CoderCard;
#[cfg(feature = "daytona")]
pub(super) use daytona::DaytonaCard;
#[cfg(feature = "railway")]
pub(super) use railway::RailwayCard;

use super::SettingsWindow;
use crate::{
    cloud::CloudProvider,
    config::{Config, corners},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

/// Open Settings > Cloud devices, on `provider`'s tab when the device picker's
/// row sets that provider up, or on the first tab for its general row.
// Add Device records the request instead in unit tests.
#[cfg_attr(test, allow(dead_code, unused_imports))]
pub(crate) fn open(
    source: WeakEntity<crate::HerdrWindow>,
    provider: Option<CloudProvider>,
    cx: &mut App,
) {
    super::open(source, cx);
    // Queued after `open`'s own deferred work, so the window exists by now.
    cx.defer(move |cx| {
        let Some(handle) = cx.default_global::<super::SettingsWindowHandle>().window else {
            return;
        };
        let _ = handle.update(cx, |view, window, cx| {
            if let Some(provider) = provider {
                view.cloud_tab = provider;
            }
            view.select_section(super::Section::CloudDevices, window, cx);
        });
    });
}

/// The provider whose tab shows first.
pub(super) fn first_tab() -> CloudProvider {
    // `cloud` builds always have a provider; see `main.rs`.
    CloudProvider::ALL[0]
}

/// A card's text input, styled like the rest of Settings.
fn input(
    placeholder: &str,
    text: &str,
    config: &Config,
    theme: &crate::config::Theme,
    cx: &mut Context<SettingsWindow>,
) -> Entity<SearchInput> {
    let input = cx.new(SearchInput::new);
    input.update(cx, |input, cx| {
        input.set_appearance(config.ui.clone(), theme.clone(), cx);
        input.set_placeholder(placeholder, cx);
        if !text.is_empty() {
            input.set_text_selected(text, cx);
        }
    });
    input
}

/// Follow a reloaded config in a card's inputs: a field still showing what
/// was loaded last takes the new value, and one the user has edited keeps the
/// edit, so a later Save neither writes stale values back nor drops typing.
#[cfg(any(
    feature = "azure",
    feature = "boat",
    feature = "coder",
    feature = "daytona"
))]
fn follow_config<const N: usize>(
    inputs: &[Entity<SearchInput>; N],
    previous: [&str; N],
    loaded: [&str; N],
    cx: &mut App,
) {
    for ((input, previous), loaded) in inputs.iter().zip(previous).zip(loaded) {
        if previous != loaded && input.read(cx).text().trim() == previous {
            input.update(cx, |input, cx| {
                if loaded.is_empty() {
                    input.clear(cx);
                } else {
                    input.set_text_selected(loaded, cx);
                }
            });
        }
    }
}

/// A fingerprint of the cloud devices `window` lists, which changes when one
/// is added or removed there, by the sidebar, a job, or auto-stop.
fn cloud_devices(window: &crate::HerdrWindow) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for endpoint in &window.endpoints {
        if matches!(
            endpoint.connection.target,
            herdr_client::ConnectTarget::Cloud { .. }
        ) {
            endpoint.id.hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl SettingsWindow {
    /// Build each provider's card on first view, then refresh what it reads.
    pub(super) fn open_cloud_devices(&mut self, cx: &mut Context<Self>) {
        for &provider in CloudProvider::ALL {
            match provider {
                #[cfg(feature = "azure")]
                CloudProvider::Azure => self.open_azure_card(cx),
                #[cfg(feature = "boat")]
                CloudProvider::Boat => self.open_boat_card(cx),
                #[cfg(feature = "coder")]
                CloudProvider::Coder => self.open_coder_card(cx),
                #[cfg(feature = "daytona")]
                CloudProvider::Daytona => self.open_daytona_card(cx),
                #[cfg(feature = "railway")]
                CloudProvider::Railway => self.open_railway_card(cx),
            }
        }
    }

    /// Called after the config file reloads, so each card follows an account
    /// that was just saved or edited by hand.
    pub(super) fn cloud_config_changed(&mut self, cx: &mut Context<Self>) {
        for &provider in CloudProvider::ALL {
            match provider {
                #[cfg(feature = "azure")]
                CloudProvider::Azure => self.azure_config_changed(cx),
                #[cfg(feature = "boat")]
                CloudProvider::Boat => self.boat_config_changed(cx),
                #[cfg(feature = "coder")]
                CloudProvider::Coder => self.coder_config_changed(cx),
                #[cfg(feature = "daytona")]
                CloudProvider::Daytona => self.daytona_config_changed(cx),
                #[cfg(feature = "railway")]
                CloudProvider::Railway => self.railway_config_changed(cx),
            }
        }
    }

    /// A cloud job finishing in the main window may have saved a device, so
    /// the cards read the device list and account again.
    pub(super) fn cloud_source_changed(
        &mut self,
        source: &Entity<crate::HerdrWindow>,
        cx: &mut Context<Self>,
    ) {
        let window = source.read(cx);
        let devices = cloud_devices(window);
        let jobs = &window.cloud_jobs;
        let (finished, revision) = (jobs.finished(), jobs.revision());
        if devices != self.cloud_devices_seen {
            self.cloud_devices_seen = devices;
            self.cloud_config_changed(cx);
        }
        if revision != self.cloud_jobs_drawn {
            self.cloud_jobs_drawn = revision;
            if self.section == super::Section::CloudDevices {
                cx.notify();
            }
        }
        if finished == self.cloud_jobs_seen {
            return;
        }
        self.cloud_jobs_seen = finished;
        self.cloud_config_changed(cx);
    }

    fn cloud_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut tabs = div().flex().flex_wrap().gap(px(8.));
        for &provider in CloudProvider::ALL {
            let key = provider.key();
            tabs = tabs.child(
                self.control_choice(
                    format!("cloud-tab-{key}"),
                    crate::cloud::name(provider),
                    self.cloud_tab == provider,
                    true,
                )
                .debug_selector(move || format!("cloud-tab-{key}"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cloud_tab = provider;
                    cx.notify();
                })),
            );
        }
        tabs
    }

    pub(super) fn render_cloud_devices(&self, cx: &mut Context<Self>) -> Div {
        let card = match self.cloud_tab {
            #[cfg(feature = "azure")]
            CloudProvider::Azure => self.render_azure_card(cx),
            #[cfg(feature = "boat")]
            CloudProvider::Boat => self.render_boat_card(cx),
            #[cfg(feature = "coder")]
            CloudProvider::Coder => self.render_coder_card(cx),
            #[cfg(feature = "daytona")]
            CloudProvider::Daytona => self.render_daytona_card(cx),
            #[cfg(feature = "railway")]
            CloudProvider::Railway => self.render_railway_card(cx),
        };
        div()
            .relative()
            .debug_selector(|| "cloud-accounts".into())
            .flex()
            .flex_col()
            .gap(px(16.))
            .min_w_0()
            .child(super::search::track_bounds("cloud-accounts".into()))
            .when_some(crate::cloud::unavailable(), |section, reason| {
                section.child(self.control_note(format!(
                    "{reason} Accounts can still be edited here for other systems."
                )))
            })
            .child(self.render_cloud_setup(cx))
            .when(CloudProvider::ALL.len() > 1, |section| {
                section.child(self.cloud_tabs(cx))
            })
            .child(card)
    }

    fn cloud_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        enabled: bool,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id.into())
            .flex_none()
            .px(px(12.))
            .py(px(6.))
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(theme.active)))
            })
            .when(!enabled, |button| button.opacity(0.5))
            .child(label.into())
    }

    /// A label over its input; the input draws its own field.
    fn cloud_field(&self, label: &'static str, input: AnyView) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .min_w_0()
            .child(div().text_color(rgb(self.theme.muted)).child(label))
            .child(input)
    }

    /// A saved device with its Remove button; removal leaves the machine
    /// itself untouched at the provider.
    #[cfg(any(
        feature = "boat",
        feature = "coder",
        feature = "daytona",
        feature = "railway"
    ))]
    fn cloud_device_row(
        &self,
        index: usize,
        device: &crate::cloud::SavedDevice,
        detail: String,
        ready: bool,
        forget: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        self.cloud_device_row_with(index, device, detail, ready, forget, [], cx)
    }

    /// `cloud_device_row` with provider `actions` before its Remove button.
    #[cfg(any(
        feature = "boat",
        feature = "coder",
        feature = "daytona",
        feature = "railway"
    ))]
    #[allow(clippy::too_many_arguments)]
    fn cloud_device_row_with(
        &self,
        index: usize,
        device: &crate::cloud::SavedDevice,
        detail: String,
        ready: bool,
        forget: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
        actions: impl IntoIterator<Item = AnyElement>,
        cx: &mut Context<Self>,
    ) -> Div {
        let key = device.provider.key();
        let id = device.id.clone();
        let remove = self
            .cloud_button(
                SharedString::from(format!("cloud-{key}-remove-{index}")),
                "Remove",
                ready,
            )
            .debug_selector(move || format!("cloud-{key}-remove-{index}"))
            .when(ready, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| forget(this, id.clone(), cx)))
            })
            .into_any_element();
        self.cloud_device_row_actions(device, detail, actions.into_iter().chain([remove]))
    }

    /// A saved device's label and `detail`, with `actions` at the end.
    #[cfg(any(
        feature = "azure",
        feature = "boat",
        feature = "coder",
        feature = "daytona",
        feature = "railway"
    ))]
    fn cloud_device_row_actions(
        &self,
        device: &crate::cloud::SavedDevice,
        detail: String,
        actions: impl IntoIterator<Item = AnyElement>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .child(
                div()
                    .min_w_0()
                    .child(div().truncate().child(device.label.clone()))
                    .child(
                        div()
                            .truncate()
                            .text_color(rgb(self.theme.muted))
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(8.))
                    .children(actions),
            )
    }
}
