//! The behaviour pages: one module per Settings page, each rebuilt from the
//! shared row and control builders in `ui`.
mod configuration;
mod fonts;
mod general;
mod indicators;
pub(in crate::settings_window) mod notifications;
pub(in crate::settings_window) mod remote_control;
mod sidebar;
mod status_bar;
mod tabs;
mod theme;
mod window;

use super::{SettingsWindow, inputs::DropdownField, ui};
pub(super) use fonts::size_control;
use gpui::{prelude::*, *};
pub(super) use sidebar::Spacing;
pub(super) use window::padding_ids;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(in crate::settings_window) use configuration::open_path;

pub(in crate::settings_window) const SHARED_READ_ONLY: &str =
    "Herdr's settings are read-only on this platform.";
const SHARED_UNAVAILABLE: &str = "Herdr's settings are unavailable. Reload to retry.";

/// The text on an override dropdown: the local choice, or what Herdr says.
pub(super) fn override_label(shared_value: &str, local: Option<&str>) -> String {
    match local {
        Some(choice) => choice.to_owned(),
        None => format!("Use Herdr setting \u{b7} {shared_value}"),
    }
}

impl SettingsWindow {
    /// Disables a row that edits Herdr's shared file when that file cannot be
    /// written, saying why instead of showing the description.
    fn gate_shared(&self, row: ui::Row) -> ui::Row {
        match self.shared_gate() {
            Some(reason) => row.disabled(reason),
            None => row,
        }
    }

    /// Why rows that edit Herdr's shared file are disabled, if they are.
    pub(in crate::settings_window) fn shared_gate(&self) -> Option<&'static str> {
        if !cfg!(unix) {
            Some(SHARED_READ_ONLY)
        } else if self.shared.is_none() {
            Some(SHARED_UNAVAILABLE)
        } else {
            None
        }
    }

    /// A dropdown whose first entry follows Herdr and whose other entries are
    /// local overrides. `local` is the index into `choices` of the current one.
    fn override_dropdown(
        &self,
        field: DropdownField,
        shared_value: &str,
        choices: &[&str],
        local: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = override_label(shared_value, local.and_then(|i| choices.get(i).copied()));
        let options: Vec<SharedString> = std::iter::once(override_label(shared_value, None))
            .chain(choices.iter().map(|choice| (*choice).to_owned()))
            .map(SharedString::from)
            .collect();
        let selected = local.map_or(0, |index| index + 1);
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::dropdown(
            &self.theme,
            field.selector(),
            label,
            true,
            true,
            self.dropdown.as_ref().filter(|open| open.field == field),
            move |window, cx| {
                let options = options.clone();
                let _ = opener.update(cx, |this, cx| {
                    this.open_dropdown(field, options, selected, window, cx)
                });
            },
            move |index, window, cx| {
                let _ = picker.update(cx, |this, cx| {
                    this.pick_dropdown(field, index, cx);
                    window.focus(&this.focus, cx);
                });
            },
            move |window, cx| {
                let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
            },
        )
    }
}

/// A callback for a control that reports a value rather than a click.
fn on_value<T: 'static>(
    cx: &Context<SettingsWindow>,
    act: impl Fn(&mut SettingsWindow, T, &mut Window, &mut Context<SettingsWindow>) + 'static,
) -> impl Fn(T, &mut Window, &mut App) + 'static {
    let view = cx.entity().downgrade();
    move |value, window, cx| {
        let _ = view.update(cx, |this, cx| act(this, value, window, cx));
    }
}

pub(super) const CLIPBOARD_POSITIONS: [(crate::config::ClipboardToastPosition, &str); 6] = {
    use crate::config::ClipboardToastPosition as P;
    [
        (P::TopLeft, "Top left"),
        (P::TopCenter, "Top center"),
        (P::TopRight, "Top right"),
        (P::BottomLeft, "Bottom left"),
        (P::BottomCenter, "Bottom center"),
        (P::BottomRight, "Bottom right"),
    ]
};

pub(super) const TOAST_POSITIONS: [(herdr_client::protocol::ToastHerdrPosition, &str); 4] = {
    use herdr_client::protocol::ToastHerdrPosition as P;
    [
        (P::TopLeft, "Top left"),
        (P::TopRight, "Top right"),
        (P::BottomLeft, "Bottom left"),
        (P::BottomRight, "Bottom right"),
    ]
};

/// "On" or "Off" for a boolean override dropdown.
pub(super) fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}
