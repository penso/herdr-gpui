//! The Nearby devices list under the SSH target field: a sliding bar while
//! sources search, then what they found, minus devices already saved, and
//! filtered by what the field holds.

use super::{Discovery, Source};
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    progress::{self, Progress},
};
use gpui::{prelude::*, *};

/// Suggestion rows shown before the list scrolls.
pub(super) const MAX_VISIBLE_ROWS: f32 = 4.5;

impl HerdrWindow {
    pub(in crate::menu::devices) fn render_device_suggestions(
        &self,
        setup: &super::super::add_device::Setup,
        discovery: &Discovery,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let text = setup.fields[0].read(cx).text().to_owned();
        let completion = self.device_completion(setup, &text);
        let status = if discovery.awaiting_local_network {
            fl!("devices-discover-waiting")
        } else if discovery.searching() {
            fl!(
                "devices-discover-searching",
                sources = sources(&discovery.pending)
            )
        } else {
            fl!("devices-discover-found", count = completion.unsaved)
        };
        let mut header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_none().child(fl!("devices-discover-heading")))
            .child(
                div()
                    .debug_selector(|| "device-discovery-status".into())
                    .flex_1()
                    // Wraps rather than cutting a longer language short.
                    .min_w_0()
                    .text_color(rgb(theme.muted))
                    .child(status),
            );
        if !discovery.searching() {
            header = header.child(
                div()
                    .id("device-discovery-again")
                    .debug_selector(|| "device-discovery-again".into())
                    .flex_none()
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child(fl!("devices-discover-again"))
                    .on_click(cx.listener(|this, _, _, cx| this.start_device_discovery(cx))),
            );
        }
        let mut view = div()
            .debug_selector(|| "device-discovery".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(header);
        if discovery.searching() {
            view = view.child(progress::bar(
                "device-discovery-progress",
                Progress::Busy,
                crate::menu::accent(theme).into(),
                rgb(theme.active).into(),
            ));
        }
        // A long list scrolls by itself rather than pushing the label and
        // session fields out of the dialog; half a row shows there is more.
        let row = (self.config.ui.size * 1.6 + 12.).ceil();
        let mut list = div()
            .id("device-suggestions")
            .debug_selector(|| "device-suggestions".into())
            .flex_none()
            .flex()
            .flex_col()
            .max_h(px(row * MAX_VISIBLE_ROWS))
            .overflow_y_scroll()
            .track_scroll(&setup.suggestion_scroll);
        for (index, (suggestion, target)) in completion
            .rows
            .iter()
            .zip(completion.fills.iter().cloned())
            .enumerate()
        {
            let name = suggestion.name.clone();
            list = list.child(
                div()
                    .id(("device-suggestion", index))
                    .debug_selector(move || format!("device-suggestion-{index}"))
                    .flex_none()
                    .h(px(row))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .when(completion.highlight == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(220.))
                            .truncate()
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(theme.subtext()))
                            .child(target.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child(sources(&suggestion.sources)),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.use_device_suggestion(&target, &name, window, cx);
                    })),
            );
        }
        view = view.child(list);
        for (source, error) in &discovery.failures {
            view = view.child(div().text_color(rgb(theme.muted)).child(fl!(
                "devices-discover-failed",
                source = source.name(),
                error = error.to_string(),
            )));
        }
        // While waiting the system alert may be up already; after a refusal
        // the setting is the only way. "Search again" then checks anew.
        let refused = discovery
            .failures
            .iter()
            .any(|(_, error)| matches!(error, crate::Error::LocalNetworkDenied));
        if discovery.awaiting_local_network || refused {
            view =
                view.children(self.local_network_settings_button("device-discovery-settings", cx));
        }
        view
    }

    /// Fill the form from a suggestion. A label the user typed is kept; one an
    /// earlier suggestion filled in, and not edited since, is replaced.
    pub(in crate::menu::devices) fn use_device_suggestion(
        &mut self,
        target: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(setup) = &mut self.menu.device_setup else {
            return;
        };
        if !matches!(setup.step, super::super::add_device::Step::Form) {
            return;
        }
        let [target_field, label_field, _] = setup.fields.clone();
        target_field.update(cx, |field, cx| field.set_text_selected(target, cx));
        let label = label_field.read(cx).text().trim();
        if label.is_empty() || setup.suggested_label.as_deref() == Some(label) {
            // Recorded first, so the field's change event sees its own fill.
            setup.suggested_label = Some(name.to_owned());
            label_field.update(cx, |field, cx| field.set_text_selected(name, cx));
        }
        window.focus(&target_field.read(cx).focus.clone(), cx);
        self.set_device_error(None);
        cx.notify();
    }
}

/// `Bonjour`, `Tailscale and Bonjour`, `SSH config, Tailscale, and Bonjour`.
pub(super) fn sources(sources: &[Source]) -> String {
    let names: Vec<String> = sources.iter().map(|source| source.name()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => fl!(
            "devices-discover-list-two",
            first = first.as_str(),
            second = second.as_str()
        ),
        [first, middle @ .., last] => {
            let start = middle.iter().fold(first.clone(), |start, next| {
                fl!(
                    "devices-discover-list-middle",
                    start = start,
                    next = next.as_str()
                )
            });
            fl!(
                "devices-discover-list-end",
                start = start,
                last = last.as_str()
            )
        }
    }
}
